//! 音频库播放回调；播放线程、解码和试听均由 AudioEngine 管理。

use super::super::*;
use crate::engine::PlaybackTarget;
use slint::Model;
use std::time::{Duration, Instant};

/// 绑定播放器的音量、播放、暂停、定位、停止和前后导航回调。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_play_action(ui, state);
    register_double_click(ui, state);
    register_seek(ui, state);
    register_stop(ui, state);
    register_pause(ui, state);
    register_navigation(ui, state);
    register_file_volume(ui, state);
}

/// 修改独立试听槽的音量；百分比 UI 值转换为引擎归一化增益并持久化默认值。
fn register_file_volume(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_file_volume_edited(move |value| {
        // Slint 传入百分比，试听槽只接受 0..1 增益；两者在这里完成边界收敛。
        let volume = (value / 100.0).clamp(0.0, 1.0);
        engine.set_sfx_volume_for_slot(crate::engine::PLAYBACK_SLOT, volume);
        app_config.borrow_mut().mix.sfx = volume;
        save_queue.schedule();
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_audio_file_volume(volume * 100.0);
        }
    });
}

/// 记录播放目标（监听器、路由或两者）后复用双击入口启动条目。
fn register_play_action(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_play(move |mode| {
        if let Some(ui) = ui_weak.upgrade() {
            let target = playback_target(mode);
            // 播放入口先把目标保存到缓存，再复用双击回调，确保列表和热键走同一启动路径。
            cache.borrow_mut().playback_target = target;
            cache.borrow_mut().force_audio_play = true;
            ui.invoke_audio_entry_double_clicked(ui.get_audio_entry_index());
        }
    });
}

/// 处理单击/双击与强制播放；同一条目再次双击只切换暂停状态。
fn register_double_click(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_double_clicked(move |index| {
        let now = Instant::now();
        // 强制播放必须优先于双击判定；普通点击只记录时间，不应立即启动解码。
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
        // index 是当前可见列表索引，selected_entry 负责把搜索/分页索引还原为真实条目。
        ui.set_audio_entry_index(index);
        if !should_play {
            return;
        }
        let Some(entry) = selected_entry(&cache, index as usize) else {
            return;
        };
        // 播放中的同一条目双击只暂停/继续，避免重新解码导致播放位置丢失。
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
        cache.borrow_mut().route_preview_slot = None;
        cache.borrow_mut().playback_target = target;
        ui.set_player_paused(false);
        stop_ui(&ui, &engine, false);
        start_ui(&ui, &engine, &entry, 0, target);
    });
}

/// 将进度比例转换为 100ns 时间戳，重建临时条目后从该位置继续播放。
fn register_seek(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
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
        // 定位通过重启文件流实现，旧流必须先停掉，否则两个解码任务可能同时写入试听槽。
        engine.stop_file();
        let target = cache.borrow().playback_target;
        start_ui(&ui, &engine, &entry, target_hns, target);
        if let Some(slot) = cache.borrow().route_preview_slot {
            let config = app_config.borrow().file_preview_gain(slot);
            engine.set_monitor_volume(config);
        }
        if let Some(slot) = cache.borrow().route_preview_slot
            && engine.is_running()
            && cache.borrow().route_audio_enabled.get(slot) == Some(&true)
        {
            let _ = engine.stop_file_source(slot);
            let _ = engine.play_file_source(&entry, slot, false, target_hns, false);
        }
        ui.set_audio_progress(position);
        ui.set_audio_current_position(format_hns_position(target_hns, entry.duration_hns).into());
    });
}

/// 停止试听槽并清空播放器选择，同时恢复预览监听策略。
fn register_stop(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_stop(move || {
        // 试听文件使用独立槽位，停止这里只能终止当前文件，不能清理路由中的其他音频槽。
        if let Some(slot) = cache.borrow().route_preview_slot {
            let _ = engine.stop_file_source(slot);
        }
        engine.stop_file();
        cache.borrow_mut().paused_audio = None;
        cache.borrow_mut().playback_target = PlaybackTarget::Both;
        cache.borrow_mut().route_preview_slot = None;
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_player_paused(false);
            ui.set_player_entry_index(-1);
            engine.set_monitor_default(ui.get_preview_enabled());
            reset_ui(&ui);
        }
    });
}

