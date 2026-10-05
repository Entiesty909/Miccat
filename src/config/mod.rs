//! 应用配置：JSON 持久化、旧热键配置迁移、单实例互斥。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod single_instance;
mod store;

pub(crate) use single_instance::acquire_single_instance;
pub(crate) use store::{load, save};

/// 全局开始/停止热键。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Hotkey {
    /// RegisterHotKey 修饰符：1 alt, 2 ctrl, 4 shift。
    pub mods: u32,
    /// 虚拟键码。
    pub vk: u32,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            mods: 0x0002 | 0x0004,
            vk: 0x77,
        }
    }
}

impl Hotkey {
    /// 给 UI 看的标签。
    pub fn label(self) -> String {
        let mut parts = Vec::new();
        if self.mods & 2 != 0 {
            parts.push("Ctrl");
        }
        if self.mods & 4 != 0 {
            parts.push("Shift");
        }
        if self.mods & 1 != 0 {
            parts.push("Alt");
        }
        parts.push(vk_name(self.vk));
        parts.join("+")
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
    /// WASAPI render ID，设备变更后用 name 兜底。
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
}

/// 三路混音增益。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MixConfig {
    /// 进程源增益。
    pub process: f32,
    /// 物理麦增益。
    pub mic: f32,
    /// 音效/文件增益。
    pub sfx: f32,
    /// 最终总增益。
    pub master: f32,
}

impl Default for MixConfig {
    fn default() -> Self {
        Self {
            process: 1.0,
            mic: 1.0,
            sfx: 1.0,
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

fn default_category() -> String {
    "未分类".into()
}
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

fn default_debug_level() -> i32 {
    3
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            master: default_debug_level(),
            categories: BTreeMap::new(),
        }
    }
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
    /// 是否把音频库播放同时送到本机预览设备。
    #[serde(default)]
    pub preview_enabled: bool,
    /// 本机预览音量 0..=1。
    #[serde(default = "default_preview_volume")]
    pub preview_volume: f32,
    /// 音频分类树节点，含仍存在的“全部”和“未分类”根项。
    #[serde(default)]
    pub audio_categories: Vec<String>,
    /// 音频库条目。
    #[serde(default)]
    pub audio_entries: Vec<AudioEntry>,
    /// 调试开关。
    #[serde(default)]
    pub debug: DebugConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 3,
            output: OutputConfig::default(),
            route: RouteConfig::default(),
            mix: MixConfig::default(),
            hotkeys: Hotkeys::default(),
            preview_enabled: false,
            preview_volume: 0.5,
            audio_categories: vec!["全部".into(), "未分类".into()],
            audio_entries: Vec::new(),
            debug: DebugConfig::default(),
        }
    }
}

fn vk_name(vk: u32) -> &'static str {
    match vk {
        0x20 => "Space",
        0x70 => "F1",
        0x71 => "F2",
        0x72 => "F3",
        0x73 => "F4",
        0x74 => "F5",
        0x75 => "F6",
        0x76 => "F7",
        0x77 => "F8",
        0x78 => "F9",
        0x79 => "F10",
        0x7A => "F11",
        0x7B => "F12",
        _ => "Key",
    }
}

/// Slint 按键文本 → VK。
pub fn slint_key_to_vk(text: &str) -> Option<u32> {
    if text == " " || text.eq_ignore_ascii_case("space") {
        return Some(0x20);
    }
    if let Some(number) = text.strip_prefix('F').or_else(|| text.strip_prefix('f'))
        && let Ok(number) = number.parse::<u32>()
        && (1..=12).contains(&number)
    {
        return Some(0x70 + number - 1);
    }
    let mut characters = text.chars();
    let key = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let value = key as u32;
    if (0xF001..=0xF00C).contains(&value) {
        return Some(0x70 + value - 0xF001);
    }
    let value = key.to_ascii_uppercase() as u32;
    if (0x30..=0x39).contains(&value) || (0x41..=0x5A).contains(&value) {
        Some(value)
    } else {
        None
    }
}
