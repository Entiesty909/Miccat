//! Slint 桥接层仅缓存派生视图状态。

use std::time::Instant;

use crate::audio::device::DestDevice;
use crate::config;
use crate::engine::PlaybackTarget;
use crate::process::ProcessInfo;

/// Slint 显示所需的派生状态，不作为配置或音频状态权威。
#[derive(Default)]
pub(super) struct UiCache {
    /// 全部进程（未筛选）。
    pub(super) processes: Vec<ProcessInfo>,
    /// 当前列出的进程 PID，下标与 ComboBox 对齐。
    pub(super) pids: Vec<u32>,
    /// 输出设备列表。
    pub(super) dests: Vec<DestDevice>,
    /// 麦克风设备 ID，下标 0 是「不混入麦克风」。
    pub(super) mic_ids: Vec<String>,
    /// 麦克风显示名，与 `mic_ids` 对齐。
    pub(super) mic_names: Vec<String>,
    /// 输出列表是否包含物理播放设备。
    pub(super) all_devices: bool,
    /// 多输出选择状态，与 `dests` 对齐。
    pub(super) device_selected: Vec<bool>,
    /// 当前已加入混音的进程 PID。
    pub(super) selected_pids: Vec<u32>,
    /// 已加入但暂时关闭的进程 PID；关闭只影响路由，不移除卡片。
    pub(super) disabled_pids: Vec<u32>,
    /// 各进程卡片的独立增益。
    pub(super) process_volumes: Vec<f32>,
    /// 各进程卡片静音前的增益。
    pub(super) process_muted_volumes: Vec<Option<f32>>,
    /// 各进程卡片的独立输出设备 ID。
    pub(super) process_output_ids: Vec<Option<String>>,
    /// 是否暂时从路由页隐藏音频文件源。
    pub(super) file_source_hidden: bool,
    /// 是否暂时从路由页隐藏麦克风源。
    pub(super) mic_source_hidden: bool,
    /// 是否设置系统默认麦。
    pub(super) set_default_mic: bool,
    /// 是否设置默认播放设备。
    pub(super) set_default_render: bool,
    /// 是否设置默认通信播放。
    pub(super) set_default_communications: bool,
    /// 音频库分类视图数据。
    pub(super) audio_categories: Vec<String>,
    /// 音频库条目视图数据。
    pub(super) audio_entries: Vec<config::AudioEntry>,
    /// 当前分类路径。
    pub(super) audio_category: String,
    /// 音频库当前名称筛选词。
    pub(super) audio_search: String,
    /// 音频库当前页，从零开始，分页大小固定为 20。
    pub(super) audio_page: usize,
    /// 音频库名称是否按升序显示。
    pub(super) audio_sort_ascending: bool,
    /// 最近一次音频点击，用于区分单击选择与双击播放。
    pub(super) last_audio_click: Option<(usize, Instant)>,
    /// 菜单播放不经过第二次伪造点击。
    pub(super) force_audio_play: bool,
    /// 待加入路由或当前路由使用的音频库条目实际下标。
    pub(super) route_audio_entry: Option<usize>,
    /// 用户是否启用音频文件路由源；与全局路由启停状态分离。
    pub(super) route_file_enabled: bool,
    /// 已加入路由的音频库条目实际下标。
    pub(super) route_audio_entries: Vec<usize>,
    /// 与 `route_audio_entries` 对齐的用户启用状态。
    pub(super) route_audio_enabled: Vec<bool>,
    /// 各音频卡片的独立增益。
    pub(super) audio_volumes: Vec<f32>,
    /// 各音频卡片静音前的增益。
    pub(super) audio_muted_volumes: Vec<Option<f32>>,
    /// 各音频卡片的独立输出设备 ID。
    pub(super) audio_output_ids: Vec<Option<String>>,
    /// 暂停时可恢复的条目、采样位置和播放目标。
    pub(super) paused_audio: Option<(config::AudioEntry, u64, PlaybackTarget)>,
    /// 当前列表播放目标。
    pub(super) playback_target: PlaybackTarget,
    /// 用户选中的 render 设备 ID，供刷新设备列表时保留选择。
    pub(super) selected_device_ids: Vec<String>,
}
