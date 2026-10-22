//! 把真实进程、文件播放和物理麦克风投影为独立路由源行。

use super::cache::UiCache;
use super::library::{PAGE_SIZE, visible_rank};
use super::{AppState, MainWindow};
use crate::engine::AudioEngine;
use slint::{Model, ModelRc, VecModel};

/// 路由源行的 Slint 投影缓存。
///
/// Slint 当前以多组并行模型承载一行源卡片，因此所有 `push_*` 方法必须同步写入每组模型。
#[derive(Default)]
pub(super) struct SourceRows {
    /// 每行的来源类型：0 进程、1 文件、2 麦克风；与其他并行模型按同一行号对应。
    kinds: Vec<i32>,
    /// 进程/文件的字体图标字形；麦克风也保留占位值以维持行对齐。
    icons: Vec<slint::SharedString>,
    /// 系统提取的图标图像；没有图像的来源使用空图像占位。
    icon_images: Vec<slint::Image>,
    /// 卡片主标题，来源于进程名或文件名。
    titles: Vec<slint::SharedString>,
    /// 卡片副标题，用于区分来源类别或显示麦克风设备名。
    subtitles: Vec<slint::SharedString>,
    /// 卡片详情文本，例如 PID、分类或时长。
    details: Vec<slint::SharedString>,
    /// 面向用户的来源状态文案；动态刷新时只改内容，不替换模型。
    statuses: Vec<slint::SharedString>,
    /// 是否启用该来源的路由；下标必须与 `kinds` 一致。
    active: Vec<bool>,
    /// UI 百分比形式的线性音量值，范围通常为 0 到 100。
    volumes: Vec<f32>,
    /// 与 `volumes` 对应的分贝显示文本，避免 Slint 重复计算。
    volume_dbs: Vec<slint::SharedString>,
    /// 与 `peak_values` 对应的分贝显示文本。
    peak_dbs: Vec<slint::SharedString>,
    /// 当前来源的峰值线性电平，用于电平表绘制。
    peak_values: Vec<f32>,
    /// 音量接近零时的静音标记；由缓存音量派生而非独立真相。
    muted: Vec<bool>,
    /// 文件播放暂停标记；进程和麦克风行固定为 false。
    paused: Vec<bool>,
    /// 每行保存的输出设备下标，映射到当前设备模型而非持久化 ID。
    output_indices: Vec<i32>,
    /// 文件卡片播放模式；非文件行固定为顺序模式占位。
    play_modes: Vec<i32>,
    /// 文件播放进度 0..=1；非文件行固定为零。
    progresses: Vec<f32>,
    /// 文件当前播放位置文本；非文件行为空。
    position_texts: Vec<slint::SharedString>,
    /// 文件总时长文本；非文件行为空。
    duration_texts: Vec<slint::SharedString>,
    // 这些并行数组共同构成按行索引的 Slint 模型，长度不变量由 `is_aligned` 校验。
}

/// 进程源行的业务数据，避免构造方法依赖长参数列表。
pub(super) struct ProcessRowInput {
    /// 显示名称。
    pub(super) name: String,
    /// 进程 PID。
    pub(super) pid: u32,
    /// 快照场景使用的状态文案。
    pub(super) status: String,
    /// 进程图标。
    pub(super) icon_image: slint::Image,
    /// 是否启用转发。
    pub(super) active: bool,
    /// 线性增益。
    pub(super) volume: f32,
    /// 最近峰值。
    pub(super) peak: f32,
    /// 输出设备下标。
    pub(super) output_index: i32,
}

