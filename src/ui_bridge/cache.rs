//! Slint 桥接层仅缓存派生视图状态。

use std::time::Instant;

use crate::audio::device::DestDevice;
use crate::config;
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
    /// 进程筛选关键字。
    pub(super) filter: String,
    /// 输出列表是否包含物理播放设备。
    pub(super) all_devices: bool,
    /// 多输出选择状态，与 `dests` 对齐。
    pub(super) device_selected: Vec<bool>,
    /// 当前已加入混音的进程 PID。
    pub(super) selected_pids: Vec<u32>,
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
    /// 最近一次音频点击，用于区分单击选择与双击播放。
    pub(super) last_audio_click: Option<(usize, Instant)>,
    /// 菜单播放不经过第二次伪造点击。
    pub(super) force_audio_play: bool,
    /// 搜索关键字。
    pub(super) audio_search: String,
    /// 等待设置热键的音频可见下标。
    pub(super) audio_hotkey_index: Option<usize>,
}
