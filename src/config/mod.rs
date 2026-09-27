//! 应用配置：JSON 持久化、旧热键配置迁移、单实例互斥。

use serde::{Deserialize, Serialize};

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
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RouteConfig {
    /// 上次选中的可执行文件名，用名字找新 PID。
    pub process_name: Option<String>,
    /// 多进程输入名称；为空时使用 process_name。
    #[serde(default)]
    pub process_names: Vec<String>,
    /// 开始转发时是否混入设置页选择的物理麦。
    #[serde(default)]
    pub mix_mic: bool,
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
    /// 是否循环。
    #[serde(default)]
    pub loop_playback: bool,
}

fn default_category() -> String {
    "未分类".into()
}
fn default_preview_volume() -> f32 {
    0.5
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
    /// 自定义音频分类树节点。
    #[serde(default)]
    pub audio_categories: Vec<String>,
    /// 音频库条目。
    #[serde(default)]
    pub audio_entries: Vec<AudioEntry>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 2,
            output: OutputConfig::default(),
            route: RouteConfig::default(),
            mix: MixConfig::default(),
            hotkeys: Hotkeys::default(),
            preview_enabled: false,
            preview_volume: 0.5,
            audio_categories: Vec::new(),
            audio_entries: Vec::new(),
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
    if let Some(n) = text.strip_prefix('F').or_else(|| text.strip_prefix('f'))
        && let Ok(i) = n.parse::<u32>()
        && (1..=12).contains(&i)
    {
        return Some(0x70 + i - 1);
    }
    let mut ch = text.chars();
    let c = ch.next()?;
    if ch.next().is_some() {
        let v = c as u32;
        if (0xF001..=0xF00C).contains(&v) {
            return Some(0x70 + (v - 0xF001));
        }
        return None;
    }
    let u = c.to_ascii_uppercase() as u32;
    if (0x30..=0x39).contains(&u) || (0x41..=0x5A).contains(&u) {
        Some(u)
    } else {
        None
    }
}
