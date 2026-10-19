//! 轮询引擎启停状态、音频播放进度和全局输出设备名称。

use super::*;
use crate::engine::EngineStatus;
use slint::Model;
use slint::{Timer, TimerMode};
use std::cell::Cell;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub(super) fn start_status_timer(ui: &MainWindow, state: &AppState) -> Timer {
    let ui_weak = ui.as_weak();
    let state = state.clone();
    let last_dest_idx = Cell::new(ui.get_device_index());
    let last_diag_unseen = Cell::new(0usize);
    let mut source_meter_history = SourceMeterHistory::default();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        apply_list_refresh(&ui, &state);
        update_engine_status(&ui, &state);
        super::sources::update_dynamic(&ui, &state);
        update_source_meter(&ui, &state, &mut source_meter_history);
        update_player_progress(&ui, &state);
        update_device_label(&ui, &state, &last_dest_idx);
        ui.set_master_volume_db(format_gain_db(state.engine.master_volume()).into());
        super::diagnostics::sync(&ui, &last_diag_unseen);
    });
    timer
}
const PEAK_HISTORY_LENGTH: usize = 32;

#[derive(Default)]
struct SourceMeterHistory {
    kinds: Vec<i32>,
    titles: Vec<slint::SharedString>,
    samples: Vec<f32>,
    master_samples: Vec<f32>,
}

fn update_source_meter(ui: &MainWindow, state: &AppState, history: &mut SourceMeterHistory) {
    let kinds = ui.get_source_kinds();
    let titles = ui.get_source_titles();
    let row_count = kinds.row_count().min(titles.row_count());
    let rows_changed = history.kinds.len() != row_count
        || history.titles.len() != row_count
        || (0..row_count).any(|index| {
            history.kinds[index] != kinds.row_data(index).unwrap_or_default()
                || history.titles[index] != titles.row_data(index).unwrap_or_default()
        });
    if rows_changed {
        history.kinds = (0..row_count)
            .map(|index| kinds.row_data(index).unwrap_or_default())
            .collect();
        history.titles = (0..row_count)
            .map(|index| titles.row_data(index).unwrap_or_default())
            .collect();
        history.samples.clear();
        history.samples.resize(row_count * PEAK_HISTORY_LENGTH, 0.0);
    }

    let cache = state.cache.borrow();
    let mut process_slot = 0usize;
    let mut peak_dbs = Vec::with_capacity(row_count);
    let mut source_history_changed = rows_changed;
    for index in 0..row_count {
        let peak = match history.kinds[index] {
            0 => {
                let mut value = 0.0;
                while process_slot < cache.selected_pids.len() {
                    let slot = process_slot;
                    let pid = cache.selected_pids[slot];
                    process_slot += 1;
                    if cache.processes.iter().any(|process| process.pid == pid) {
                        value = state.engine.process_peak(slot);
                        break;
                    }
                }
                value
            }
            1 => state.engine.sfx_peak(),
            2 => state.engine.mic_peak(),
            _ => 0.0,
        };
        peak_dbs.push(format_peak_db(peak).into());
        let offset = index * PEAK_HISTORY_LENGTH;
        let segment_was_nonzero = history.samples[offset..offset + PEAK_HISTORY_LENGTH]
            .iter()
            .any(|sample| *sample != 0.0);
        source_history_changed |= peak != 0.0 || segment_was_nonzero;
        history
            .samples
            .copy_within(offset + 1..offset + PEAK_HISTORY_LENGTH, offset);
        history.samples[offset + PEAK_HISTORY_LENGTH - 1] = peak;
    }
    drop(cache);
    if source_history_changed {
        ui.set_source_meter_history(ModelRc::new(VecModel::from(history.samples.clone())));
        ui.set_source_meter_dbs(ModelRc::new(VecModel::from(peak_dbs)));
    }
    let master_peak = state.engine.route_peak();
    let master_was_nonzero = history.master_samples.iter().any(|sample| *sample != 0.0);
    let master_changed = master_peak != 0.0 || master_was_nonzero;
    history.master_samples.push(master_peak);
    if history.master_samples.len() > PEAK_HISTORY_LENGTH {
        history.master_samples.remove(0);
    }
    while history.master_samples.len() < PEAK_HISTORY_LENGTH {
        history.master_samples.insert(0, 0.0);
    }
    ui.set_master_peak_value(master_peak);
    ui.set_master_peak_db(format_peak_db(master_peak).into());
    if master_changed {
        ui.set_master_meter_history(ModelRc::new(VecModel::from(history.master_samples.clone())));
    }
}
fn format_gain_db(value: f32) -> String {
    if value <= 0.001 {
        return "-∞ dB".into();
    }
    format!("{:.1} dB", 20.0 * value.log10())
}
fn format_peak_db(value: f32) -> String {
    if value <= 0.0001 {
        return "-∞ dB".into();
    }
    format!("{:.1} dB", 20.0 * value.log10())
}
fn apply_list_refresh(ui: &MainWindow, state: &AppState) {
    let result = state
        .list_refresh
        .result
        .lock()
        .ok()
        .and_then(|mut pending| pending.take());
    let Some(result) = result else {
        return;
    };
    state.list_refresh.pending.store(false, Ordering::Release);
    match result {
        Ok(lists) => {
            super::lists::apply_process_list(ui, &state.cache, lists.processes);
            super::lists::apply_mics(ui, &state.cache, lists.mics);
            super::lists::apply_destinations(ui, &state.cache, lists.destinations, false);
            if ui.get_mix_mic() && ui.get_mic_index() == 0 {
                ui.invoke_mic_mix_changed(false);
            }
            if state.windows.add_source.window().is_visible() {
                super::sync_add_source_window(
                    ui,
                    state.windows.add_source.as_ref(),
                    &state.cache,
                    &state.app_config,
                );
            }
        }
        Err(message) => super::diagnostics::report_warning(ui, &message),
    }
}

