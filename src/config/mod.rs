//! 应用配置：JSON 持久化、旧热键配置迁移、单实例互斥。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// 配置子模块分别负责单实例句柄和 JSON 文件的生命周期，根模块只暴露稳定入口。
mod single_instance;
mod store;

// 仅向上层导出读写配置和获取单实例所需的最小接口。
pub(crate) use single_instance::acquire_single_instance;
pub(crate) use store::{load, save};

/// 全局开始/停止热键。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Hotkey {
    /// RegisterHotKey 修饰符位掩码：1=Alt、2=Ctrl、4=Shift，可按位组合。
    pub mods: u32,
    /// Windows 虚拟键码；与 `mods` 一起构成全局热键注册值。
    pub vk: u32,
}

/// 提供兼容旧配置的全局热键默认值。
impl Default for Hotkey {
    /// 返回 Ctrl+Shift+F8，作为首次启动的安全默认切换键。
    fn default() -> Self {
        // Ctrl+Shift+F8 避开常见媒体键，首次启动即可安全切换路由。
        Self {
            mods: 0x0002 | 0x0004,
            vk: 0x77,
        }
    }
}

/// 全局热键集合。
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Hotkeys {
    /// 开始/停止转发。
    pub toggle_route: Hotkey,
    /// 播放上一首；未绑定时不注册。
    #[serde(default)]
    pub previous_track: Option<Hotkey>,
    /// 播放下一首。
    #[serde(default)]
    pub next_track: Option<Hotkey>,
    /// 停止当前播放。
    #[serde(default)]
    pub stop_playback: Option<Hotkey>,
}

/// 输出设备与目标麦配置。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    /// WASAPI render endpoint 的不透明 ID；设备变更后由 `device_name` 兜底匹配。
    pub device_id: Option<String>,
    /// 上次显示名。
    pub device_name: Option<String>,
    /// 配对的虚拟麦克风 ID。
    pub mic_id: Option<String>,
    /// 配对的虚拟麦克风显示名。
    pub mic_name: Option<String>,
    /// 是否把虚拟麦设为系统默认录音设备。
    pub set_default_mic: bool,
    /// 是否设置为系统默认播放设备。
    #[serde(default)]
    pub set_default_render: bool,
    /// 是否设置为默认通信播放设备。
    #[serde(default)]
    pub set_default_communications: bool,
    /// 是否显示物理播放设备。
    pub all_devices: bool,
    /// 多选的 render 设备 ID；为空时退回 `device_id`。
    #[serde(default)]
    pub device_ids: Vec<String>,
    /// 多个进程卡片各自使用的 render endpoint ID；旧配置回退到 process_output_id。
    #[serde(default)]
    pub process_output_ids: Vec<String>,
    /// 多个音频卡片各自使用的 render endpoint ID；旧配置回退到 file_output_id。
    #[serde(default)]
    pub file_output_ids: Vec<String>,
    /// 进程来源使用的 render endpoint ID。
    #[serde(default)]
    pub process_output_id: Option<String>,
    /// 音频文件来源使用的 render endpoint ID。
    #[serde(default)]
    pub file_output_id: Option<String>,
    /// 麦克风来源使用的 render endpoint ID。
    #[serde(default)]
    pub mic_output_id: Option<String>,
}

/// 路由来源、麦克风混入和启动恢复状态的持久化配置。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RouteConfig {
    /// 上次选中的可执行文件名，用名字找新 PID。
    pub process_name: Option<String>,
    /// 多进程输入名称；为空时使用 process_name。
    #[serde(default)]
    pub process_names: Vec<String>,
    /// 开始转发时是否混入选择的物理麦克风。
    #[serde(default)]
    pub mix_mic: bool,
    /// 选择混入的物理麦克风 ID。
    #[serde(default)]
    pub mic_id: Option<String>,
    /// 已关闭的进程卡片槽位；按启动时恢复的进程顺序定位。
    #[serde(default)]
    pub disabled_process_slots: Vec<usize>,
    /// 已加入路由的音频文件路径；启动时映射回音频库条目。
    #[serde(default)]
    pub audio_source_paths: Vec<String>,
    /// 与 `audio_source_paths` 对齐的音频卡片启用状态。
    #[serde(default)]
    pub audio_source_enabled: Vec<bool>,
}

/// 三路混音增益。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MixConfig {
    /// 进程源默认增益。
    pub process: f32,
    /// 各进程卡片的独立增益；缺省时使用 process。
    #[serde(default)]
    pub process_volumes: Vec<f32>,
    /// 物理麦增益。
    pub mic: f32,
    /// 音效/文件默认增益。
    pub sfx: f32,
    /// 各音频卡片的独立增益；缺省时使用 sfx。
    #[serde(default)]
    pub sfx_volumes: Vec<f32>,
    /// 最终总增益。
    pub master: f32,
}

/// 保证新配置拥有各混音通道的中性增益和空的独立音量表。
impl Default for MixConfig {
    /// 以单位增益启动各来源，避免首次播放被静音。
    fn default() -> Self {
        Self {
            process: 1.0,
            process_volumes: Vec::new(),
            mic: 1.0,
            sfx: 1.0,
            sfx_volumes: Vec::new(),
            master: 1.0,
        }
    }
}

