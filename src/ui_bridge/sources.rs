//! 把真实进程、文件播放和物理麦克风投影为独立路由源行。

use super::cache::UiCache;
use super::library::{PAGE_SIZE, visible_rank};
use super::{AppState, MainWindow};
use crate::engine::AudioEngine;
use slint::{Model, ModelRc, VecModel};

/// 路由源行数据；真实现与 UI 快照共用同一份结构与写入逻辑。
#[derive(Default)]
pub(super) struct SourceRows {
    pub(super) kinds: Vec<i32>,
    pub(super) icons: Vec<slint::SharedString>,
    pub(super) icon_images: Vec<slint::Image>,
    pub(super) titles: Vec<slint::SharedString>,
    pub(super) subtitles: Vec<slint::SharedString>,
    pub(super) details: Vec<slint::SharedString>,
    pub(super) statuses: Vec<slint::SharedString>,
    pub(super) active: Vec<bool>,
    pub(super) volumes: Vec<f32>,
    pub(super) volume_dbs: Vec<slint::SharedString>,
    pub(super) peak_dbs: Vec<slint::SharedString>,
    pub(super) peak_values: Vec<f32>,
    pub(super) muted: Vec<bool>,
    pub(super) paused: Vec<bool>,
    pub(super) output_indices: Vec<i32>,
}

