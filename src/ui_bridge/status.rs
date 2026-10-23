//! 轮询引擎启停状态、音频播放进度和全局输出设备名称。

use super::*;
use crate::engine::EngineStatus;
use slint::Model;
use slint::{Timer, TimerMode};
use std::cell::Cell;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// 创建 50 ms 周期的 UI 定时器；所有引擎状态读取和模型更新都回到 Slint 线程。
pub(super) fn start_status_timer(ui: &MainWindow, state: &AppState) -> Timer {
    let ui_weak = ui.as_weak();
    let state = state.clone();
    // 窗口句柄只在定时器创建时查找一次，拖动期间只采集轻量屏幕坐标。
    let cursor_window = crate::platform::window::find_current_window("Miccat");
    let last_dest_idx = Cell::new(ui.get_device_index());
    let last_diag_unseen = Cell::new(0usize);
    let mut source_meter_history = SourceMeterHistory::default();
    let timer = Timer::default();
    // 轮询固定在 UI 线程执行：后台只准备列表结果，Slint 模型和窗口属性在此处批量提交。
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        // 按 50ms 轮询屏幕光标并换算到主窗口逻辑坐标，离开客户区后仍持续更新。
        if ui.get_volume_adjusting()
            && let Some(hwnd) = cursor_window
            && let Some((pointer_x, pointer_y)) =
                crate::platform::window::cursor_position_in_client(hwnd)
        {
            let scale = ui.window().scale_factor().max(0.1);
            ui.set_volume_pointer_x(pointer_x as f32 / scale);
            ui.set_volume_pointer_y(pointer_y as f32 / scale);
            ui.set_volume_pointer_valid(true);
        }
        // 窗口关闭后弱引用升级失败，定时器保持安全空操作而不延长窗口生命周期。
        apply_list_refresh(&ui, &state);
        // 先应用枚举结果，再读取引擎状态，避免同一 tick 使用已过期的设备或进程模型。
        update_engine_status(&ui, &state);
        super::sources::update_dynamic(&ui, &state);
        // 完成态在动态模型更新后仅处理一次，按卡片模式续播或停在末尾。
        super::callbacks::poll_finished_route_audio(&ui, &state);
        update_source_meter(&ui, &state, &mut source_meter_history);
        update_player_progress(&ui, &state);
        update_device_label(&ui, &state, &last_dest_idx);
        ui.set_master_volume_db(format_gain_db(state.engine.master_volume()).into());
        super::diagnostics::sync(&ui, &last_diag_unseen);
    });
    timer
}
/// 电平历史保留的采样点数，供 UI 绘制最近一段时间的峰值轨迹。
const PEAK_HISTORY_LENGTH: usize = 32;

/// 按当前源行结构保存峰值历史；行结构变化时历史必须整体重置。
#[derive(Default)]
struct SourceMeterHistory {
    /// 源行的种类，0/1/2 分别对应进程、文件混音和麦克风。
    kinds: Vec<i32>,
    /// 源行标题快照，用于发现卡片重排而不是沿用旧电平。
    titles: Vec<slint::SharedString>,
    /// 所有源行串联的峰值历史，按行再按时间点排列。
    samples: Vec<f32>,
    /// 主路由的独立峰值历史。
    master_samples: Vec<f32>,
}

