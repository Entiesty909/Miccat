//! 音频库播放回调；播放线程、解码和试听均由 AudioEngine 管理。

use super::super::*;
use std::time::{Duration, Instant};

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_play_action(ui, state);
    register_double_click(ui, state);
    register_seek(ui, state);
    register_stop(ui, state);
}

fn register_play_action(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_play(move |_| {
        if let Some(ui) = ui_weak.upgrade() {
            cache.borrow_mut().force_audio_play = true;
            ui.invoke_audio_entry_double_clicked(ui.get_audio_entry_index());
        }
    });
}

fn register_double_click(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_double_clicked(move |index| {
        let now = Instant::now();
        let should_play = {
            let mut cache = cache.borrow_mut();
            let forced = cache.force_audio_play;
            cache.force_audio_play = false;
            let double = cache.last_audio_click.is_some_and(|(last, time)| {
                last == index as usize && now.duration_since(time) < Duration::from_millis(450)
            });
            cache.last_audio_click = Some((index as usize, now));
            forced || double
        };
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        ui.set_audio_entry_index(index);
        if !should_play {
            return;
        }
        let Some(entry) = selected_entry(&cache, index as usize) else {
            return;
        };
        let current = engine.file_state();
        if current.playing && current.name == entry.name {
            stop_ui(&ui, &engine);
            return;
        }
        stop_ui(&ui, &engine);
        start_ui(&ui, &engine, &entry, 0);
    });
}

fn register_seek(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_progress_seek(move |position| {
        let current = engine.file_state();
        if !current.playing || current.duration_hns == 0 {
            return;
        }
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let position = position.clamp(0.0, 1.0);
        let target_hns = (f64::from(position) * current.duration_hns as f64) as u64;
        let entry = config::AudioEntry {
            name: current.name,
            path: current.path,
            category: String::new(),
            duration_hns: current.duration_hns,
            loop_playback: current.looped,
        };
        engine.stop_file();
        start_ui(&ui, &engine, &entry, target_hns);
        ui.set_audio_progress(position);
        ui.set_audio_current_position(format_hns_position(target_hns, entry.duration_hns).into());
    });
}

fn register_stop(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_stop(move || {
        engine.stop_file();
        if let Some(ui) = ui_weak.upgrade() {
            reset_ui(&ui);
        }
    });
}

fn selected_entry(cache: &Rc<RefCell<UiCache>>, index: usize) -> Option<config::AudioEntry> {
    let actual = visible_entry(cache, index)?;
    cache.borrow().audio_entries.get(actual).cloned()
}

fn start_ui(ui: &MainWindow, engine: &AudioEngine, entry: &config::AudioEntry, start_hns: u64) {
    match engine.play_file(entry, start_hns) {
        Ok(()) => {
            ui.set_audio_current_name(entry.name.clone().into());
            ui.set_audio_duration_seconds(entry.duration_hns as f32 / 10_000_000.0);
            ui.set_audio_progress(if entry.duration_hns == 0 {
                0.0
            } else {
                start_hns as f32 / entry.duration_hns as f32
            });
            ui.set_player_playing(true);
            ui.set_error_text("".into());
        }
        Err(error) => {
            reset_ui(ui);
            ui.set_error_text(error.user_message().into());
        }
    }
}

fn stop_ui(ui: &MainWindow, engine: &AudioEngine) {
    engine.stop_file();
    reset_ui(ui);
}

fn reset_ui(ui: &MainWindow) {
    ui.set_player_playing(false);
    ui.set_audio_progress(0.0);
    ui.set_audio_duration_seconds(0.0);
    ui.set_audio_current_position("".into());
}