/// 音频库中的一项。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioEntry {
    /// 显示名。
    pub name: String,
    /// 文件路径。
    pub path: String,
    /// 可选的自定义 Windows 图标文件路径；为空时使用音符图标。
    #[serde(default)]
    pub icon_path: Option<String>,
    /// 字体图标 glyph；旧配置缺失时使用简约音乐音符。
    #[serde(default = "default_audio_icon_glyph")]
    pub icon_glyph: String,
    /// 所属分类路径；使用 `/` 分隔父分类和子分类。
    #[serde(default = "default_category")]
    pub category: String,
    /// 文件时长，100 ns 为单位；旧配置为 0，启动时补全。
    #[serde(default)]
    pub duration_hns: u64,
    /// 导入或迁移时读取的文件大小；用于音频库详情。
    #[serde(default)]
    pub file_size_bytes: u64,
    /// 文件音频流的采样率，单位为 Hz。
    #[serde(default)]
    pub sample_rate: u32,
    /// 文件音频流的声道数。
    #[serde(default)]
    pub channels: u16,
    /// 是否循环。
    #[serde(default)]
    pub loop_playback: bool,
    /// 可选的应用内播放快捷键；未设置时不绑定。
    #[serde(default)]
    pub hotkey: Option<String>,
    /// 全局播放热键（组合键，应用不在前台也生效）；旧字段 `hotkey` 为前台单键。
    #[serde(default)]
    pub hotkey_binding: Option<Hotkey>,
}

/// 为缺失图标字段提供稳定的字体图标默认值。
fn default_audio_icon_glyph() -> String {
    "\u{EC4F}".into()
}

/// 为缺失分类字段提供统一的未分类根路径。
fn default_category() -> String {
    "未分类".into()
}
/// 为首次启用本机预览提供 0..=1 范围内的中等音量。
fn default_preview_volume() -> f32 {
    0.5
}

/// 调试开关持久化配置。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugConfig {
    /// 默认日志级别编码，见 `DebugLevel::code`。
    #[serde(default = "default_debug_level")]
    pub master: i32,
    /// 分类键到级别编码的覆盖；缺省表示跟随默认。
    #[serde(default)]
    pub categories: BTreeMap<String, i32>,
}

/// 为缺失日志级别字段提供默认编码。
fn default_debug_level() -> i32 {
    3
}

/// 以默认日志级别和空覆盖表初始化调试配置。
impl Default for DebugConfig {
    /// 使用默认级别并清空分类覆盖，确保日志可见且可预测。
    fn default() -> Self {
        Self {
            master: default_debug_level(),
            categories: BTreeMap::new(),
        }
    }
}

/// 保存各独立窗口下次启动时可恢复的位置。
/// 保存三个独立顶层窗口关闭时的物理屏幕坐标。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WindowConfig {
    /// 主窗口上次关闭位置；缺省时首次居中。
    #[serde(default)]
    pub main: Option<WindowPosition>,
    /// 添加源窗口上次关闭位置；首次打开时居中。
    #[serde(default)]
    pub add_source: Option<WindowPosition>,
    /// 音频编辑窗口上次关闭位置；首次打开时居中。
    #[serde(default)]
    pub audio_edit: Option<WindowPosition>,
}

/// 以物理像素记录窗口左上角，避免 DPI 缩放造成歧义。
/// 窗口左上角的物理屏幕坐标。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPosition {
    /// 屏幕横坐标。
    pub x: i32,
    /// 屏幕纵坐标。
    pub y: i32,
}

/// 配置文件根对象。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    /// 配置格式版本。
    pub version: u32,
    /// 输出配置。
    pub output: OutputConfig,
    /// 路由配置。
    pub route: RouteConfig,
    /// 混音配置。
    pub mix: MixConfig,
    /// 热键集合。
    pub hotkeys: Hotkeys,
    /// 当前界面主题索引：0 浅色、1 深色、2 淡粉。
    #[serde(default)]
    pub theme_index: i32,
    /// 是否把音频库播放同时送到本机预览设备。
    #[serde(default)]
    pub preview_enabled: bool,
    /// 本机预览音量 0..=1。
    #[serde(default = "default_preview_volume")]
    pub preview_volume: f32,
    /// 音频分类树节点，含仍存在的“全部”和“未分类”根项。
    #[serde(default)]
    pub audio_categories: Vec<String>,
    /// 分类路径到 Windows MDL2 glyph 的持久化映射。
    #[serde(default)]
    pub audio_category_icons: BTreeMap<String, String>,
    /// 音频库条目。
    #[serde(default)]
    pub audio_entries: Vec<AudioEntry>,
    /// 主窗口与独立子窗口位置。
    #[serde(default)]
    pub windows: WindowConfig,
    /// 调试开关。
    #[serde(default)]
    pub debug: DebugConfig,
}

/// 建立当前配置版本的完整默认树；新字段通过各自默认值保持旧 JSON 可读。
impl Default for AppConfig {
    /// 组合所有子配置的默认值，并写入当前 JSON 版本号。
    fn default() -> Self {
        Self {
            version: 3,
            // 版本号与根分类一起建立当前 schema 的最低不变量；迁移逻辑保持幂等。
            output: OutputConfig::default(),
            route: RouteConfig::default(),
            mix: MixConfig::default(),
            hotkeys: Hotkeys::default(),
            theme_index: 0,
            preview_enabled: false,
            preview_volume: 0.5,
            // 两个根项供分类树和音频库筛选使用，新配置始终从它们开始。
            audio_categories: vec!["全部".into(), "未分类".into()],
            audio_category_icons: BTreeMap::new(),
            audio_entries: Vec::new(),
            windows: WindowConfig::default(),
            debug: DebugConfig::default(),
        }
    }
}
