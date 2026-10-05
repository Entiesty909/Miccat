//! 把真实进程、文件播放和物理麦克风投影为独立路由源行。

use super::cache::UiCache;
use super::library::{PAGE_SIZE, visible_rank};
use super::{AppState, MainWindow};
use crate::engine::AudioEngine;
use crate::library::visible_entry_index;
use slint::{Model, ModelRc, VecModel};

/// 路由源行数据；真实现与 UI 快照共用同一份结构与写入逻辑。
#[derive(Default)]
pub(super) struct SourceRows {
    pub(super) kinds: Vec<i32>,
    pub(super) icons: Vec<slint::SharedString>,
    pub(super) titles: Vec<slint::SharedString>,
    pub(super) subtitles: Vec<slint::SharedString>,
    pub(super) details: Vec<slint::SharedString>,
    pub(super) statuses: Vec<slint::SharedString>,
    pub(super) active: Vec<bool>,
    pub(super) volumes: Vec<f32>,
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

    for pid in cache.selected_pids.iter().copied() {
        let Some(process) = cache.processes.iter().find(|process| process.pid == pid) else {
            continue;
        };
        let enabled = !cache.disabled_pids.contains(&pid);
        rows.kinds.push(0);
        rows.icons.push("\u{E7F4}".into());
        rows.titles.push(process.name.clone().into());
        rows.subtitles.push("".into());
        rows.details.push(format!("PID: {pid}").into());
        rows.statuses.push("".into());
        rows.active.push(enabled);
        rows.volumes.push(state.engine.volume() * 100.0);
        rows.muted.push(state.engine.volume() <= 0.001);
        rows.paused.push(false);
        rows.output_indices.push(output_index(
            &cache.dests,
            settings.process_output_id.as_deref(),
            global_output,
        ));
    }
    drop(cache);

    let file = state.engine.file_state();
    if !state.cache.borrow().file_source_hidden && !file.name.is_empty() {
        add_file_row(
            &mut rows,
            &file.name,
            if file.playing {
                "播放中"
            } else if ui.get_player_paused() {
                "已暂停"
            } else {
                "已停止"
            },
            file.playing,
            ui.get_player_paused(),
            state.engine.sfx_volume(),
            output_index(
                &state.cache.borrow().dests,
                settings.file_output_id.as_deref(),
                global_output,
            ),
        );
    } else if !state.cache.borrow().file_source_hidden
        && let Some(entry) = selected_entry(ui, state)
    {
        add_file_row(
            &mut rows,
            &entry.name,
            "待播放",
            false,
            false,
            state.engine.sfx_volume(),
            output_index(
                &state.cache.borrow().dests,
                settings.file_output_id.as_deref(),
                global_output,
            ),
        );
    }

    let (mic_count, mic_name, mic_index, mix_mic, mic_output_index) = {
        let cache = state.cache.borrow();
        let mic_index = ui.get_mic_index().max(0) as usize;
        (
            cache.mic_names.len(),
            cache.mic_names.get(mic_index).cloned().unwrap_or_default(),
            mic_index,
            ui.get_mix_mic(),
            output_index(
                &cache.dests,
                settings.mic_output_id.as_deref(),
                global_output,
            ),
        )
    };
    if mic_count > 1 && !state.cache.borrow().mic_source_hidden {
        rows.kinds.push(2);
        rows.icons.push("\u{E720}".into());
        rows.titles.push("麦克风".into());
        rows.subtitles.push("".into());
        rows.details.push(
            if mic_index == 0 {
                "未选择"
            } else {
                &mic_name
            }
            .into(),
        );
        rows.statuses.push("".into());
        rows.active.push(mix_mic);
        rows.volumes.push(state.engine.mic_volume() * 100.0);
        rows.muted.push(state.engine.mic_volume() <= 0.001);
        rows.paused.push(false);
        rows.output_indices.push(mic_output_index);
    }

    rows.apply(ui);
    ui.set_file_output_index(output_index(
        &state.cache.borrow().dests,
        settings.file_output_id.as_deref(),
        global_output,
    ));
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
    status: &str,
    playing: bool,
    is_paused: bool,
    volume: f32,
    output_index: i32,
) {
    rows.kinds.push(1);
    rows.icons.push("\u{E8B5}".into());
    rows.titles.push(name.into());
    rows.subtitles.push("".into());
    rows.details.push(status.into());
    rows.statuses.push("".into());
    rows.active.push(playing);
    rows.volumes.push(volume * 100.0);
    rows.muted.push(volume <= 0.001);
    rows.paused.push(is_paused);
    rows.output_indices.push(output_index);
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

fn selected_entry(ui: &MainWindow, state: &AppState) -> Option<crate::config::AudioEntry> {
    if ui.get_audio_entry_index() < 0 {
        ui.set_audio_entry_index(0);
    }
    let cache = state.cache.borrow();
    let visible = visible_entry_index(
        &cache.audio_entries,
        &cache.audio_category,
        ui.get_audio_entry_index().max(0) as usize,
    )?;
    cache.audio_entries.get(visible).cloned()
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