/// 文件源行的业务数据。
pub(super) struct FileRowInput {
    /// 软件显示名称。
    pub(super) name: String,
    /// 用于提取文件图标的路径。
    pub(super) path: String,
    /// 分类和时长等详情文案。
    pub(super) detail: String,
    /// 文件播放状态文案。
    pub(super) status: String,
    /// 是否启用转发。
    pub(super) active: bool,
    /// 是否暂停。
    pub(super) paused: bool,
    /// 线性增益。
    pub(super) volume: f32,
    /// 最近峰值。
    pub(super) peak: f32,
    /// 输出设备下标。
    pub(super) output_index: i32,
    /// 卡片播放模式编码。
    pub(super) play_mode: u8,
    /// 当前播放位置，单位为 100 纳秒。
    pub(super) position_hns: u64,
    /// 条目总时长，单位为 100 纳秒。
    pub(super) duration_hns: u64,
}

/// 物理麦克风源行的业务数据。
pub(super) struct MicrophoneRowInput {
    /// 物理设备友好名称。
    pub(super) name: String,
    /// 快照场景使用的状态文案。
    pub(super) status: String,
    /// 是否启用混音。
    pub(super) active: bool,
    /// 线性增益。
    pub(super) volume: f32,
    /// 最近峰值。
    pub(super) peak: f32,
    /// 输出设备下标。
    pub(super) output_index: i32,
}

impl SourceRows {
    /// 添加进程源行，并一次性填充该行的所有 Slint 模型。
    pub(super) fn push_process(&mut self, input: ProcessRowInput) {
        self.kinds.push(0);
        self.icons.push("\u{E7F4}".into());
        self.icon_images.push(input.icon_image);
        self.titles.push(input.name.into());
        self.subtitles.push("进程音频".into());
        self.details.push(format!("PID: {}", input.pid).into());
        self.statuses.push(input.status.into());
        self.active.push(input.active);
        self.volumes.push(input.volume * 100.0);
        self.volume_dbs.push(gain_db(input.volume * 100.0));
        self.peak_values.push(input.peak);
        self.peak_dbs.push(peak_db(input.peak));
        self.muted.push(input.volume <= 0.001);
        self.paused.push(false);
        self.output_indices.push(input.output_index);
        self.play_modes.push(0);
        self.progresses.push(0.0);
        self.position_texts.push("".into());
        self.duration_texts.push("".into());
    }

    /// 添加音频文件源行，并统一写入播放状态、音量和输出下标。
    pub(super) fn push_file(&mut self, input: FileRowInput) {
        self.kinds.push(1);
        self.icons.push("\u{E8B5}".into());
        self.icon_images
            .push(crate::platform::icon::file_icon(&input.path));
        self.titles.push(input.name.into());
        self.subtitles.push(input.detail.into());
        self.details.push("".into());
        self.statuses.push(input.status.into());
        self.active.push(input.active);
        self.volumes.push(input.volume * 100.0);
        self.volume_dbs.push(gain_db(input.volume * 100.0));
        self.peak_values.push(input.peak);
        self.peak_dbs.push(peak_db(input.peak));
        self.muted.push(input.volume <= 0.001);
        self.paused.push(input.paused);
        self.output_indices.push(input.output_index);
        self.play_modes.push(i32::from(input.play_mode));
        self.progresses
            .push(progress(input.position_hns, input.duration_hns));
        self.position_texts
            .push(super::format::format_duration_hns(input.position_hns).into());
        self.duration_texts
            .push(super::format::format_duration_hns(input.duration_hns).into());
    }

    /// 添加物理麦克风源行；麦克风没有文件播放状态，但仍保持完整行模型。
    pub(super) fn push_microphone(&mut self, input: MicrophoneRowInput) {
        self.kinds.push(2);
        self.icons.push("\u{E720}".into());
        self.icon_images.push(slint::Image::default());
        self.titles.push("麦克风".into());
        self.subtitles
            .push(format!("{} · 麦克风", input.name).into());
        self.details.push("".into());
        self.statuses.push(input.status.into());
        self.active.push(input.active);
        self.volumes.push(input.volume * 100.0);
        self.volume_dbs.push(gain_db(input.volume * 100.0));
        self.peak_values.push(input.peak);
        self.peak_dbs.push(peak_db(input.peak));
        self.muted.push(input.volume <= 0.001);
        self.paused.push(false);
        self.output_indices.push(input.output_index);
        self.play_modes.push(0);
        self.progresses.push(0.0);
        self.position_texts.push("".into());
        self.duration_texts.push("".into());
    }

