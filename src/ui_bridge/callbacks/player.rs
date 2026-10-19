//! 音频库播放回调；播放线程、解码和试听均由 AudioEngine 管理。

use super::super::*;
use crate::engine::PlaybackTarget;
use slint::Model;
use std::time::{Duration, Instant};

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_play_action(ui, state);
    register_double_click(ui, state);
    register_seek(ui, state);
    register_stop(ui, state);
    register_pause(ui, state);
    register_navigation(ui, state);
    register_file_volume(ui, state);
}

fn register_file_volume(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_file_volume_edited(move |value| {
        let volume = (value / 100.0).clamp(0.0, 1.0);
        engine.set_sfx_volume_for_slot(crate::engine::PLAYBACK_SLOT, volume);
        app_config.borrow_mut().mix.sfx = volume;
        save_queue.schedule();
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_audio_file_volume(volume * 100.0);
        }
    });
}

fn register_play_action(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_play(move |mode| {
        if let Some(ui) = ui_weak.upgrade() {
            let target = playback_target(mode);
            cache.borrow_mut().playback_target = target;
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
        let (should_play, target, forced) = {
            let mut cache = cache.borrow_mut();
            let forced = cache.force_audio_play;
            cache.force_audio_play = false;
            let target = if forced {
                cache.playback_target
            } else {
                PlaybackTarget::Both
            };
            let double = cache.last_audio_click.is_some_and(|(last, time)| {
                last == index as usize && now.duration_since(time) < Duration::from_millis(450)
            });
            cache.last_audio_click = Some((index as usize, now));
            (forced || double, target, forced)
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
        // 双击正在播放的同一条目 = 暂停/继续，而不是停止重播。
        if current.playing && current.name == entry.name && !forced {
            ui.invoke_player_pause_toggle();
            return;
        }
        if ui.get_player_paused() && current.name == entry.name && !forced {
            ui.invoke_player_pause_toggle();
            return;
        }
        cache.borrow_mut().paused_audio = None;
        cache.borrow_mut().file_source_hidden = false;
        cache.borrow_mut().playback_target = target;
        ui.set_player_paused(false);
        stop_ui(&ui, &engine, false);
        start_ui(&ui, &engine, &entry, 0, target);
    });
}

fn register_seek(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
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
            icon_path: None,
            icon_glyph: "\u{EC4F}".into(),
            duration_hns: current.duration_hns,
            file_size_bytes: 0,
            sample_rate: 0,
            channels: 0,
            loop_playback: current.looped,
            hotkey: None,
            hotkey_binding: None,
        };
        engine.stop_file();
        let target = cache.borrow().playback_target;
        start_ui(&ui, &engine, &entry, target_hns, target);
        ui.set_audio_progress(position);
        ui.set_audio_current_position(format_hns_position(target_hns, entry.duration_hns).into());
    });
}

fn register_stop(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_stop(move || {
        engine.stop_file();
        cache.borrow_mut().paused_audio = None;
        cache.borrow_mut().playback_target = PlaybackTarget::Both;
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_player_paused(false);
            ui.set_player_entry_index(-1);
            engine.set_monitor_default(ui.get_preview_enabled());
            reset_ui(&ui);
        }
    });
}

fn register_pause(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_player_pause_toggle(move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        if ui.get_player_paused() {
            let paused = cache.borrow_mut().paused_audio.take();
            if let Some((entry, position, target)) = paused {
                start_ui(&ui, &engine, &entry, position, target);
                ui.set_player_paused(false);
            }
            return;
        }
        let file = engine.file_state();
        if !file.playing {
            return;
        }
        let entry = cache
            .borrow()
            .audio_entries
            .iter()
            .find(|entry| entry.path == file.path)
            .cloned();
        let Some(entry) = entry else {
            return;
        };
        let target = cache.borrow().playback_target;
        cache.borrow_mut().paused_audio = Some((entry, file.position_hns, target));
        if file.duration_hns > 0 {
            ui.set_audio_progress(file.position_hns as f32 / file.duration_hns as f32);
            ui.set_audio_current_position(
                format_hns_position(file.position_hns, file.duration_hns).into(),
            );
        }
        engine.stop_file();
        engine.set_monitor_default(ui.get_preview_enabled());
        ui.set_player_paused(true);
        ui.set_player_playing(false);
    });
}

