//! 路由音频卡片队列：按音频库分类切换条目，并处理四种播放模式。

use super::super::super::*;
use crate::config::AudioEntry;
use crate::engine::FileSourceState;
use slint::Model;
use std::time::{SystemTime, UNIX_EPOCH};

/// 顺序播放：播到分类末尾后停止。
const MODE_SEQUENTIAL: u8 = 0;
/// 列表循环：末尾回到分类第一首。
const MODE_LIST_LOOP: u8 = 1;
/// 单曲循环：完成后重新播放当前条目。
const MODE_SINGLE_LOOP: u8 = 2;
/// 随机播放：优先选择分类内不同的条目。
const MODE_SHUFFLE: u8 = 3;
/// 播放模式总数；配置中的未知值统一归一化到顺序播放。
const MODE_COUNT: u8 = 4;

/// 注册卡片上一首、下一首和播放模式切换回调。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_step(ui, state);
    register_mode(ui, state);
}

/// 绑定上一首/下一首；两个方向共用分类队列选择逻辑。
fn register_step(ui: &MainWindow, state: &AppState) {
    let ui_weak = ui.as_weak();
    let previous_state = state.clone();
    ui.on_source_previous(move |row| {
        let Some(ui) = ui_weak.upgrade() else { return };
        step_row(&ui, &previous_state, row, -1);
    });

    let ui_weak = ui.as_weak();
    let next_state = state.clone();
    ui.on_source_next_track(move |row| {
        let Some(ui) = ui_weak.upgrade() else { return };
        step_row(&ui, &next_state, row, 1);
    });
}

/// 绑定播放模式循环切换，并同步缓存、Slint 模型和配置。
fn register_mode(ui: &MainWindow, state: &AppState) {
    let ui_weak = ui.as_weak();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    ui.on_source_play_mode_cycled(move |row| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let Some(slot) = file_slot_for_row(&ui, row) else {
            return;
        };
        let mode = {
            let mut cache = cache.borrow_mut();
            ensure_slot(&mut cache.audio_play_modes, slot, MODE_SEQUENTIAL);
            let mode = next_mode(cache.audio_play_modes[slot]);
            cache.audio_play_modes[slot] = mode;
            mode
        };
        let modes = ui.get_source_play_modes();
        let Ok(row) = usize::try_from(row) else {
            return;
        };
        if row < modes.row_count() {
            modes.set_row_data(row, i32::from(mode));
        }
        super::super::super::persist::save_process_selection(&cache, &app_config, &save_queue);
    });
}

/// 50 ms 状态轮询调用一次；每个完成态只处理一次，避免重复切歌。
pub(in crate::ui_bridge) fn poll_finished(ui: &MainWindow, state: &AppState) {
    let slots = state.cache.borrow().route_audio_entries.len();
    for slot in 0..slots {
        let finished = state.engine.file_source_state(slot) == FileSourceState::Finished;
        let should_advance = {
            let mut cache = state.cache.borrow_mut();
            ensure_slot(&mut cache.audio_finished_handled, slot, false);
            let should_advance = finished && !cache.audio_finished_handled[slot];
            cache.audio_finished_handled[slot] = finished;
            should_advance
        };
        if should_advance {
            advance_finished_slot(ui, state, slot);
        }
    }
}

/// 用户按卡片前后按钮时，在该条目所属分类内切换并立即播放。
fn step_row(ui: &MainWindow, state: &AppState, row: i32, direction: i32) {
    if !ui.get_running() {
        return;
    }
    let Some(slot) = file_slot_for_row(ui, row) else {
        return;
    };
    let selected = {
        let cache = state.cache.borrow();
        cache
            .route_audio_entries
            .get(slot)
            .copied()
            .and_then(|current| {
                let candidates = category_indices(
                    &cache.audio_entries,
                    &cache.route_audio_entries,
                    slot,
                    current,
                );
                adjacent_index(&candidates, current, direction, true)
            })
    };
    if let Some(selected) = selected {
        play_slot(ui, state, slot, selected);
    }
}

/// 自然播放完成后按当前模式选择下一条；顺序播放末尾保持完成态。
fn advance_finished_slot(ui: &MainWindow, state: &AppState, slot: usize) {
    let selected = {
        let cache = state.cache.borrow();
        let current = match cache.route_audio_entries.get(slot).copied() {
            Some(current) => current,
            None => return,
        };
        let mode = normalized_mode(
            cache
                .audio_play_modes
                .get(slot)
                .copied()
                .unwrap_or(MODE_SEQUENTIAL),
        );
        let candidates = category_indices(
            &cache.audio_entries,
            &cache.route_audio_entries,
            slot,
            current,
        );
        match mode {
            MODE_SINGLE_LOOP => Some(current),
            MODE_LIST_LOOP => adjacent_index(&candidates, current, 1, true),
            MODE_SHUFFLE => shuffled_index(&candidates, current, shuffle_seed()),
            _ => adjacent_index(&candidates, current, 1, false),
        }
    };
    if let Some(selected) = selected {
        play_slot(ui, state, slot, selected);
    }
}

