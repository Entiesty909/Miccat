//! AudioEngine 状态的只读查询接口。

use std::sync::atomic::Ordering;

use super::{AudioEngine, EngineStatus};

/// AudioEngine 的只读状态查询实现；不参与状态转换，只读取共享快照。
impl AudioEngine {
    /// 返回当前引擎状态快照。
    pub fn status(&self) -> EngineStatus {
        // 锁异常时返回显式错误状态，避免把状态查询失败伪装成空闲。
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or(EngineStatus::Error("status lock".into()))
    }

    /// 返回路由是否处于运行状态。
    /// 使用 Acquire 读取与输出线程 Release 配对发布的生命周期标志。
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}