impl SourceRows {
    /// 写入全部行模型；内容未变时保持原模型，避免 Repeater 每 50 ms 重建。
    pub(super) fn apply(&self, ui: &MainWindow) {
        set_if_changed(&ui.get_source_kinds(), self.kinds.clone(), |values| {
            ui.set_source_kinds(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(&ui.get_source_icons(), self.icons.clone(), |values| {
            ui.set_source_icons(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(
            &ui.get_source_icon_images(),
            self.icon_images.clone(),
            |values| {
                ui.set_source_icon_images(ModelRc::new(VecModel::from(values)));
            },
        );
        set_if_changed(&ui.get_source_titles(), self.titles.clone(), |values| {
            ui.set_source_titles(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(
            &ui.get_source_subtitles(),
            self.subtitles.clone(),
            |values| {
                ui.set_source_subtitles(ModelRc::new(VecModel::from(values)));
            },
        );
        set_if_changed(&ui.get_source_details(), self.details.clone(), |values| {
            ui.set_source_details(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(&ui.get_source_statuses(), self.statuses.clone(), |values| {
            ui.set_source_statuses(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(&ui.get_source_active(), self.active.clone(), |values| {
            ui.set_source_active(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(&ui.get_source_volumes(), self.volumes.clone(), |values| {
            ui.set_source_volumes(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(
            &ui.get_source_volume_dbs(),
            self.volume_dbs.clone(),
            |values| {
                ui.set_source_volume_dbs(ModelRc::new(VecModel::from(values)));
            },
        );
        set_if_changed(&ui.get_source_peak_dbs(), self.peak_dbs.clone(), |values| {
            ui.set_source_peak_dbs(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(
            &ui.get_source_peak_values(),
            self.peak_values.clone(),
            |values| {
                ui.set_source_peak_values(ModelRc::new(VecModel::from(values)));
            },
        );
        set_if_changed(&ui.get_source_muted(), self.muted.clone(), |values| {
            ui.set_source_muted(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(&ui.get_source_paused(), self.paused.clone(), |values| {
            ui.set_source_paused(ModelRc::new(VecModel::from(values)));
        });
        set_if_changed(
            &ui.get_source_output_indices(),
            self.output_indices.clone(),
            |values| {
                ui.set_source_output_indices(ModelRc::new(VecModel::from(values)));
            },
        );
    }
}

/// 根据配置、设备枚举和引擎快照刷新路由源行。
pub(super) fn update(ui: &MainWindow, state: &AppState) {
    let mut rows = SourceRows::default();
    let global_output = ui.get_device_index().max(0) as usize;
    let settings = state.app_config.borrow().output.clone();
    let cache = state.cache.borrow();

    for (slot, pid) in cache.selected_pids.iter().copied().enumerate() {
        let Some(process) = cache.processes.iter().find(|process| process.pid == pid) else {
            continue;
        };
        let enabled = !cache.disabled_pids.contains(&pid);
        let volume = cache
            .process_volumes
            .get(slot)
            .copied()
            .unwrap_or_else(|| state.engine.process_volume(slot));
        let peak = state.engine.process_peak(slot);
        rows.kinds.push(0);
        rows.icons.push("\u{E7F4}".into());
        rows.icon_images
            .push(crate::platform::icon::process_icon(pid));
        rows.titles.push(process.name.clone().into());
        rows.subtitles.push("进程音频".into());
        rows.details.push(format!("PID: {pid}").into());
        rows.statuses.push("".into());
        rows.active.push(enabled);
        rows.volumes.push(volume * 100.0);
        rows.peak_dbs.push(peak_db(peak));
        rows.peak_values.push(peak);
        rows.volume_dbs.push(gain_db(volume * 100.0));
        rows.muted.push(volume <= 0.001);
        rows.paused.push(false);
        rows.output_indices.push(output_index(
            &cache.dests,
            cache
                .process_output_ids
                .get(slot)
                .and_then(Option::as_deref),
            cache
                .dests
                .iter()
                .position(|device| {
                    Some(device.render_id.as_str()) == settings.process_output_id.as_deref()
                })
                .unwrap_or(global_output),
        ));
    }
    drop(cache);

    if !state.cache.borrow().file_source_hidden {
        let audio_entries = {
            let cache = state.cache.borrow();
            if cache.route_audio_entries.is_empty() {
                cache
                    .route_audio_entry
                    .and_then(|index| cache.audio_entries.get(index).cloned())
                    .map(|entry| vec![(0, entry, cache.route_file_enabled)])
                    .unwrap_or_default()
            } else {
                cache
                    .route_audio_entries
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, index)| {
                        cache.audio_entries.get(*index).cloned().map(|entry| {
                            (
                                slot,
                                entry,
                                cache
                                    .route_audio_enabled
                                    .get(slot)
                                    .copied()
                                    .unwrap_or(false),
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            }
        };
        for (slot, entry, enabled) in audio_entries {
            let playback = state.engine.file_source_state(slot);
            let playing = playback == crate::engine::FileSourceState::Playing;
            let paused = playback == crate::engine::FileSourceState::Paused;
            let volume = state
                .cache
                .borrow()
                .audio_volumes
                .get(slot)
                .copied()
                .unwrap_or_else(|| state.engine.sfx_volume_for_slot(slot));
            let output_fallback = global_output;
            add_file_row(
                &mut rows,
                &entry.name,
                entry.icon_path.as_deref().unwrap_or(entry.path.as_str()),
                file_source_status(playback),
                (enabled, paused),
                (
                    volume,
                    if playing {
                        state.engine.sfx_peak()
                    } else {
                        0.0
                    },
                ),
                output_index(
                    &state.cache.borrow().dests,
                    state
                        .cache
                        .borrow()
                        .audio_output_ids
                        .get(slot)
                        .and_then(Option::as_deref),
                    output_fallback,
                ),
            );
        }
    }
    let (mic_count, mix_mic, mic_output_index, mic_name) = {
        let cache = state.cache.borrow();
        let mic_index = ui.get_mic_index().max(0) as usize;
        (
            cache.mic_names.len(),
            ui.get_mix_mic(),
            output_index(
                &cache.dests,
                settings.mic_output_id.as_deref(),
                global_output,
            ),
            cache
                .mic_names
                .get(mic_index)
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| "麦克风".to_string()),
        )
    };
    if mic_count > 1 && !state.cache.borrow().mic_source_hidden {
        rows.kinds.push(2);
        rows.icons.push("\u{E720}".into());
        rows.icon_images.push(slint::Image::default());
        rows.titles.push("麦克风".into());
        rows.subtitles.push(mic_name.into());
        rows.details.push("".into());
        rows.statuses.push("".into());
        rows.active.push(mix_mic);
        rows.volumes.push(state.engine.mic_volume() * 100.0);
        rows.volume_dbs
            .push(gain_db(state.engine.mic_volume() * 100.0));
        let peak = state.engine.mic_peak();
        rows.peak_dbs.push(peak_db(peak));
        rows.peak_values.push(peak);
        rows.paused.push(false);
        rows.output_indices.push(mic_output_index);
    }

    rows.apply(ui);
    ui.set_file_output_index(global_output as i32);
}
/// 只更新现有源行的实时电平和启用状态，不重建图标或下拉模型。
pub(super) fn update_dynamic(ui: &MainWindow, state: &AppState) {
    let kinds = ui.get_source_kinds();
    let cache = state.cache.borrow();
    let mut process_slot = 0usize;
    let mut file_slot = 0usize;
    for row in 0..kinds.row_count() {
        let kind = kinds.row_data(row).unwrap_or(-1);
        let (peak, volume, active, paused, status) = match kind {
            0 => {
                let mut slot = process_slot;
                let mut peak = 0.0;
                let mut active = false;
                while process_slot < cache.selected_pids.len() {
                    let candidate_slot = process_slot;
                    let pid = cache.selected_pids[candidate_slot];
                    process_slot += 1;
                    if cache.processes.iter().any(|process| process.pid == pid) {
                        slot = candidate_slot;
                        peak = state.engine.process_peak(slot);
                        active = !cache.disabled_pids.contains(&pid);
                        break;
                    }
                }
                (
                    peak,
                    cache
                        .process_volumes
                        .get(slot)
                        .copied()
                        .unwrap_or_else(|| state.engine.process_volume(slot)),
                    active,
                    false,
                    "",
                )
            }
            1 => {
                let slot = file_slot;
                let active = cache
                    .route_audio_enabled
                    .get(slot)
                    .copied()
                    .unwrap_or(cache.route_file_enabled);
                file_slot += 1;
                let playback = state.engine.file_source_state(slot);
                (
                    if playback == crate::engine::FileSourceState::Playing {
                        state.engine.sfx_peak()
                    } else {
                        0.0
                    },
                    cache
                        .audio_volumes
                        .get(slot)
                        .copied()
                        .unwrap_or_else(|| state.engine.sfx_volume_for_slot(slot)),
                    active,
                    playback == crate::engine::FileSourceState::Paused,
                    file_source_status(playback),
                )
            }
            2 => (
                state.engine.mic_peak(),
                state.engine.mic_volume(),
                ui.get_mix_mic(),
                false,
                "",
            ),
            _ => continue,
        };
        let peaks = ui.get_source_peak_values();
        if peaks.row_data(row) != Some(peak) {
            peaks.set_row_data(row, peak);
        }
        let peak_dbs = ui.get_source_peak_dbs();
        let peak_db_text = peak_db(peak);
        if peak_dbs.row_data(row).as_ref() != Some(&peak_db_text) {
            peak_dbs.set_row_data(row, peak_db_text);
        }
        let volumes = ui.get_source_volumes();
        let volume_percent = volume * 100.0;
        if volumes.row_data(row) != Some(volume_percent) {
            volumes.set_row_data(row, volume_percent);
        }
        let volume_dbs = ui.get_source_volume_dbs();
        let volume_db_text = gain_db(volume_percent);
        if volume_dbs.row_data(row).as_ref() != Some(&volume_db_text) {
            volume_dbs.set_row_data(row, volume_db_text);
        }
        let active_rows = ui.get_source_active();
        if active_rows.row_data(row) != Some(active) {
            active_rows.set_row_data(row, active);
        }
        let paused_rows = ui.get_source_paused();
        if paused_rows.row_data(row) != Some(paused) {
            paused_rows.set_row_data(row, paused);
        }
        let statuses = ui.get_source_statuses();
        let status: slint::SharedString = status.into();
        if statuses.row_data(row).as_ref() != Some(&status) {
            statuses.set_row_data(row, status);
        }
    }
}

/// 只有内容真的变了才重建模型。
///
/// Slint 的 `ModelRc` 相等性是指针比较：每次 `ModelRc::new` 都会让 `for`
/// 的 Repeater 整体销毁重建所有行，下拉框在用户点开时就被换掉，表现为
/// 「点了没反应」。这里逐项比较，内容一致时保持原模型不动。
pub(super) fn set_if_changed<T, F>(current: &ModelRc<T>, next: Vec<T>, apply: F)
where
    T: PartialEq + Clone + 'static,
    F: FnOnce(Vec<T>),
{
    let same = current.row_count() == next.len()
        && (0..next.len()).all(|index| current.row_data(index).as_ref() == Some(&next[index]));
    if !same {
        apply(next);
    }
}

/// 追加一条音频文件源行。
pub(super) fn add_file_row(
    rows: &mut SourceRows,
    name: &str,
    path: &str,
    status: &str,
    playback: (bool, bool),
    levels: (f32, f32),
    output_index: i32,
) {
    let (enabled, is_paused) = playback;
    let (volume, peak) = levels;
    rows.kinds.push(1);
    rows.icons.push("\u{E8B5}".into());
    rows.icon_images
        .push(crate::platform::icon::file_icon(path));
    rows.titles.push(name.into());
    rows.subtitles.push("音频文件".into());
    rows.details.push("".into());
    rows.statuses.push(status.into());
    rows.active.push(enabled);
    rows.volumes.push(volume * 100.0);
    rows.volume_dbs.push(gain_db(volume * 100.0));
    rows.peak_values.push(peak);
    rows.peak_dbs.push(peak_db(peak));
    rows.muted.push(volume <= 0.001);
    rows.paused.push(is_paused);
    rows.output_indices.push(output_index);
}

fn file_source_status(state: crate::engine::FileSourceState) -> &'static str {
    match state {
        crate::engine::FileSourceState::Stopped => "待播放",
        crate::engine::FileSourceState::Playing => "播放中",
        crate::engine::FileSourceState::Paused => "已暂停",
        crate::engine::FileSourceState::Finished => "播放完成",
    }
}

fn gain_db(value: f32) -> slint::SharedString {
    if value <= 0.001 {
        return "-∞ dB".into();
    }
    format!("{:.1} dB", 20.0 * (value / 100.0).log10()).into()
}

fn peak_db(value: f32) -> slint::SharedString {
    if value <= 0.0001 {
        return "-∞ dB".into();
    }
    format!("{:.1} dB", 20.0 * value.log10()).into()
}

/// 输出设备下标：优先用保存的 ID，找不到时退回给定下标。
pub(super) fn output_index(
    devices: &[crate::audio::device::DestDevice],
    saved_id: Option<&str>,
    fallback: usize,
) -> i32 {
    saved_id
        .and_then(|id| devices.iter().position(|device| device.render_id == id))
        .unwrap_or(fallback.min(devices.len().saturating_sub(1))) as i32
}

/// 将当前播放文件映射到音频库当前页的可见行；过滤后不可见时不冒充其他条目。
pub(super) fn file_context_index(
    ui: &MainWindow,
    engine: &AudioEngine,
    cache: &std::rc::Rc<std::cell::RefCell<UiCache>>,
) -> Option<i32> {
    let file = engine.file_state();
    if file.path.is_empty() {
        return (ui.get_audio_entry_index() >= 0).then(|| ui.get_audio_entry_index());
    }
    let (actual_index, current_page) = {
        let cache = cache.borrow();
        (
            cache
                .audio_entries
                .iter()
                .position(|entry| entry.path == file.path)?,
            cache.audio_page,
        )
    };
    let rank = visible_rank(cache, actual_index)?;
    visible_page_row(rank, current_page)
}

fn visible_page_row(rank: usize, page: usize) -> Option<i32> {
    (rank / PAGE_SIZE == page).then_some((rank % PAGE_SIZE) as i32)
}

#[cfg(test)]
mod tests {
    use super::visible_page_row;

    #[test]
    fn visible_file_context_stays_on_its_actual_page() {
        assert_eq!(visible_page_row(0, 0), Some(0));
        assert_eq!(visible_page_row(20, 1), Some(0));
        assert_eq!(visible_page_row(20, 0), None);
        assert_eq!(visible_page_row(39, 1), Some(19));
    }
}
