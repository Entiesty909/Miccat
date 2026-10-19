//! 路由源行回调：输出目标、音量、静音、启用与热切换。

use super::super::super::*;
use slint::Model;
use std::cell::RefCell;
use std::rc::Rc;

use crate::config;
use crate::engine::AudioEngine;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    {
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        ui.on_source_output_changed(move |row, target_index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, row) else {
                return;
            };
            let target_id = cache
                .borrow()
                .dests
                .get(target_index as usize)
                .map(|device| device.render_id.clone());
            let Some(target_id) = target_id else { return };
            {
                let mut config = app_config.borrow_mut();
                match kind {
                    0 => config.output.process_output_id = Some(target_id),
                    1 => config.output.file_output_id = Some(target_id),
                    _ => config.output.mic_output_id = Some(target_id),
                }
            }
            save_queue.schedule();
            // 回写模型，否则下一次 50 ms 刷新会用旧下标把用户的选择覆盖回去。
            if let Ok(row_index) = usize::try_from(row) {
                let indices = ui.get_source_output_indices();
                if row_index < indices.row_count() {
                    indices.set_row_data(row_index, target_index);
                }
            }
        });
    }
    {
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        ui.on_file_output_changed(move |target_index| {
            let target_id = cache
                .borrow()
                .dests
                .get(target_index as usize)
                .map(|device| device.render_id.clone());
            let Some(target_id) = target_id else { return };
            app_config.borrow_mut().output.file_output_id = Some(target_id);
            save_queue.schedule();
        });
    }
    {
        let engine = state.engine.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        ui.on_source_volume_edited(move |index, value| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            let value = (value / 100.0).clamp(0.0, 1.0);
            set_source_volume(&engine, &app_config, kind, value);
            save_queue.schedule();
            // 回写模型：把引擎实际生效的值写回，保证下一次刷新看到相同数值，
            // 不会在拖动过程中重建行、把滑块从用户手里抢走。
            let actual = source_volume(&engine, kind) * 100.0;
            if let Ok(row) = usize::try_from(index) {
                let volumes = ui.get_source_volumes();
                if row < volumes.row_count() {
                    volumes.set_row_data(row, actual);
                }
            }
        });
    }
    {
        let engine = state.engine.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        ui.on_microphone_input_volume_edited(move |value| {
            let volume = (value / 100.0).clamp(0.0, 1.0);
            engine.set_mic_volume(volume);
            app_config.borrow_mut().mix.mic = volume;
            save_queue.schedule();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_microphone_input_volume(volume * 100.0);
            }
        });
    }
    {
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let ui_weak = ui.as_weak();
        ui.on_source_mute_toggled(move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            let current = source_volume(&engine, kind);
            let restore = if current > 0.001 {
                cache.borrow_mut().muted_volumes[kind] = Some(current);
                0.0
            } else {
                cache.borrow_mut().muted_volumes[kind]
                    .take()
                    .unwrap_or_else(|| configured_source_volume(&app_config, kind))
            };
            set_engine_source_volume(&engine, kind, restore);
            // 回写模型，避免下一次 50 ms 刷新把按钮状态翻回去。
            let muted = source_volume(&engine, kind) <= 0.001;
            if let Ok(row) = usize::try_from(index) {
                let states = ui.get_source_muted();
                if row < states.row_count() {
                    states.set_row_data(row, muted);
                }
                let volumes = ui.get_source_volumes();
                if row < volumes.row_count() {
                    volumes.set_row_data(row, source_volume(&engine, kind) * 100.0);
                }
            }
        });
    }
    {
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        ui.on_source_active_toggled(move |index, enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            match kind {
                0 => {
                    if toggle_process_source_row(&ui, &cache, index) {
                        crate::ui_bridge::persist::save_process_selection(
                            &cache,
                            &app_config,
                            &save_queue,
                        );
                        if ui.get_running() {
                            hot_swap_process_sources(&engine, &cache);
                        }
                    }
                }
                1 => toggle_file_source(&ui, &engine, &cache, index, enabled),
                2 if enabled != ui.get_mix_mic() => ui.invoke_mic_mix_changed(enabled),
                _ => {}
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_source_pause(move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if source_kind(&ui, index) == Some(1) && ui.get_running() && ui.get_player_playing() {
                ui.invoke_player_pause_toggle();
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_source_stop(move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if source_kind(&ui, index) == Some(1) {
                ui.invoke_audio_stop();
            }
        });
    }
    ui.on_source_action({
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let state_for_update = state.clone();
        let ui_weak = ui.as_weak();
        move |index, action| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            match action {
                1 if kind == 1 => {
                    if let Some(entry_index) =
                        crate::ui_bridge::sources::file_context_index(&ui, &engine, &cache)
                    {
                        ui.set_page(1);
                        ui.set_audio_entry_index(entry_index);
                        ui.invoke_audio_entry_hotkey_bind();
                    }
                }
                2 => {
                    super::super::route_start::start_source_only(
                        &ui,
                        &cache,
                        &app_config,
                        &engine,
                        kind,
                        index,
                    );
                }
                3 if kind == 1 => {
                    if let Some(entry_index) =
                        crate::ui_bridge::sources::file_context_index(&ui, &engine, &cache)
                    {
                        ui.set_page(1);
                        ui.set_audio_entry_index(entry_index);
                    }
                }
                5 if kind == 0 => {
                    ui.set_source_replace_index(index);
                    ui.set_source_add_tab(0);
                    super::super::super::open_add_source_window(&ui, &state_for_update);
                }
                6 if kind == 1 => {
                    ui.set_source_replace_index(index);
                    ui.set_source_add_tab(1);
                    super::super::super::open_add_source_window(&ui, &state_for_update);
                }
                4 => {
                    delete_source(&ui, &cache, &engine, kind, index);
                    crate::ui_bridge::persist::save_process_selection(
                        &cache,
                        &app_config,
                        &save_queue,
                    );
                }
                _ => {}
            }
            super::super::super::sources::update(&ui, &state_for_update);
        }
    });
}

fn delete_source(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    engine: &AudioEngine,
    kind: usize,
    source_index: i32,
) {
    match kind {
        0 => {
            let Ok(source_index) = usize::try_from(source_index) else {
                return;
            };
            let kinds = ui.get_source_kinds();
            let kinds = (0..kinds.row_count())
                .map(|row| kinds.row_data(row).unwrap_or(-1))
                .collect::<Vec<_>>();
            let Some(slot) = process_source_slot(&kinds, source_index) else {
                return;
            };
            let cache_state = cache.borrow();
            let Some(pid) =
                visible_process_pid(&cache_state.selected_pids, &cache_state.pids, slot)
            else {
                return;
            };
            drop(cache_state);
            let mut cache_state = cache.borrow_mut();
            cache_state
                .selected_pids
                .retain(|selected| *selected != pid);
            cache_state
                .disabled_pids
                .retain(|disabled| *disabled != pid);
            drop(cache_state);
            if ui.get_running() {
                hot_swap_process_sources(engine, cache);
            }
        }
        1 => {
            let Ok(source_index) = usize::try_from(source_index) else {
                return;
            };
            let kinds = ui.get_source_kinds();
            let file_slot = (0..source_index)
                .filter(|row| kinds.row_data(*row) == Some(1))
                .count();
            let mut cache_state = cache.borrow_mut();
            if file_slot < cache_state.route_audio_entries.len() {
                cache_state.route_audio_entries.remove(file_slot);
                if file_slot < cache_state.route_audio_enabled.len() {
                    cache_state.route_audio_enabled.remove(file_slot);
                }
            }
            cache_state.route_audio_entry = cache_state.route_audio_entries.first().copied();
            cache_state.route_file_enabled = cache_state
                .route_audio_enabled
                .iter()
                .any(|enabled| *enabled);
            if cache_state.route_audio_entries.is_empty() {
                cache_state.file_source_hidden = true;
            }
            drop(cache_state);
            if ui.get_player_playing() {
                ui.invoke_audio_stop();
            }
        }

        2 => {
            cache.borrow_mut().mic_source_hidden = true;
            if ui.get_mix_mic() {
                ui.invoke_mic_mix_changed(false);
            }
        }
        _ => {}
    }
}
fn source_kind(ui: &MainWindow, index: i32) -> Option<usize> {
    ui.get_source_kinds()
        .row_data(index as usize)
        .map(|kind| kind as usize)
}
/// 运行中把已选进程列表重新映射到采集槽位：按槽位替换，不重建输出。
fn hot_swap_process_sources(engine: &AudioEngine, cache: &Rc<RefCell<UiCache>>) {
    let wanted: Vec<u32> = {
        let cache = cache.borrow();
        cache
            .selected_pids
            .iter()
            .copied()
            .filter(|pid| cache.pids.contains(pid) && !cache.disabled_pids.contains(pid))
            .collect()
    };
    if let Err(error) = engine.replace_process_sources(&wanted) {
        tracing::warn!("重建进程源失败：{error}");
    }
}

fn toggle_file_source(
    ui: &MainWindow,
    engine: &AudioEngine,
    cache: &Rc<RefCell<UiCache>>,
    source_index: i32,
    enabled: bool,
) {
    let Ok(source_index) = usize::try_from(source_index) else {
        return;
    };
    let kinds = ui.get_source_kinds();
    let file_slot = (0..source_index)
        .filter(|row| kinds.row_data(*row) == Some(1))
        .count();
    let selected_visible = visible_entry(cache, ui.get_audio_entry_index().max(0) as usize);
    let entry = {
        let mut cache_state = cache.borrow_mut();
        if cache_state.route_audio_entries.is_empty()
            && let Some(index) = selected_visible
        {
            cache_state.route_audio_entries.push(index);
            cache_state.route_audio_enabled.push(false);
        }
        if file_slot >= cache_state.route_audio_entries.len() {
            return;
        }
        if let Some(state) = cache_state.route_audio_enabled.get_mut(file_slot) {
            *state = enabled;
        }
        cache_state.route_file_enabled = cache_state.route_audio_enabled.iter().any(|state| *state);
        let index = cache_state.route_audio_entries[file_slot];
        cache_state.audio_entries.get(index).cloned()
    };
    if engine.is_running()
        && enabled
        && let Some(entry) = entry
    {
        let _ = engine.play_file_source(&entry);
    }
}

fn toggle_process_source_row(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    source_index: i32,
) -> bool {
    let Ok(source_index) = usize::try_from(source_index) else {
        return false;
    };
    let source_kinds = ui.get_source_kinds();
    let kinds = (0..source_kinds.row_count())
        .map(|row| source_kinds.row_data(row).unwrap_or(-1))
        .collect::<Vec<_>>();
    let Some(slot) = process_source_slot(&kinds, source_index) else {
        return false;
    };
    let cache_state = cache.borrow();
    let Some(pid) = visible_process_pid(&cache_state.selected_pids, &cache_state.pids, slot) else {
        return false;
    };
    drop(cache_state);
    let mut cache = cache.borrow_mut();
    if cache.disabled_pids.contains(&pid) {
        cache.disabled_pids.retain(|disabled| *disabled != pid);
    } else {
        cache.disabled_pids.push(pid);
    }
    true
}

fn process_source_slot(source_kinds: &[i32], source_index: usize) -> Option<usize> {
    if source_kinds.get(source_index) != Some(&0) {
        return None;
    }
    Some(
        source_kinds[..source_index]
            .iter()
            .filter(|kind| **kind == 0)
            .count(),
    )
}
fn visible_process_pid(
    selected_pids: &[u32],
    available_pids: &[u32],
    visible_slot: usize,
) -> Option<u32> {
    selected_pids
        .iter()
        .filter(|pid| available_pids.contains(pid))
        .nth(visible_slot)
        .copied()
}

fn source_volume(engine: &AudioEngine, kind: usize) -> f32 {
    match kind {
        0 => engine.volume(),
        1 => engine.sfx_volume(),
        _ => engine.mic_volume(),
    }
}

fn configured_source_volume(app_config: &Rc<RefCell<config::AppConfig>>, kind: usize) -> f32 {
    let config = app_config.borrow();
    match kind {
        0 => config.mix.process,
        1 => config.mix.sfx,
        _ => config.mix.mic,
    }
}

fn set_engine_source_volume(engine: &AudioEngine, kind: usize, value: f32) {
    match kind {
        0 => engine.set_volume(value),
        1 => engine.set_sfx_volume(value),
        _ => engine.set_mic_volume(value),
    }
}

fn set_source_volume(
    engine: &AudioEngine,
    app_config: &Rc<RefCell<config::AppConfig>>,
    kind: usize,
    value: f32,
) {
    match kind {
        0 => {
            engine.set_volume(value);
            app_config.borrow_mut().mix.process = value;
        }
        1 => {
            engine.set_sfx_volume(value);
            app_config.borrow_mut().mix.sfx = value;
        }
        _ => {
            engine.set_mic_volume(value);
            app_config.borrow_mut().mix.mic = value;
        }
    }
}
#[cfg(test)]
#[path = "../route/tests.rs"]
mod tests;
