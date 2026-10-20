//! 音频基础设施模块：组织 WASAPI 捕获、渲染、格式与缓冲组件，并集中导出设备枚举及默认设备策略接口。

// 固定容量的音频数据缓冲与实时线程间传递。
pub mod buffer;

// WASAPI 捕获、设备枚举与系统默认设备策略的 COM 边界分别由子模块承载。
pub mod capture;
pub mod com;
pub mod decode;
pub mod device;
pub mod dump;
mod errors;
pub mod format;
pub mod policy;
pub mod preview;
pub mod render;
pub mod sfx;
pub mod wav;