fn update_engine_status(ui: &MainWindow, state: &AppState) {
    match state.engine.status() {
        EngineStatus::Running => {
            if !ui.get_running() {
                ui.set_running(true);
                ui.set_status_text("运行中".into());
                ui.set_status_kind(1);
            }
        }
        EngineStatus::Idle => {
            if ui.get_running() {
                restore_default_mics(&state.saved_mics);
                ui.set_running(false);
                ui.set_status_text("未运行".into());
                ui.set_status_kind(0);
                ui.set_error_text("".into());
            }
        }
        EngineStatus::ProcessExited => {
            restore_default_mics(&state.saved_mics);
            ui.set_running(false);
            ui.set_status_text("进程已退出".into());
            ui.set_status_kind(2);
            super::diagnostics::report_warning(ui, "进程已退出");
        }
        EngineStatus::DeviceGone => {
            restore_default_mics(&state.saved_mics);
            ui.set_running(false);
            ui.set_status_text("设备已断开".into());
            ui.set_status_kind(3);
            super::diagnostics::report_error(ui, &Error::DeviceNotFound);
        }
        EngineStatus::Error(message) => {
            restore_default_mics(&state.saved_mics);
            ui.set_running(false);
            ui.set_status_text("错误".into());
            ui.set_status_kind(3);
            ui.set_error_text(message.clone().into());
            super::diagnostics::report_warning(ui, &message);
        }
    }
}

fn update_player_progress(ui: &MainWindow, state: &AppState) {
    if ui.get_audio_entry_index() >= 0
        && let Some(actual) = visible_entry(&state.cache, ui.get_audio_entry_index() as usize)
        && let Some(entry) = state.cache.borrow().audio_entries.get(actual)
    {
        ui.set_player_loop(entry.loop_playback);
    }
    if ui.get_player_paused() {
        return;
    }
    let file = state.engine.file_state();
    if let Some(error) = file.error {
        ui.set_player_playing(false);
        super::diagnostics::report_warning(ui, &error);
        return;
    }
    if !file.playing && ui.get_player_playing() {
        ui.set_player_playing(false);
    }
    if file.duration_hns == 0 || ui.get_audio_seeking_progress() {
        return;
    }
    ui.set_audio_current_name(file.name.into());
    ui.set_audio_duration_seconds(file.duration_hns as f32 / 10_000_000.0);
    let progress = file.position_hns as f32 / file.duration_hns as f32;
    ui.set_audio_progress(progress.clamp(0.0, 1.0));
    ui.set_audio_current_position(format_hns_position(file.position_hns, file.duration_hns).into());
}

fn update_device_label(ui: &MainWindow, state: &AppState, last_dest_idx: &Cell<i32>) {
    let device_index = ui.get_device_index();
    if device_index != last_dest_idx.get() {
        last_dest_idx.set(device_index);
        update_output_text(ui, &state.cache);
    }
}
