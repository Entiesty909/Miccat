//! Slint 桥接层仅缓存派生视图状态。
//! 音频运行态和持久化配置分别以 `AudioEngine`、`AppConfig` 为准，缓存刷新后可重新构建。

use std::time::Instant;

use crate::audio::device::DestDevice;
use crate::config;
use crate::engine::PlaybackTarget;
use crate::process::ProcessInfo;

/// Slint 显示和交互所需的派生状态，不作为配置或音频运行态权威。
#[derive(Default)]
pub(super) struct UiCache {
    /// 当前枚举到的全部进程快照；筛选和 UI 模型由此派生，路由运行态仍查引擎。
    pub(super) processes: Vec<ProcessInfo>,
    /// 与 UI 进程模型按下标对齐的 PID；刷新时用 PID 而非旧下标恢复选择。
    pub(super) pids: Vec<u32>,
    /// 当前输出设备快照；每个元素的 `render_id` 是跨刷新保持选择的身份。
    pub(super) dests: Vec<DestDevice>,
    /// 与麦克风模型按下标对齐的设备 ID；下标 0 是“不混入麦克风”哨兵。
    pub(super) mic_ids: Vec<String>,
    /// 与 `mic_ids` 一一对应的显示名；首项为空字符串以保持哨兵下标对齐。
    pub(super) mic_names: Vec<String>,
    /// 与 `dests` 一一对应的多选勾选状态；刷新时由稳定设备 ID 重新投影。
    pub(super) device_selected: Vec<bool>,
    /// 当前已加入混音的进程 PID；顺序对应路由卡片及其平行状态数组。
    pub(super) selected_pids: Vec<u32>,
    /// 已加入但暂时关闭的进程 PID；关闭只影响路由，不移除对应卡片槽位。
    pub(super) disabled_pids: Vec<u32>,
    /// 与进程卡片槽位对齐的独立增益；数值单位为线性音量倍数。
    pub(super) process_volumes: Vec<f32>,
    /// 与进程卡片槽位对齐的静音前增益；`Some` 表示可从静音恢复原值。
    pub(super) process_muted_volumes: Vec<Option<f32>>,
    /// 与进程卡片槽位对齐的独立输出设备 ID；`None` 表示使用全局输出选择。
    pub(super) process_output_ids: Vec<Option<String>>,
    /// 是否暂时从路由页隐藏音频文件源；只影响显示，不删除音频槽位。
    pub(super) file_source_hidden: bool,
    /// 是否暂时从路由页隐藏麦克风源；只影响显示，不改变引擎捕获状态。
    pub(super) mic_source_hidden: bool,
    /// 音频库分类视图数据；内容由配置/库扫描结果投影而来。
    pub(super) audio_categories: Vec<String>,
    /// 音频库条目视图数据；路由槽位只保存其实际下标引用。
    pub(super) audio_entries: Vec<config::AudioEntry>,
    /// 当前分类路径；空值代表根分类，筛选列表由此重新计算。
    pub(super) audio_category: String,
    /// 音频库当前名称筛选词；改变后页码必须回到第一页。
    pub(super) audio_search: String,
    /// 音频库当前页，从零开始，分页大小固定为 20。
    pub(super) audio_page: usize,
    /// 音频库名称是否按升序显示；改变排序后页码回到第一页。
    pub(super) audio_sort_ascending: bool,
    /// 最近一次音频点击及其时间，用于区分单击选择与双击播放。
    pub(super) last_audio_click: Option<(usize, Instant)>,
    /// 菜单触发播放时跳过第二次伪造点击，避免重复启动同一条目。
    pub(super) force_audio_play: bool,
    /// 待加入路由或当前单音频路由使用的音频库条目实际下标。
    pub(super) route_audio_entry: Option<usize>,
    /// 用户是否启用音频文件路由源；与全局路由启停状态分离。
    pub(super) route_file_enabled: bool,
    /// 已加入路由的音频库条目实际下标；顺序对应音频卡片槽位。
    pub(super) route_audio_entries: Vec<usize>,
    /// 与 `route_audio_entries` 按槽位对齐的用户启用状态。
    pub(super) route_audio_enabled: Vec<bool>,
    /// 与音频卡片槽位对齐的播放模式：0 顺序、1 列表循环、2 单曲循环、3 随机。
    pub(super) audio_play_modes: Vec<u8>,
    /// 记录完成态是否已经处理，防止 50 ms 轮询重复切歌。
    pub(super) audio_finished_handled: Vec<bool>,
    /// 与音频卡片槽位对齐的独立增益；数值单位为线性音量倍数。
    pub(super) audio_volumes: Vec<f32>,
    /// 与音频卡片槽位对齐的静音前增益；`Some` 表示可从静音恢复原值。
    pub(super) audio_muted_volumes: Vec<Option<f32>>,
    /// 与音频卡片槽位对齐的独立输出设备 ID；`None` 表示使用全局输出选择。
    pub(super) audio_output_ids: Vec<Option<String>>,
    /// 暂停时保存的条目、采样位置（100 纳秒）和播放目标，用于恢复库内试听。
    pub(super) paused_audio: Option<(config::AudioEntry, u64, PlaybackTarget)>,
    /// 当前列表播放目标；由播放控件投影给引擎，不等同于路由卡片状态。
    pub(super) playback_target: PlaybackTarget,
    /// 用户选中的 render 设备 ID；刷新设备列表时按不透明 ID 保留选择。
    pub(super) selected_device_ids: Vec<String>,
}
