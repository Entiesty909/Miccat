//! AudioEngine 状态的只读查询接口。

use std::sync::atomic::Ordering;

use super::{AudioEngine, EngineStatus};

impl AudioEngine {
    /// 返回当前引擎状态快照。
    pub fn status(&self) -> EngineStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or(EngineStatus::Error("status lock".into()))
    }

    /// 返回路由是否处于运行状态。
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}
