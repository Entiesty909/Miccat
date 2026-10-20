//! 音频模块对共享 COM 基础设施的安全转发。
/// 音频捕获与输出共用平台层的 MTA 初始化，确保线程 apartment 规则一致。
pub(crate) use crate::platform::com::init_mta;