    /// 每个数组都代表同一张卡片的一个属性列，因此只要有一列漏写，Slint 就会把相邻来源错配。
    /// 检查所有并行模型是否仍描述相同数量的源行。
    fn is_aligned(&self) -> bool {
        let count = self.kinds.len();
        [
            self.icons.len(),
            self.icon_images.len(),
            self.titles.len(),
            self.subtitles.len(),
            self.details.len(),
            self.statuses.len(),
            self.active.len(),
            self.volumes.len(),
            self.volume_dbs.len(),
            self.peak_dbs.len(),
            self.peak_values.len(),
            self.muted.len(),
            self.paused.len(),
            self.output_indices.len(),
            self.play_modes.len(),
            self.progresses.len(),
            self.position_texts.len(),
            self.duration_texts.len(),
        ]
        .into_iter()
        .all(|length| length == count)
    }

    /// `SourceRows` 只在本地组装完整快照；真正替换模型由 `set_if_changed` 决定，以保留控件焦点和下拉选择。
    /// 写入全部行模型；内容未变时保持原模型，避免 Repeater 每 50 ms 重建。
    pub(super) fn apply(&self, ui: &MainWindow) {
        // Slint 的并行属性没有类型层面的行结构，写入前必须主动检查长度不变量。
        debug_assert!(
            self.is_aligned(),
            "source row models have different lengths"
        );
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
        set_if_changed(
            &ui.get_source_play_modes(),
            self.play_modes.clone(),
            |values| {
                ui.set_source_play_modes(ModelRc::new(VecModel::from(values)));
            },
        );
        set_if_changed(
            &ui.get_source_progresses(),
            self.progresses.clone(),
            |values| {
                ui.set_source_progresses(ModelRc::new(VecModel::from(values)));
            },
        );
        set_if_changed(
            &ui.get_source_position_texts(),
            self.position_texts.clone(),
            |values| {
                ui.set_source_position_texts(ModelRc::new(VecModel::from(values)));
            },
        );
        set_if_changed(
            &ui.get_source_duration_texts(),
            self.duration_texts.clone(),
            |values| {
                ui.set_source_duration_texts(ModelRc::new(VecModel::from(values)));
            },
        );
    }
}

