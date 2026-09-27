//! player 相关 Slint 回调注册。

use super::super::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
    register_3(ui, state);
    register_4(ui, state);
}

fn register_1(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_entry_play(move |_| {
            if let Some(ui) = ui_weak.upgrade() {
                cache.borrow_mut().force_audio_play = true;
                ui.invoke_audio_entry_double_clicked(ui.get_audio_entry_index());
            }
        });
    }
}

fn register_2(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let preview = state.preview.clone();
    {
        let ui_weak = ui.as_weak();
        let engine = engine.clone();
        let cache = cache.clone();
        let preview = preview.clone();
        let app_config = app_config.clone();
        let shared_state = (*state).clone();
        ui.on_audio_entry_double_clicked(move |index| {
            let now = Instant::now();
            let should_play = {
                let mut c = cache.borrow_mut();
                let forced = c.force_audio_play;
                c.force_audio_play = false;
                let double = c.last_audio_click.is_some_and(|(last, time)| {
                    last == index as usize && now.duration_since(time) < Duration::from_millis(450)
                });
                c.last_audio_click = Some((index as usize, now));
                forced || double
            };
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            ui.set_audio_entry_index(index);
            if !should_play {
                return;
            }
            let entry = visible_entry(&cache, index as usize)
                .and_then(|actual| cache.borrow().audio_entries.get(actual).cloned());
            let Some(entry) = entry else {
                return;
            };
            if ui.get_player_playing() && ui.get_audio_current_name() == entry.name {
                stop_playback(&shared_state);
                ui.set_player_playing(false);
                ui.set_audio_progress(0.0);
                ui.set_audio_duration_seconds(0.0);
                ui.set_audio_current_position("".into());
                return;
            }
            stop_playback(&shared_state);
            ui.set_player_playing(false);
            ui.set_audio_progress(0.0);
            ui.set_audio_duration_seconds(0.0);
            ui.set_audio_current_position("".into());
            start_playback(
                &ui,
                &engine,
                &cache,
                &preview,
                app_config.borrow().preview_volume,
                &entry,
                0,
            );
        });
    }
}

fn register_3(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let preview = state.preview.clone();
    {
        let ui_weak = ui.as_weak();
        let engine = engine.clone();
        let cache = cache.clone();
        let preview = preview.clone();
        let app_config = app_config.clone();
        let shared_state = (*state).clone();
        ui.on_audio_progress_seek(move |position| {
            let Some(playing) = cache.borrow().audio_playing.clone() else {
                return;
            };
            if playing.duration_hns == 0 {
                return;
            }
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let position = position.clamp(0.0, 1.0);
            let target_hns = (f64::from(position) * playing.duration_hns as f64) as u64;
            // 先停旧流，否则新旧两段会叠着放。
            stop_playback(&shared_state);
            let entry = config::AudioEntry {
                name: playing.name.clone(),
                path: playing.path.clone(),
                category: String::new(),
                duration_hns: playing.duration_hns,
                loop_playback: playing.looped,
                hotkey: None,
            };
            start_playback(
                &ui,
                &engine,
                &cache,
                &preview,
                app_config.borrow().preview_volume,
                &entry,
                target_hns,
            );
            // 立刻回显目标位置，不用等解码线程出第一块。
            ui.set_audio_progress(position);
            ui.set_audio_current_position(
                format_hns_position(target_hns, playing.duration_hns).into(),
            );
        });
    }
}

fn register_4(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let shared_state = (*state).clone();
        ui.on_audio_stop(move || {
            stop_playback(&shared_state);
            {
                let mut c = cache.borrow_mut();
                c.audio_playing = None;
                c.audio_anchor = None;
            }
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_player_playing(false);
                ui.set_audio_progress(0.0);
                ui.set_audio_duration_seconds(0.0);
                ui.set_audio_current_position("".into());
            }
        });
    }
}

fn stop_playback(state: &AppState) {
    if let Some(cancel) = state.cache.borrow_mut().audio_cancel.take() {
        cancel.store(true, Ordering::Release);
    }
    if let Ok(sender) = state.engine.sfx_sender() {
        let _ = sender.send(crate::audio::sfx::SfxCommand::StopAll);
    }
    if let Ok(guard) = state.preview.lock() {
        if let Some(preview) = guard.as_ref() {
            preview.stop_source();
        }
    }
}