fn register_navigation(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let engine = state.engine.clone();
    let ui_weak = ui.as_weak();
    ui.on_player_previous(move || {
        navigate(&ui_weak, &cache, &engine, -1);
    });
    let cache = state.cache.clone();
    let engine = state.engine.clone();
    let ui_weak = ui.as_weak();
    ui.on_player_next(move || {
        navigate(&ui_weak, &cache, &engine, 1);
    });
}

fn navigate(
    ui_weak: &slint::Weak<MainWindow>,
    cache: &Rc<RefCell<UiCache>>,
    engine: &AudioEngine,
    direction: i32,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let current = ui.get_audio_entry_index();
    let next = current + direction;
    if next < 0 || next >= ui.get_audio_entries().row_count() as i32 {
        return;
    }
    let Some(entry) = selected_entry(cache, next as usize) else {
        return;
    };
    ui.set_audio_entry_index(next);
    let target = cache.borrow().playback_target;
    cache.borrow_mut().paused_audio = None;
    engine.stop_file();
    start_ui(&ui, engine, &entry, 0, target);
}

fn selected_entry(cache: &Rc<RefCell<UiCache>>, index: usize) -> Option<config::AudioEntry> {
    let actual = visible_entry(cache, index)?;
    cache.borrow().audio_entries.get(actual).cloned()
}

/// 播放指定条目；供全局热键等非列表入口复用。
pub(super) fn play_entry(
    ui: &MainWindow,
    engine: &AudioEngine,
    entry: &config::AudioEntry,
    target: PlaybackTarget,
) {
    start_ui(ui, engine, entry, 0, target);
}

fn start_ui(
    ui: &MainWindow,
    engine: &AudioEngine,
    entry: &config::AudioEntry,
    start_hns: u64,
    target: PlaybackTarget,
) {
    // 同步确保试听设备就绪：异步打开与随后的解码投递有竞态，
    // 会表现为「显示播放中但没有声音」。
    engine.ensure_monitor_default(target != PlaybackTarget::Route || ui.get_preview_enabled());
    match engine.play_file(entry, start_hns, target) {
        Ok(()) => {
            ui.set_audio_current_name(entry.name.clone().into());
            ui.set_audio_duration_seconds(entry.duration_hns as f32 / 10_000_000.0);
            ui.set_audio_progress(if entry.duration_hns == 0 {
                0.0
            } else {
                start_hns as f32 / entry.duration_hns as f32
            });
            ui.set_player_paused(false);
            ui.set_player_entry_index(ui.get_audio_entry_index());
            ui.set_player_playing(true);
            ui.set_error_text("".into());
        }
        Err(error) => {
            reset_ui(ui);
            super::super::diagnostics::report_error(ui, &error);
        }
    }
}

fn stop_ui(ui: &MainWindow, engine: &AudioEngine, restore_monitor: bool) {
    if restore_monitor {
        engine.set_monitor_default(ui.get_preview_enabled());
    }
    engine.stop_file();
    reset_ui(ui);
}

fn reset_ui(ui: &MainWindow) {
    ui.set_player_playing(false);
    ui.set_player_paused(false);
    ui.set_player_entry_index(-1);
    ui.set_audio_progress(0.0);
    ui.set_audio_duration_seconds(0.0);
    ui.set_audio_current_position("".into());
}

fn playback_target(mode: i32) -> PlaybackTarget {
    match mode {
        1 => PlaybackTarget::Monitor,
        2 => PlaybackTarget::Route,
        _ => PlaybackTarget::Both,
    }
}
