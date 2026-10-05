//! 统一错误。UI 只展示 `user_message()`，细节走 Display / tracing。

use std::fmt;
use std::io;

/// 应用层错误。音频热路径禁止 unwrap。
#[derive(Debug)]
pub enum Error {
    /// windows-rs / WASAPI HRESULT。
    Com(windows::core::Error),
    /// 目标进程不存在或音频会话已结束。
    ProcessNotFound(u32),
    /// 输出设备 ID 无效或设备已拔出。
    DeviceNotFound,
    /// Process Loopback 初始化失败。
    CaptureInit(&'static str),
    /// 渲染端 Initialize / 打开设备失败。
    RenderInit(&'static str),
    /// 同一驱动的多个端点不能同时打开（如 VB-Cable 的扬声器与 In 16ch）。
    DeviceBusy(String),
    /// mix 格式无法解析或无法转换。
    Format(&'static str),
    /// WAV 等文件 IO。
    Io(io::Error),
    /// 激活或等待超时。
    TimedOut(&'static str),
    /// CLI / UI 参数。
    InvalidArgs(String),
    /// VB-CABLE 下载或安装失败。
    Cable(String),
}

impl Error {
    /// 给用户看的短句，不含 HRESULT。
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

    /// 稳定的诊断代码，用于归类与复制。
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

    /// 建议用户采取的动作；无建议时返回空串。
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

    /// 技术细节（含 HRESULT），供问题中心详情展示。
    pub fn detail(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Error {
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

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Com(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<windows::core::Error> for Error {
    fn from(value: windows::core::Error) -> Self {
        Self::Com(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