/// 根据配置、设备枚举和引擎快照刷新路由源行。
pub(super) fn update(ui: &MainWindow, state: &AppState) {
    // 结构性刷新按“进程 → 文件 → 麦克风”的顺序建立行；后续动态刷新依赖这个顺序维护引擎槽位映射。
    let mut rows = SourceRows::default();
    let global_output = ui.get_device_index().max(0) as usize;
    let settings = state.app_config.borrow().output.clone();
    let cache = state.cache.borrow();

    // `selected_pids` 的位置就是引擎进程槽位，不能按显示行号重新编号，否则音量和峰值会写到别的进程。
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
        let output_index = output_index(
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
        );
        rows.push_process(ProcessRowInput {
            name: process.name.clone(),
            pid,
            status: if enabled { "运行中" } else { "已停用" }.into(),
            icon_image: crate::platform::icon::process_icon(pid),
            active: enabled,
            volume,
            peak,
            output_index,
        });
    }
    drop(cache);

    // 文件槽位同样来自缓存中的音频索引数组；隐藏文件源时不创建行，避免 UI 暴露不可操作的槽位。
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
        // 每个文件行绑定自己的播放槽位，状态、进度和音量都从该槽读取。
        for (slot, entry, enabled) in audio_entries {
            let playback = state.engine.file_source_state(slot);
            let playing = playback == crate::engine::FileSourceState::Playing;
            let paused = playback == crate::engine::FileSourceState::Paused;
            let position_hns = state.engine.file_source_position_hns(slot);
            let extension = std::path::Path::new(&entry.path)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_uppercase)
                .unwrap_or_else(|| "音频".into());
            let category = crate::library::category_leaf(&entry.category);
            let detail = format!("本地音频 · {extension} · {category}");
            let play_mode = state
                .cache
                .borrow()
                .audio_play_modes
                .get(slot)
                .copied()
                .unwrap_or(0);
            let volume = state
                .cache
                .borrow()
                .audio_volumes
                .get(slot)
                .copied()
                .unwrap_or_else(|| state.engine.sfx_volume_for_slot(slot));
            let output_fallback = global_output;
            rows.push_file(FileRowInput {
                name: entry.name.clone(),
                path: entry
                    .icon_path
                    .as_deref()
                    .unwrap_or(entry.path.as_str())
                    .into(),
                detail,
                status: file_source_status(playback).into(),
                active: enabled,
                paused,
                volume,
                peak: if playing {
                    state.engine.sfx_peak()
                } else {
                    0.0
                },
                output_index: output_index(
                    &state.cache.borrow().dests,
                    state
                        .cache
                        .borrow()
                        .audio_output_ids
                        .get(slot)
                        .and_then(Option::as_deref),
                    output_fallback,
                ),
                play_mode,
                position_hns,
                duration_hns: entry.duration_hns,
            });
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
    // 麦克风只有在存在可选设备且未被隐藏时才占用一行；其槽位与进程/文件槽位分开管理。
    if mic_count > 1 && !state.cache.borrow().mic_source_hidden {
        let peak = state.engine.mic_peak();
        rows.push_microphone(MicrophoneRowInput {
            name: mic_name,
            status: if mix_mic { "已连接" } else { "未启用" }.into(),
            active: mix_mic,
            volume: state.engine.mic_volume(),
            peak,
            output_index: mic_output_index,
        });
    }

    // 结构性模型一次性提交，防止中间状态被 Slint 渲染；实时值交给 `update_dynamic` 原地更新。
    rows.apply(ui);
    ui.set_file_output_index(global_output as i32);
}
/// 只更新现有源行的实时电平和启用状态，不重建图标或下拉模型。
pub(super) fn update_dynamic(ui: &MainWindow, state: &AppState) {
    let kinds = ui.get_source_kinds();
    let cache = state.cache.borrow();
    // 并行模型的行号只表示当前可见行；两个 `*_slot` 计数器分别跳过缺失进程并映射文件槽位。
    let mut process_slot = 0usize;
    let mut file_slot = 0usize;
    for row in 0..kinds.row_count() {
        let kind = kinds.row_data(row).unwrap_or(-1);
        let (peak, volume, active, paused, status, progress_value, position, duration) = match kind
        {
            0 => {
                let mut slot = process_slot;
                let mut peak = 0.0;
                let mut active = false;
                // 缓存可能保留已退出进程，必须消耗其原槽位但只为仍存在的进程更新一行。
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
                    if active { "运行中" } else { "已停用" },
                    0.0,
                    String::new(),
                    String::new(),
                )
            }
            1 => {
                // 文件行按“当前已输出的文件行”计数，与结构刷新时的文件槽位顺序保持一致。
                let slot = file_slot;
                let active = cache
                    .route_audio_enabled
                    .get(slot)
                    .copied()
                    .unwrap_or(cache.route_file_enabled);
                file_slot += 1;
                let playback = state.engine.file_source_state(slot);
                let position_hns = state.engine.file_source_position_hns(slot);
                let duration_hns = cache
                    .route_audio_entries
                    .get(slot)
                    .and_then(|index| cache.audio_entries.get(*index))
                    .map(|entry| entry.duration_hns)
                    .unwrap_or(0);
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
                    progress(position_hns, duration_hns),
                    super::format::format_duration_hns(position_hns),
                    super::format::format_duration_hns(duration_hns),
                )
            }
            2 => (
                state.engine.mic_peak(),
                state.engine.mic_volume(),
                ui.get_mix_mic(),
                false,
                if ui.get_mix_mic() {
                    "已连接"
                } else {
                    "未启用"
                },
                0.0,
                String::new(),
                String::new(),
            ),
            _ => continue,
        };
        // 电平、音量和状态是高频变化数据，只改已有 VecModel 的单行，避免重建图标与设备下拉框。
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
        let progresses = ui.get_source_progresses();
        if progresses.row_data(row) != Some(progress_value) {
            progresses.set_row_data(row, progress_value);
        }
        let positions = ui.get_source_position_texts();
        let position: slint::SharedString = position.into();
        if positions.row_data(row).as_ref() != Some(&position) {
            positions.set_row_data(row, position);
        }
        let durations = ui.get_source_duration_texts();
        let duration: slint::SharedString = duration.into();
        if durations.row_data(row).as_ref() != Some(&duration) {
            durations.set_row_data(row, duration);
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
    // ModelRc 的指针身份会影响 Repeater 生命周期；只有内容变化才替换，避免打断用户正在操作的控件。
    let same = current.row_count() == next.len()
        && (0..next.len()).all(|index| current.row_data(index).as_ref() == Some(&next[index]));
    if !same {
        apply(next);
    }
}
/// 将 100 纳秒位置转换为进度比例；未知时长保持零。
fn progress(position_hns: u64, duration_hns: u64) -> f32 {
    if duration_hns == 0 {
        0.0
    } else {
        (position_hns as f64 / duration_hns as f64).clamp(0.0, 1.0) as f32
    }
}

