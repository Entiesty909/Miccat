//! 轮询引擎启停状态、音频播放进度和全局输出设备名称。

use super::*;
use crate::engine::EngineStatus;
use slint::{Timer, TimerMode};
use std::cell::Cell;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub(super) fn start_status_timer(ui: &MainWindow, state: &AppState) -> Timer {
    let ui_weak = ui.as_weak();
    let state = state.clone();
    let last_dest_idx = Cell::new(ui.get_device_index());
    let last_diag_unseen = Cell::new(0usize);
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        apply_list_refresh(&ui, &state);
        update_engine_status(&ui, &state);
        update_player_progress(&ui, &state);
        update_device_label(&ui, &state, &last_dest_idx);
        super::sources::update(&ui, &state);
        super::diagnostics::sync(&ui, &last_diag_unseen);
    });
    timer
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