/// 把引擎峰值映射到当前 UI 源行，并只在数据或历史发生变化时更新模型。
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
    // 行类型或标题改变意味着槽位映射可能改变，旧峰值不能归到新的卡片上。
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
                    // process_slot 是源行到引擎进程槽位的映射；失效 PID 不应推进引擎槽位。
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
        // 每行历史固定占 PEAK_HISTORY_LENGTH 个元素，模型长度稳定才能让 Slint 绘图复用。
        let offset = index * PEAK_HISTORY_LENGTH;
        // 固定长度历史按行宽平移，尾部写入本次峰值以保持时间方向一致。
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
    // 释放缓存借用后再写 UI，避免模型回调期间持有缓存借用造成重入冲突。
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
/// 将线性增益（0..1）转换为显示用分贝值。
fn format_gain_db(value: f32) -> String {
    if value <= 0.001 {
        return "-∞ dB".into();
    }
    format!("{:.1} dB", 20.0 * value.log10())
}
/// 将线性峰值转换为显示用分贝值；极低值显示为静音符号。
fn format_peak_db(value: f32) -> String {
    if value <= 0.0001 {
        return "-∞ dB".into();
    }
    format!("{:.1} dB", 20.0 * value.log10())
}
/// 消费后台枚举结果，并在 UI 线程一次性替换进程、麦克风和输出模型。
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
    // pending 是唤醒标记而非数据本身，消费结果后清零才能允许下一轮枚举再次入队。
    // pending 只表示有结果可消费；取出结果后释放锁，再在 UI 线程更新模型。
    match result {
        Ok(lists) => {
            super::lists::apply_process_list(ui, &state.cache, lists.processes);
            super::lists::apply_mics(ui, &state.cache, lists.mics);
            super::lists::apply_destinations(ui, &state.cache, lists.destinations, false);
            remove_missing_audio_sources(ui, state, &lists.missing_audio_paths);
            if ui.get_mix_mic() && ui.get_mic_index() == 0 {
                ui.invoke_mic_mix_changed(false);
            }
            // 刷新可能删掉进程、文件或麦克风卡片，结构模型必须在同一轮重建。
            super::sources::update(ui, state);
            if state.windows.add_source.window().is_visible() {
                super::window_lifecycle::sync_add_source_window(
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

/// 删除刷新时已不存在的音频文件卡片，并同步停止对应 SFX 槽和持久化配置。
fn remove_missing_audio_sources(ui: &MainWindow, state: &AppState, missing_paths: &[String]) {
    let mut removed_slots = Vec::new();
    let changed = {
        let mut cache = state.cache.borrow_mut();
        if cache.route_audio_entries.is_empty() {
            false
        } else {
            let old_len = cache.route_audio_entries.len();
            let entries = std::mem::take(&mut cache.route_audio_entries);
            let enabled = std::mem::take(&mut cache.route_audio_enabled);
            let play_modes = std::mem::take(&mut cache.audio_play_modes);
            let finished = std::mem::take(&mut cache.audio_finished_handled);
            let volumes = std::mem::take(&mut cache.audio_volumes);
            let muted = std::mem::take(&mut cache.audio_muted_volumes);
            let outputs = state.app_config.borrow().output.file_output_ids.clone();
            let fallback_outputs = state.app_config.borrow().output.device_ids.clone();
            let mut kept_entries = Vec::new();
            let mut kept_enabled = Vec::new();
            let mut kept_play_modes = Vec::new();
            let mut kept_finished = Vec::new();
            let mut kept_volumes = Vec::new();
            let mut kept_muted = Vec::new();
            let mut kept_outputs = Vec::new();
            for (slot, index) in entries.into_iter().enumerate() {
                let exists = cache
                    .audio_entries
                    .get(index)
                    .is_some_and(|entry| !missing_paths.contains(&entry.path));
                if !exists {
                    removed_slots.push(slot);
                    continue;
                }
                kept_entries.push(index);
                kept_enabled.push(enabled.get(slot).copied().unwrap_or(false));
                kept_play_modes.push(play_modes.get(slot).copied().unwrap_or(0));
                kept_finished.push(finished.get(slot).copied().unwrap_or(false));
                kept_volumes.push(volumes.get(slot).copied().unwrap_or(1.0));
                kept_muted.push(muted.get(slot).copied().flatten());
                kept_outputs.push(
                    outputs
                        .get(slot)
                        .cloned()
                        .unwrap_or_else(|| fallback_outputs.clone()),
                );
            }
            let changed = kept_entries.len() != old_len;
            cache.route_audio_entries = kept_entries;
            cache.route_audio_enabled = kept_enabled;
            cache.audio_play_modes = kept_play_modes;
            cache.audio_finished_handled = kept_finished;
            cache.audio_volumes = kept_volumes;
            cache.audio_muted_volumes = kept_muted;
            if changed && !outputs.is_empty() {
                state.app_config.borrow_mut().output.file_output_ids = kept_outputs;
            }
            cache.route_audio_entry = cache.route_audio_entries.first().copied();
            cache.route_file_enabled = cache.route_audio_enabled.iter().any(|enabled| *enabled);
            cache.file_source_hidden = cache.route_audio_entries.is_empty();
            changed
        }
    };
    if !changed {
        return;
    }
    if state.engine.is_running() {
        for slot in removed_slots {
            let _ = state.engine.stop_file_source(slot);
        }
    }
    super::persist::save_process_selection(&state.cache, &state.app_config, &state.save_queue);
    ui.set_audio_entry_index(-1);
}

/// 把 AudioEngine 的生命周期状态投影到状态文案，并在结束态恢复系统默认麦克风。
fn update_engine_status(ui: &MainWindow, state: &AppState) {
    match state.engine.status() {
        EngineStatus::Running => {
            // 只有状态真的跨过边界才改写文案，避免 50ms 轮询反复触发同一批 UI 更新。
            if !ui.get_running() {
                ui.set_running(true);
                ui.set_status_text("运行中".into());
                ui.set_status_kind(1);
            }
        }
        EngineStatus::Idle => {
            // 引擎回到空闲态时恢复启动前的系统麦克风，避免路由退出后遗留占用。
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

/// 同步试听文件的播放、时长和拖动进度；用户拖动时不覆盖其临时位置。
fn update_player_progress(ui: &MainWindow, state: &AppState) {
    if ui.get_audio_entry_index() >= 0
        && let Some(actual) = visible_entry(&state.cache, ui.get_audio_entry_index() as usize)
        && let Some(entry) = state.cache.borrow().audio_entries.get(actual)
    {
        ui.set_player_loop(entry.loop_playback);
    }
    // 拖动或暂停期间 UI 是临时编辑态，不能被引擎的异步位置覆盖。
    if ui.get_player_paused() {
        return;
    }
    let file = state.engine.file_state();
    if let Some(error) = file.error {
        ui.set_player_playing(false);
        super::diagnostics::report_warning(ui, &error);
        return;
    }
    // 文件状态是试听进度的权威来源；错误和自然结束都先同步播放按钮，再处理位置。
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

/// 仅在主输出下标变化时刷新设备说明，避免每个定时 tick 重写文案。
fn update_device_label(ui: &MainWindow, state: &AppState, last_dest_idx: &Cell<i32>) {
    let device_index = ui.get_device_index();
    // 设备索引不变时说明文案无需重算，减少定时器对 Slint 属性的重复写入。
    if device_index != last_dest_idx.get() {
        last_dest_idx.set(device_index);
        update_output_text(ui, &state.cache);
    }
}
