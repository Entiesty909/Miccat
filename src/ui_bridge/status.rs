//! 轮询引擎状态、电平、播放进度与设备标签。

use super::*;
use crate::engine::EngineStatus;
use slint::{Timer, TimerMode};
use std::cell::Cell;
use std::time::Duration;

pub(super) fn start_status_timer(ui: &MainWindow, state: &AppState) -> Timer {
    let ui_weak = ui.as_weak();
    let state = state.clone();
    let last_dest_idx = Cell::new(ui.get_device_index());
    let last_mic_idx = Cell::new(ui.get_mic_index());
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        update_engine_status(&ui, &state);
        update_audio_levels(&ui, &state);
        update_player_progress(&ui, &state);
        update_device_labels(&ui, &state, &last_dest_idx, &last_mic_idx);
    });
    timer
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
            ui.set_error_text("进程已退出".into());
        }
        EngineStatus::DeviceGone => {
            restore_default_mics(&state.saved_mics);
            ui.set_running(false);
            ui.set_status_text("设备已断开".into());
            ui.set_status_kind(3);
            ui.set_error_text("无法连接到音频设备\n设备可能已经被拔出。".into());
        }
        EngineStatus::Error(message) => {
            restore_default_mics(&state.saved_mics);
            ui.set_running(false);
            ui.set_status_text("错误".into());
            ui.set_status_kind(3);
            ui.set_error_text(message.into());
        }
    }
}

fn update_audio_levels(ui: &MainWindow, state: &AppState) {
    let running = state.engine.is_running();
    let capture = if running {
        state.engine.sample_capture_peak()
    } else {
        0.0
    };
    let mic = if running {
        state.engine.sample_mic_peak()
    } else {
        0.0
    };
    let render = if running {
        state.engine.sample_render_peak()
    } else {
        0.0
    };
    push_bar(&mut state.capture_hist.borrow_mut(), capture);
    push_bar(&mut state.mic_hist.borrow_mut(), mic);
    push_bar(&mut state.render_hist.borrow_mut(), render);
    set_wave(
        ui,
        &state.capture_hist.borrow(),
        &state.mic_hist.borrow(),
        &state.render_hist.borrow(),
    );
}

fn update_player_progress(ui: &MainWindow, state: &AppState) {
    if !ui.get_player_playing() || ui.get_audio_seeking_progress() {
        return;
    }
    let duration_seconds = ui.get_audio_duration_seconds();
    let anchor = state.cache.borrow().audio_anchor;
    let looped = state
        .cache
        .borrow()
        .audio_playing
        .as_ref()
        .is_some_and(|playing| playing.looped);
    if let (true, Some((start, offset_hns))) = (duration_seconds > 0.0, anchor) {
        let duration_hns = (f64::from(duration_seconds) * 10_000_000.0) as u64;
        let elapsed_hns = (start.elapsed().as_nanos() / 100) as u64;
        let played_hns = offset_hns + elapsed_hns;
        let ended = played_hns >= duration_hns;
        if ended && !looped {
            ui.set_player_playing(false);
        }
        let shown_hns = if ended && !looped {
            duration_hns
        } else if duration_hns == 0 {
            0
        } else {
            played_hns % duration_hns
        };
        let progress = if duration_hns == 0 {
            0.0
        } else {
            shown_hns as f32 / duration_hns as f32
        };
        ui.set_audio_progress(progress.clamp(0.0, 1.0));
        ui.set_audio_current_position(format_hns_position(shown_hns, duration_hns).into());
    }
}

fn update_device_labels(
    ui: &MainWindow,
    state: &AppState,
    last_dest_idx: &Cell<i32>,
    last_mic_idx: &Cell<i32>,
) {
    let device_index = ui.get_device_index();
    if device_index != last_dest_idx.get() {
        last_dest_idx.set(device_index);
        update_output_text(ui, &state.cache);
    }
    let mic_index = ui.get_mic_index();
    if mic_index != last_mic_idx.get() {
        last_mic_idx.set(mic_index);
        update_mic_text(ui, &state.cache);
    }
    ui.set_has_mic(mic_index != 0);
}

fn push_bar(hist: &mut Vec<f32>, value: f32) {
    if hist.len() >= WAVE_BARS {
        hist.remove(0);
    }
    hist.push(value.clamp(0.0, 1.0));
}

pub(super) fn set_wave(ui: &MainWindow, capture: &[f32], mic: &[f32], render: &[f32]) {
    ui.set_capture_wave(ModelRc::new(VecModel::from(capture.to_vec())));
    ui.set_mic_wave(ModelRc::new(VecModel::from(mic.to_vec())));
    ui.set_render_wave(ModelRc::new(VecModel::from(render.to_vec())));
}
