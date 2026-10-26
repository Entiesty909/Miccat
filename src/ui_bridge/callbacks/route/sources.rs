//! 路由源行回调：输出目标、音量、静音、启用与热切换。
mod outputs;

use super::super::super::*;
use slint::Model;
use std::cell::RefCell;
use std::rc::Rc;

use crate::config;
use crate::engine::AudioEngine;
/// 检查候选 PID 是否已占用其他进程槽；替换时排除当前槽位。
pub(crate) fn duplicate_process(pids: &[u32], candidate: u32, replacing: Option<usize>) -> bool {
    // selected_pids 的索引就是进程槽位；替换已有卡片时跳过原槽位，允许原源保持不变。
    pids.iter()
        .enumerate()
        .any(|(index, pid)| Some(index) != replacing && *pid == candidate)
}

/// 检查候选音频条目是否已占用其他文件槽，防止同一文件出现在多个卡片。
pub(crate) fn duplicate_audio(
    entries: &[usize],
    candidate: usize,
    replacing: Option<usize>,
) -> bool {
    entries
        // 音频条目同样按文件槽位去重，不能让同一库条目同时驱动两张卡片。
        .iter()
        .enumerate()
        .any(|(index, entry)| Some(index) != replacing && *entry == candidate)
}

/// 绑定源行的输出、音量、静音、启停、操作菜单等回调；kind 0/1/2 分别是进程/文件/麦克风。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    outputs::register(ui, state);
    {
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        // UI 使用百分比，音频引擎与配置统一保存为 0.0..=1.0 的增益。
        ui.on_source_volume_edited(move |index, value| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            let value = (value / 100.0).clamp(0.0, 1.0);
            set_source_volume(&ui, &cache, &engine, &app_config, kind, index, value);
            save_queue.schedule();
            if let Ok(row) = usize::try_from(index) {
                let volumes = ui.get_source_volumes();
                if row < volumes.row_count() {
                    volumes.set_row_data(row, value * 100.0);
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
        // 卡片监听只修改本机分支；转发的麦克风/SFX 原子增益保持原值。
        ui.on_source_preview_volume_edited(move |index, percent| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            let value = (percent / 100.0).clamp(0.0, 1.0);
            match kind {
                1 => {
                    let Some(slot) = source_slot(&ui, index, 1) else {
                        return;
                    };
                    let mut config = app_config.borrow_mut();
                    if config.mix.file_preview_volumes.len() <= slot {
                        let fallback = config.preview_volume;
                        config.mix.file_preview_volumes.resize(slot + 1, fallback);
                    }
                    config.mix.file_preview_volumes[slot] = value;
                    if cache.borrow().route_preview_slot == Some(slot) {
                        engine.set_monitor_volume(value);
                    }
                }
                2 => {
                    let selected = {
                        let cache = cache.borrow();
                        let ids = &cache.mic_ids;
                        let current = ui.get_mic_index().max(0) as usize;
                        ids.get(current)
                            .filter(|id| !id.is_empty())
                            .map(|id| (current, id.clone()))
                            .or_else(|| {
                                ids.iter()
                                    .enumerate()
                                    .find(|(_, id)| !id.is_empty())
                                    .map(|(index, id)| (index, id.clone()))
                            })
                    };
                    let Some((selected_index, id)) = selected else {
                        return;
                    };
                    ui.set_mic_index(selected_index as i32);
                    if let Err(error) = engine.set_mic_preview_source(Some(&id)) {
                        super::super::super::diagnostics::report_error(&ui, &error);
                        return;
                    }
                    let mut config = app_config.borrow_mut();
                    config.route.mic_id = Some(id);
                    config.mix.mic_preview_volume = value;
                    engine.set_mic_preview_volume(value);
                }
                _ => return,
            }
            if let Ok(row) = usize::try_from(index) {
                let volumes = ui.get_source_preview_volumes();
                if row < volumes.row_count() {
                    volumes.set_row_data(row, value * 100.0);
                }
            }
            save_queue.schedule();
        });
    }
    {
        let engine = state.engine.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = state.cache.clone();
        ui.on_microphone_input_volume_edited(move |value| {
            let volume = (value / 100.0).clamp(0.0, 1.0);
            engine.set_mic_volume(volume);
            cache.borrow_mut().mic_muted_volume = None;
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
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        // 静音不丢失原增益：先缓存当前值，恢复时优先取缓存再回退到配置。
        ui.on_source_mute_toggled(move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            let slot = source_slot(&ui, index, kind);
            let current = source_volume(&engine, kind, slot);
            let restore = if current > 0.001 {
                set_muted_volume(&cache, kind, slot, Some(current));
                0.0
            } else {
                take_muted_volume(&cache, kind, slot)
                    .unwrap_or_else(|| configured_source_volume(&app_config, kind, slot))
            };
            set_engine_source_volume(&engine, kind, slot, restore);
            // 先改引擎实时增益，再写缓存和持久化配置，避免刷新时 UI 与实际通道分离。
            {
                let mut cache_state = cache.borrow_mut();
                let mut config = app_config.borrow_mut();
                match kind {
                    0 => {
                        if let Some(slot) = slot {
                            set_slot(&mut cache_state.process_volumes, slot, restore);
                            set_slot(&mut config.mix.process_volumes, slot, restore);
                        }
                    }
                    1 => {
                        if let Some(slot) = slot {
                            set_slot(&mut cache_state.audio_volumes, slot, restore);
                            set_slot(&mut config.mix.sfx_volumes, slot, restore);
                        }
                    }
                    _ => config.mix.mic = restore,
                }
            }
            save_queue.schedule();
            if let Ok(row) = usize::try_from(index) {
                let states = ui.get_source_muted();
                if row < states.row_count() {
                    states.set_row_data(row, restore <= 0.001);
                }
                let volumes = ui.get_source_volumes();
                if row < volumes.row_count() {
                    volumes.set_row_data(row, restore * 100.0);
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
        // 启停操作只影响对应槽位；进程槽重建源列表，文件槽单独播放/停止。
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
                1 => {
                    super::audio_queue::toggle_file_source(&ui, &engine, &cache, index, enabled);
                    crate::ui_bridge::persist::save_process_selection(
                        &cache,
                        &app_config,
                        &save_queue,
                    );
                }
                // 卡片可在“未选择麦克风”占位状态显示；启用时先选首个真实设备。
                2 if enabled != ui.get_mix_mic() => {
                    if enabled {
                        let selected = {
                            let cache_state = cache.borrow();
                            microphone_to_enable(&cache_state.mic_ids, ui.get_mic_index())
                        };
                        if let Some(selected) = selected {
                            ui.set_mic_index(selected as i32);
                        }
                    }
                    ui.invoke_mic_mix_changed(enabled);
                }
                _ => {}
            }
        });
    }
    {
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let state_for_update = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_source_pause(move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if source_kind(&ui, index) != Some(1) {
                return;
            }
            let Some(slot) = source_slot(&ui, index, 1) else {
                return;
            };
            let preview = cache.borrow().route_preview_slot == Some(slot);
            if preview && (engine.file_state().playing || ui.get_player_paused()) {
                ui.invoke_player_pause_toggle();
            } else {
                super::audio_queue::play_current_slot(&ui, &state_for_update, slot);
            }
            super::super::super::sources::update(&ui, &state_for_update);
        });
    }
    {
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let ui_weak = ui.as_weak();
        // 滑块释放才换算目标媒体位置；停播或未知时长不产生隐式播放。
        ui.on_source_progress_seek(move |index, ratio| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if source_kind(&ui, index) != Some(1) {
                return;
            }
            let Some(slot) = source_slot(&ui, index, 1) else {
                return;
            };
            let preview = cache.borrow().route_preview_slot == Some(slot);
            let paused = preview && ui.get_player_paused();
            if !preview
                && !matches!(
                    engine.file_source_state(slot),
                    crate::engine::FileSourceState::Playing
                        | crate::engine::FileSourceState::Paused
                )
            {
                return;
            }
            let entry = {
                let cache = cache.borrow();
                cache
                    .route_audio_entries
                    .get(slot)
                    .and_then(|index| cache.audio_entries.get(*index))
                    .cloned()
            };
            let Some(entry) = entry.filter(|entry| entry.duration_hns > 0) else {
                return;
            };
            let target = (f64::from(ratio.clamp(0.0, 1.0)) * entry.duration_hns as f64) as u64;
            // 本机试听和已启用的路由槽须定位到同一媒体位置。
            if preview {
                if paused {
                    cache.borrow_mut().paused_audio = Some((
                        entry.clone(),
                        target,
                        crate::engine::PlaybackTarget::Monitor,
                    ));
                } else {
                    engine.ensure_monitor_default(true);
                    if let Err(error) =
                        engine.play_file(&entry, target, crate::engine::PlaybackTarget::Monitor)
                    {
                        super::super::super::diagnostics::report_error(&ui, &error);
                    }
                }
            }
            if engine.is_running() && cache.borrow().route_audio_enabled.get(slot) == Some(&true) {
                let result = engine
                    .stop_file_source(slot)
                    .and_then(|_| engine.play_file_source(&entry, slot, false, target, paused));
                if let Err(error) = result {
                    super::super::super::diagnostics::report_error(&ui, &error);
                }
            }
        });
    }
    {
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let ui_weak = ui.as_weak();
        let state_for_update = state.clone();
        // 停止文件源不会清除其他 SFX 槽，保证每张音频卡片生命周期独立。
        ui.on_source_stop(move |index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if source_kind(&ui, index) == Some(1) {
                let Some(slot) = source_slot(&ui, index, 1) else {
                    return;
                };
                if cache.borrow().route_preview_slot == Some(slot) {
                    ui.invoke_audio_stop();
                    cache.borrow_mut().route_preview_slot = None;
                }
                let _ = engine.stop_file_source(slot);
                super::super::super::sources::update(&ui, &state_for_update);
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
        // action 编号：1 热键、2 单独转发、3 定位、4 删除、5/6/7 替换、8 停止文件。
        move |index, action| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(kind) = source_kind(&ui, index) else {
                return;
            };
            match action {
                1 if kind == 1 => {
                    if let Some(entry_index) =
                        crate::ui_bridge::library::file_context_index(&ui, &engine, &cache)
                    {
                        ui.set_page(1);
                        ui.set_audio_entry_index(entry_index);
                        ui.invoke_audio_entry_hotkey_bind();
                    }
                }
                2 => {
                    super::super::route_start::start_source_only(
                        &ui,
                        &state_for_update,
                        kind,
                        index,
                    );
                }
                3 if kind == 1 => {
                    if let Some(entry_index) =
                        crate::ui_bridge::library::file_context_index(&ui, &engine, &cache)
                    {
                        ui.set_page(1);
                        ui.set_audio_entry_index(entry_index);
                    }
                }
                5 if kind == 0 => {
                    ui.set_source_replace_index(index);
                    ui.set_source_add_tab(0);
                    super::super::super::window_lifecycle::open_add_source_window(
                        &ui,
                        &state_for_update,
                    );
                }
                6 if kind == 1 => {
                    ui.set_source_replace_index(index);
                    ui.set_source_add_tab(1);
                    super::super::super::window_lifecycle::open_add_source_window(
                        &ui,
                        &state_for_update,
                    );
                }
                7 if kind == 2 => {
                    ui.set_source_replace_index(index);
                    ui.set_source_add_tab(2);
                    super::super::super::window_lifecycle::open_add_source_window(
                        &ui,
                        &state_for_update,
                    );
                }
                8 if kind == 1 => {
                    if let Some(slot) = source_slot(&ui, index, 1) {
                        let _ = engine.stop_file_source(slot);
                    }
                }
                4 => {
                    delete_source(&ui, &cache, &app_config, &engine, kind, index);
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

/// 删除指定类型的源，并同步压缩配置中的同槽设备集合。
fn delete_source(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    app_config: &Rc<RefCell<config::AppConfig>>,
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
            // 混合源行号不能直接索引进程数组，先按可见进程行计算同类槽位。
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
            // 删除 PID 后必须同步压缩所有进程并行数组，否则后续槽位会错配音量和输出。
            if let Some(slot) = cache_state
                .selected_pids
                .iter()
                .position(|selected| *selected == pid)
            {
                cache_state.selected_pids.remove(slot);
                cache_state
                    .disabled_pids
                    .retain(|disabled| *disabled != pid);
                remove_slot(&mut cache_state.process_volumes, slot);
                remove_slot(&mut cache_state.process_muted_volumes, slot);
                remove_slot(&mut app_config.borrow_mut().output.process_output_ids, slot);
            }
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
            // 文件卡片的槽位由它之前出现的文件行数量决定，与进程卡片互不共享索引。
            let file_slot = (0..source_index)
                .filter(|row| kinds.row_data(*row) == Some(1))
                .count();
            let mut cache_state = cache.borrow_mut();
            // 删除条目时一起移除该槽的启用、增益、静音恢复值和输出目标，保持数组同长。
            if file_slot < cache_state.route_audio_entries.len() {
                // 旧配置可能未显式存储前面的槽位，先补齐以免删除时错移后面的增益。
                let mut config = app_config.borrow_mut();
                let count = cache_state.route_audio_entries.len();
                let fallback = config.preview_volume;
                config.mix.file_preview_volumes.resize(count, fallback);
                drop(config);
                cache_state.route_audio_entries.remove(file_slot);
                remove_slot(&mut cache_state.route_audio_enabled, file_slot);
                remove_slot(&mut cache_state.audio_play_modes, file_slot);
                remove_slot(&mut cache_state.audio_finished_handled, file_slot);
                remove_slot(&mut cache_state.audio_volumes, file_slot);
                remove_slot(&mut cache_state.audio_muted_volumes, file_slot);
                remove_slot(
                    &mut app_config.borrow_mut().output.file_output_ids,
                    file_slot,
                );
                remove_slot(
                    &mut app_config.borrow_mut().mix.file_preview_volumes,
                    file_slot,
                );
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
            if ui.get_running() {
                // 引擎只停止被删除的文件槽；音频库独立播放流另行停止，不能波及其他路由卡片。
                let _ = engine.stop_file_source(file_slot);
            }
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
/// 显式启用麦克风时跳过“不混入”哨兵，同时保留已选的真实设备。
fn microphone_to_enable(ids: &[String], selected: i32) -> Option<usize> {
    if let Ok(selected) = usize::try_from(selected)
        && selected > 0
        && ids.get(selected).is_some_and(|id| !id.is_empty())
    {
        return Some(selected);
    }
    ids.iter().position(|id| !id.is_empty())
}

/// 从 Slint 源行模型读取源类型；未知或越界行不参与任何引擎操作。
fn source_kind(ui: &MainWindow, index: i32) -> Option<usize> {
    ui.get_source_kinds()
        .row_data(index as usize)
        .map(|kind| kind as usize)
}
/// 运行中按可见顺序重建进程槽；只替换采集源，不重建输出设备。
/// 运行中把已选进程列表重新映射到采集槽位：按槽位替换，不重建输出。
fn hot_swap_process_sources(engine: &AudioEngine, cache: &Rc<RefCell<UiCache>>) {
    let wanted: Vec<u32> = {
        // 引擎槽位按过滤后的可用 PID 顺序重建，因此退出或禁用的进程不会占用采集槽。
        let cache = cache.borrow();
        cache
            .selected_pids
            .iter()
            .copied()
            // selected_pids 保留用户排序，但只有仍可枚举且未禁用的 PID 才能提交给引擎。
            .filter(|pid| cache.pids.contains(pid) && !cache.disabled_pids.contains(pid))
            .collect()
    };
    if let Err(error) = engine.replace_process_sources(&wanted) {
        tracing::warn!("重建进程源失败：{error}");
    }
}

/// 切换进程卡片的禁用标记，返回值表示是否找到并修改了有效 PID。
fn toggle_process_source_row(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    source_index: i32,
) -> bool {
    let Ok(source_index) = usize::try_from(source_index) else {
        return false;
    };
    let source_kinds = ui.get_source_kinds();
    // 禁用状态按 PID 保存而非按 UI 行保存，源列表刷新后仍能对应到同一进程。
    let kinds = (0..source_kinds.row_count())
        .map(|row| source_kinds.row_data(row).unwrap_or(-1))
        .collect::<Vec<_>>();
    let Some(slot) = process_source_slot(&kinds, source_index) else {
        return false;
    };
    let cache_state = cache.borrow();
    // 可见槽位过滤掉已退出 PID 后再取真实 PID，防止点击卡片误切换错误进程。
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

/// 将混合源行号映射为同类进程源槽位，类型 0 以外的行不会产生槽位。
fn process_source_slot(source_kinds: &[i32], source_index: usize) -> Option<usize> {
    // 只有进程行才产生槽位；返回值等于此前 kind=0 行数，正是 selected_pids 的下标。
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
/// 将混合源行号映射到指定类型的同类槽位，用于输出和音量数组索引。
fn source_slot(ui: &MainWindow, source_index: i32, kind: usize) -> Option<usize> {
    // 同类槽位从混合列表中独立计数，使进程、文件的增益/输出数组互不串位。
    let source_index = usize::try_from(source_index).ok()?;
    let kinds = ui.get_source_kinds();
    (kinds.row_data(source_index) == Some(kind as i32)).then(|| {
        (0..source_index)
            .filter(|row| kinds.row_data(*row) == Some(kind as i32))
            .count()
    })
}
/// 按可见进程槽过滤掉已退出或不可用 PID，再返回对应的选择项。
fn visible_process_pid(
    selected_pids: &[u32],
    available_pids: &[u32],
    visible_slot: usize,
) -> Option<u32> {
    // selected_pids 中可能残留已退出进程，先按 available_pids 过滤再解释 visible_slot。
    selected_pids
        .iter()
        .filter(|pid| available_pids.contains(pid))
        .nth(visible_slot)
        .copied()
}

/// 读取引擎中对应源槽的当前增益；无槽位时回退到该类型的默认增益。
fn source_volume(engine: &AudioEngine, kind: usize, slot: Option<usize>) -> f32 {
    match kind {
        0 => slot
            .map(|slot| engine.process_volume(slot))
            .unwrap_or_else(|| engine.volume()),
        1 => slot
            .map(|slot| engine.sfx_volume_for_slot(slot))
            .unwrap_or_else(|| engine.sfx_volume()),
        _ => engine.mic_volume(),
    }
}

/// 读取配置中的槽位增益，供静音恢复和尚未初始化的引擎槽使用。
fn configured_source_volume(
    app_config: &Rc<RefCell<config::AppConfig>>,
    kind: usize,
    slot: Option<usize>,
) -> f32 {
    let config = app_config.borrow();
    match kind {
        0 => slot
            .and_then(|slot| config.mix.process_volumes.get(slot).copied())
            .unwrap_or(config.mix.process),
        1 => slot
            .and_then(|slot| config.mix.sfx_volumes.get(slot).copied())
            .unwrap_or(config.mix.sfx),
        _ => config.mix.mic,
    }
}

/// 把归一化增益写入对应的进程、文件或麦克风引擎通道。
fn set_engine_source_volume(engine: &AudioEngine, kind: usize, slot: Option<usize>, value: f32) {
    match kind {
        0 => {
            if let Some(slot) = slot {
                engine.set_process_volume(slot, value);
            }
        }
        1 => {
            if let Some(slot) = slot {
                engine.set_sfx_volume_for_slot(slot, value);
            }
        }
        _ => engine.set_mic_volume(value),
    }
}

/// 同时更新引擎、缓存和配置，确保下一次刷新仍保持当前卡片音量。
fn set_source_volume(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    engine: &AudioEngine,
    app_config: &Rc<RefCell<config::AppConfig>>,
    kind: usize,
    source_index: i32,
    value: f32,
) {
    let slot = source_slot(ui, source_index, kind);
    set_engine_source_volume(engine, kind, slot, value);
    let mut cache = cache.borrow_mut();
    match kind {
        0 => {
            if let Some(slot) = slot {
                set_slot(&mut cache.process_volumes, slot, value);
                set_slot(&mut cache.process_muted_volumes, slot, None);
                set_slot(
                    &mut app_config.borrow_mut().mix.process_volumes,
                    slot,
                    value,
                );
            }
        }
        1 => {
            if let Some(slot) = slot {
                set_slot(&mut cache.audio_volumes, slot, value);
                set_slot(&mut cache.audio_muted_volumes, slot, None);
                set_slot(&mut app_config.borrow_mut().mix.sfx_volumes, slot, value);
            }
        }
        _ => {
            cache.mic_muted_volume = None;
            app_config.borrow_mut().mix.mic = value;
        }
    }
}

/// 保存静音前的增益；None 表示该槽位尚未缓存可恢复值。
fn set_muted_volume(
    cache: &Rc<RefCell<UiCache>>,
    kind: usize,
    slot: Option<usize>,
    value: Option<f32>,
) {
    let Some(slot) = slot else { return };
    let mut cache = cache.borrow_mut();
    match kind {
        0 => set_slot(&mut cache.process_muted_volumes, slot, value),
        1 => set_slot(&mut cache.audio_muted_volumes, slot, value),
        _ => cache.mic_muted_volume = value,
    }
}

/// 取出并清除静音缓存，避免旧值跨多次静音操作污染恢复结果。
fn take_muted_volume(
    cache: &Rc<RefCell<UiCache>>,
    kind: usize,
    slot: Option<usize>,
) -> Option<f32> {
    let slot = slot?;
    let mut cache = cache.borrow_mut();
    match kind {
        0 => cache.process_muted_volumes.get_mut(slot)?.take(),
        1 => cache.audio_muted_volumes.get_mut(slot)?.take(),
        _ => cache.mic_muted_volume.take(),
    }
}

/// 按槽位写入向量，并补齐中间缺失位置以保持 UI 行与引擎槽位对齐。
fn set_slot<T: Clone>(values: &mut Vec<T>, slot: usize, value: T) {
    if values.len() <= slot {
        values.resize(slot + 1, value.clone());
    }
    values[slot] = value;
}

/// 删除槽位的并行状态；所有同类数组必须调用此函数保持索引同步。
fn remove_slot<T>(values: &mut Vec<T>, slot: usize) {
    if slot < values.len() {
        values.remove(slot);
    }
}
#[cfg(test)]
#[path = "../route/tests.rs"]
mod tests;