/// 更新卡片条目并启动对应 SFX 槽；模式续播不改变“启用转发”状态。
fn play_slot(ui: &MainWindow, state: &AppState, slot: usize, entry_index: usize) {
    let entry = {
        let mut cache = state.cache.borrow_mut();
        let Some(entry) = cache.audio_entries.get(entry_index).cloned() else {
            return;
        };
        let Some(current) = cache.route_audio_entries.get_mut(slot) else {
            return;
        };
        *current = entry_index;
        ensure_slot(&mut cache.audio_finished_handled, slot, false);
        cache.audio_finished_handled[slot] = true;
        entry
    };
    if let Err(error) = state.engine.play_file_source(&entry, slot, false) {
        super::super::super::diagnostics::report_error(ui, &error);
        return;
    }
    super::super::super::persist::save_process_selection(
        &state.cache,
        &state.app_config,
        &state.save_queue,
    );
    super::super::super::sources::update(ui, state);
}

/// 将混合源行号转换为文件槽位；非文件行返回 None。
fn file_slot_for_row(ui: &MainWindow, row: i32) -> Option<usize> {
    let row = usize::try_from(row).ok()?;
    let kinds = ui.get_source_kinds();
    (kinds.row_data(row) == Some(1)).then(|| {
        (0..row)
            .filter(|index| kinds.row_data(*index) == Some(1))
            .count()
    })
}

/// 返回当前条目所属分类中未被其它文件卡片占用的库下标。
fn category_indices(
    entries: &[AudioEntry],
    selected: &[usize],
    current_slot: usize,
    current: usize,
) -> Vec<usize> {
    let Some(category) = entries.get(current).map(|entry| entry.category.as_str()) else {
        return Vec::new();
    };
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let occupied_elsewhere = selected
                .iter()
                .enumerate()
                .any(|(slot, selected)| slot != current_slot && *selected == index);
            (entry.category == category && !occupied_elsewhere).then_some(index)
        })
        .collect()
}

/// 选择分类队列中的相邻条目；wrap 只用于列表循环和手动前后切换。
fn adjacent_index(
    candidates: &[usize],
    current: usize,
    direction: i32,
    wrap: bool,
) -> Option<usize> {
    let position = candidates.iter().position(|index| *index == current)?;
    if direction < 0 {
        if let Some(previous) = position.checked_sub(1) {
            Some(candidates[previous])
        } else if wrap {
            candidates.last().copied()
        } else {
            None
        }
    } else if position + 1 < candidates.len() {
        Some(candidates[position + 1])
    } else if wrap {
        candidates.first().copied()
    } else {
        None
    }
}

/// 使用标准库时间种子选择分类中的其它条目，不引入随机数依赖。
fn shuffled_index(candidates: &[usize], current: usize, seed: u64) -> Option<usize> {
    if candidates.len() <= 1 {
        return candidates.first().copied();
    }
    let others = candidates
        .iter()
        .copied()
        .filter(|index| *index != current)
        .collect::<Vec<_>>();
    others.get(seed as usize % others.len()).copied()
}

/// 生成只用于非安全随机播放的轻量时间种子。
fn shuffle_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

/// 播放模式按固定顺序循环，非法配置值先归一化。
fn next_mode(mode: u8) -> u8 {
    (normalized_mode(mode) + 1) % MODE_COUNT
}

/// 把旧配置或损坏值回退为顺序播放。
fn normalized_mode(mode: u8) -> u8 {
    if mode < MODE_COUNT {
        mode
    } else {
        MODE_SEQUENTIAL
    }
}

/// 保证并行槽位数组包含目标下标。
fn ensure_slot<T: Clone>(values: &mut Vec<T>, slot: usize, default: T) {
    if values.len() <= slot {
        values.resize(slot + 1, default);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MODE_LIST_LOOP, MODE_SEQUENTIAL, MODE_SHUFFLE, MODE_SINGLE_LOOP, adjacent_index, next_mode,
        shuffled_index,
    };

    /// 四种模式必须在同一按钮上形成稳定闭环。
    #[test]
    fn play_modes_cycle_in_display_order() {
        assert_eq!(next_mode(MODE_SEQUENTIAL), MODE_LIST_LOOP);
        assert_eq!(next_mode(MODE_LIST_LOOP), MODE_SINGLE_LOOP);
        assert_eq!(next_mode(MODE_SINGLE_LOOP), MODE_SHUFFLE);
        assert_eq!(next_mode(MODE_SHUFFLE), MODE_SEQUENTIAL);
    }

    /// 顺序播放末尾停止，列表循环末尾回到第一首。
    #[test]
    fn adjacent_selection_respects_wrap_mode() {
        let candidates = [2, 5, 8];
        assert_eq!(adjacent_index(&candidates, 8, 1, false), None);
        assert_eq!(adjacent_index(&candidates, 8, 1, true), Some(2));
        assert_eq!(adjacent_index(&candidates, 2, -1, true), Some(8));
    }

    /// 多条随机队列不会再次选择当前条目。
    #[test]
    fn shuffle_avoids_current_when_alternatives_exist() {
        let candidates = [1, 3, 7];
        assert_ne!(shuffled_index(&candidates, 3, 1), Some(3));
    }
}