/// 在暂停快照与引擎当前文件之间切换；快照保存条目、位置和播放目标。
fn register_pause(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let ui_weak = ui.as_weak();
    ui.on_player_pause_toggle(move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        if ui.get_player_paused() {
            // 暂停不保留引擎句柄，而是转移条目、位置和目标，恢复时重新创建文件流。
            let paused = cache.borrow_mut().paused_audio.take();
            if let Some((entry, position, target)) = paused {
                start_ui(&ui, &engine, &entry, position, target);
                if let Some(slot) = cache.borrow().route_preview_slot {
                    engine.set_monitor_volume(app_config.borrow().file_preview_gain(slot));
                }
                ui.set_player_paused(false);
                sync_route_card_pause(&engine, &cache, false);
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
        // 先保存当前位置再停流，避免异步停止后无法恢复到用户看到的进度。
        cache.borrow_mut().paused_audio = Some((entry, file.position_hns, target));
        if file.duration_hns > 0 {
            ui.set_audio_progress(file.position_hns as f32 / file.duration_hns as f32);
            ui.set_audio_current_position(
                format_hns_position(file.position_hns, file.duration_hns).into(),
            );
        }
        engine.stop_file();
        sync_route_card_pause(&engine, &cache, true);
        engine.set_monitor_default(ui.get_preview_enabled());
        ui.set_player_paused(true);
        ui.set_player_playing(false);
    });
}

/// 音频库常显控制条操作路由卡片时同步该文件槽；其它卡片的路由状态不受影响。
fn sync_route_card_pause(engine: &AudioEngine, cache: &Rc<RefCell<UiCache>>, paused: bool) {
    let cache = cache.borrow();
    if let Some(slot) = cache.route_preview_slot
        && engine.is_running()
        && cache.route_audio_enabled.get(slot) == Some(&true)
    {
        if !paused
            && engine.file_source_state(slot) == crate::engine::FileSourceState::Stopped
            && let Some(entry) = cache
                .route_audio_entries
                .get(slot)
                .and_then(|index| cache.audio_entries.get(*index))
        {
            let _ = engine.play_file_source(
                entry,
                slot,
                false,
                engine.file_state().position_hns,
                false,
            );
        } else {
            let _ = engine.set_file_source_paused(slot, paused);
        }
    }
}

/// 按可见列表相邻项导航，停止旧文件后在同一播放目标启动新条目。
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

/// 按方向计算可见索引并启动相邻音频条目。
fn navigate(
    ui_weak: &slint::Weak<MainWindow>,
    cache: &Rc<RefCell<UiCache>>,
    engine: &AudioEngine,
    direction: i32,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    // 导航沿 Slint 当前可见顺序移动，越界时保持当前播放状态不变。
    let current = ui.get_audio_entry_index();
    let next = current + direction;
    if next < 0 || next >= ui.get_audio_entries().row_count() as i32 {
        return;
    }
    let Some(entry) = selected_entry(cache, next as usize) else {
        return;
    };
    ui.set_audio_entry_index(next);
    // 导航沿用当前目标设备，并清除旧暂停快照，避免下一次暂停恢复到上一首。
    let target = cache.borrow().playback_target;
    cache.borrow_mut().paused_audio = None;
    cache.borrow_mut().route_preview_slot = None;
    engine.stop_file();
    start_ui(&ui, engine, &entry, 0, target);
}

/// 将分页/搜索可见索引解析为缓存中的音频条目副本。
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

/// 启动播放器并同步 UI；同步确保监听设备就绪，失败统一上报错误。
fn start_ui(
    ui: &MainWindow,
    engine: &AudioEngine,
    entry: &config::AudioEntry,
    start_hns: u64,
    target: PlaybackTarget,
) {
    // 输出打开是异步边界，先确保设备可用再投递解码任务，避免假播放状态。
    // 同步确保试听设备就绪：异步打开与随后的解码投递有竞态，
    // 会表现为「显示播放中但没有声音」。
    // 音频库播放使用全局试听音量；路由卡片播放随后覆写自己的本机分支。
    engine.set_monitor_volume((ui.get_preview_volume() / 100.0).clamp(0.0, 1.0));
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

/// 停止文件播放；可选恢复系统默认监听设备。
fn stop_ui(ui: &MainWindow, engine: &AudioEngine, restore_monitor: bool) {
    if restore_monitor {
        engine.set_monitor_default(ui.get_preview_enabled());
    }
    engine.stop_file();
    reset_ui(ui);
}

/// 清空播放器的运行、暂停、选择和进度字段，作为所有停止/失败路径的共同收尾。
fn reset_ui(ui: &MainWindow) {
    ui.set_player_playing(false);
    ui.set_player_paused(false);
    ui.set_player_entry_index(-1);
    ui.set_audio_progress(0.0);
    ui.set_audio_duration_seconds(0.0);
    ui.set_audio_current_position("".into());
}

/// 把 Slint 模式编号转换为播放器目标枚举：1 监听、2 路由、其他为两者。
fn playback_target(mode: i32) -> PlaybackTarget {
    match mode {
        1 => PlaybackTarget::Monitor,
        2 => PlaybackTarget::Route,
        _ => PlaybackTarget::Both,
    }
}
