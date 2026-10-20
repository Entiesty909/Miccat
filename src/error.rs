//! 统一错误。UI 只展示 `user_message()`，细节走 Display / tracing。

use std::fmt;
use std::io;

/// 应用层错误；音频热路径只返回错误，不通过 unwrap/expect 终止线程。
#[derive(Debug)]
pub enum Error {
    /// Windows COM/WASAPI 调用返回的原始 HRESULT 与系统消息。
    Com(windows::core::Error),
    /// 目标进程的 PID；进程退出或会话不再可访问时携带该值。
    ProcessNotFound(u32),
    /// 输出设备不可用；具体设备标识可能已失效或未能解析。
    DeviceNotFound,
    /// 进程环回捕获初始化阶段的静态失败原因。
    CaptureInit(&'static str),
    /// WASAPI 渲染端初始化阶段的静态失败原因。
    RenderInit(&'static str),
    /// 同一驱动的多个端点不能同时打开（如 VB-CABLE 的扬声器与 In 16ch）。
    /// 字符串字段保留可展示的冲突详情。
    DeviceBusy(String),
    /// 混音格式解析或转换失败的静态原因，避免在实时路径分配错误文本。
    Format(&'static str),
    /// WAV 等文件操作的原始 IO 错误，保留 source 链供诊断。
    Io(io::Error),
    /// 激活设备或等待系统状态变化超时的静态原因。
    TimedOut(&'static str),
    /// CLI/UI 参数校验失败的可展示说明。
    InvalidArgs(String),
    /// VB-CABLE 下载、解压、提权安装或卸载失败的可展示说明。
    Cable(String),
}

/// 在用户短句、稳定代码和技术详情之间提供统一的错误映射。
impl Error {
    /// UI 只调用这里，刻意隐藏 HRESULT 和内部路径，避免泄露难懂细节。
    pub fn user_message(&self) -> String {
        match self {
            Self::DeviceNotFound | Self::RenderInit(_) => {
                "无法连接到音频设备\n设备可能已经被拔出。".into()
            }
            Self::ProcessNotFound(_) => "进程已退出".into(),
            Self::CaptureInit(_) => "无法捕获该进程的音频".into(),
            Self::DeviceBusy(detail) => detail.clone(),
            Self::Format(_) => "采样率或音频格式不兼容".into(),
            Self::TimedOut(_) => "音频设备初始化超时".into(),
            Self::Com(_) => "无法初始化音频系统".into(),
            Self::Io(_) => "写入文件失败".into(),
            Self::InvalidArgs(msg) => msg.clone(),
            Self::Cable(msg) => msg.clone(),
        }
    }

    /// 将错误分类为可复制、可聚合的稳定诊断码，不随具体文本变化。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Com(_) => "E-COM",
            Self::ProcessNotFound(_) => "E-PROC",
            Self::DeviceNotFound => "E-DEV",
            Self::CaptureInit(_) => "E-CAPTURE",
            Self::RenderInit(_) => "E-RENDER",
            Self::DeviceBusy(_) => "E-BUSY",
            Self::Format(_) => "E-FORMAT",
            Self::Io(_) => "E-IO",
            Self::TimedOut(_) => "E-TIMEOUT",
            Self::InvalidArgs(_) => "E-ARGS",
            Self::Cable(_) => "E-CABLE",
        }
    }

    /// 根据错误类别提供下一步操作建议；空字符串表示参数错误无需额外动作。
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Com(_) => "确认系统音频服务已启动；若刚插拔设备，请在设置页点「刷新设备」。",
            Self::ProcessNotFound(_) => "在路由页重新选择仍在运行的进程。",
            Self::DeviceNotFound | Self::RenderInit(_) => {
                "在设置页重新选择输出设备；必要时重新插拔耳机/线缆。"
            }
            Self::DeviceBusy(_) => {
                "把「输出设备」改到另一只设备，或在设置页勾选「显示全部播放设备」后换一个。"
            }
            Self::CaptureInit(_) => "确认该进程正在播放声音，并以相同权限运行。",
            Self::Format(_) => "在设置页切换输出设备，或换一个采样率兼容的设备。",
            Self::Io(_) => "检查磁盘空间与文件是否被占用。",
            Self::TimedOut(_) => "稍后重试；若持续失败，请在问题中心查看日志。",
            Self::InvalidArgs(_) => "",
            Self::Cable(_) => "以管理员身份重试安装，或手动安装 VB-CABLE 后重启。",
        }
    }

    /// 返回完整 Display 文本，保留 HRESULT/系统错误供问题中心排查。
    pub fn detail(&self) -> String {
        self.to_string()
    }
}

/// 把内部错误转换为稳定的技术字符串，便于日志和问题中心复用。
impl fmt::Display for Error {
    /// 按错误来源保留上下文；用户短句不应替代这层诊断信息。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Com(e) => write!(f, "COM/WASAPI: {e}"),
            Self::ProcessNotFound(pid) => write!(f, "process {pid} not found"),
            Self::DeviceNotFound => write!(f, "audio device not found"),
            Self::CaptureInit(msg) => write!(f, "capture init: {msg}"),
            Self::RenderInit(msg) => write!(f, "render init: {msg}"),
            Self::DeviceBusy(msg) => write!(f, "device busy: {msg}"),
            Self::Format(msg) => write!(f, "format: {msg}"),
            Self::Io(e) => write!(f, "io: {e}"),
            Self::TimedOut(msg) => write!(f, "timeout: {msg}"),
            Self::InvalidArgs(msg) => write!(f, "{msg}"),
            Self::Cable(msg) => write!(f, "{msg}"),
        }
    }
}

/// 暴露可递归追踪的底层错误，仅 COM 和 IO 拥有 source。
impl std::error::Error for Error {
    /// 返回 Windows 或文件系统原始错误，其他变体没有可追踪子错误。
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Com(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// 将 windows-rs 错误归入统一 COM/WASAPI 变体，保留原始 HRESULT。
impl From<windows::core::Error> for Error {
    /// 保留调用方提供的 HRESULT 和系统消息，不在转换时丢失诊断信息。
    fn from(value: windows::core::Error) -> Self {
        Self::Com(value)
    }
}

/// 将标准 IO 错误归入统一文件错误，供用户短句和 source 链使用。
impl From<io::Error> for Error {
    /// 转移 IO 错误所有权，避免格式化前复制底层错误。
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