/// 将引擎文件状态转换为路由卡片文案；文案是显示派生值，不参与播放状态判断。
fn file_source_status(state: crate::engine::FileSourceState) -> &'static str {
    match state {
        crate::engine::FileSourceState::Stopped => "待播放",
        crate::engine::FileSourceState::Playing => "播放中",
        crate::engine::FileSourceState::Paused => "已暂停",
        crate::engine::FileSourceState::Finished => "播放完成",
    }
}

/// 把百分比音量转换为分贝文本；零附近使用负无穷表示，避免对数计算得到误导性的有限值。
fn gain_db(value: f32) -> slint::SharedString {
    if value <= 0.001 {
        return "-∞ dB".into();
    }
    format!("{:.1} dB", 20.0 * (value / 100.0).log10()).into()
}

/// 把 0 到 1 的峰值振幅转换为分贝文本；阈值以下视为静音底噪。
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

/// 将全局可见排名换算为当前页内行号；跨页时返回 None，防止播放条目冒充当前页其他条目。
fn visible_page_row(rank: usize, page: usize) -> Option<i32> {
    (rank / PAGE_SIZE == page).then_some((rank % PAGE_SIZE) as i32)
}

#[cfg(test)]
mod tests {
    use super::{MicrophoneRowInput, ProcessRowInput, SourceRows, visible_page_row};

    /// 三类行都必须同步扩展并行数组，否则 UI 属性会错位。
    #[test]
    fn source_row_builders_keep_parallel_models_aligned() {
        let mut rows = SourceRows::default();
        rows.push_process(ProcessRowInput {
            name: "进程".into(),
            pid: 1,
            status: "".into(),
            icon_image: slint::Image::default(),
            active: true,
            volume: 1.0,
            peak: 0.0,
            output_index: 0,
        });
        rows.push_microphone(MicrophoneRowInput {
            name: "麦克风设备".into(),
            status: "".into(),
            active: false,
            volume: 1.0,
            peak: 0.0,
            output_index: 0,
        });
        assert!(rows.is_aligned());
    }

    /// 页码换算保持条目在实际页，不因过滤或分页把上下文指向另一行。
    #[test]
    fn visible_file_context_stays_on_its_actual_page() {
        assert_eq!(visible_page_row(0, 0), Some(0));
        assert_eq!(visible_page_row(20, 1), Some(0));
        assert_eq!(visible_page_row(20, 0), None);
        assert_eq!(visible_page_row(39, 1), Some(19));
    }
}
