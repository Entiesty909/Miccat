//! 全局诊断环形缓冲：线程安全地收集错误与警告。
//!
//! 诊断中心是底层模块，只依赖 `std` 与 `windows`，不触碰 UI 或桥接层。
//! 所有状态用 `Mutex` + 原子量保护，`Send + Sync`，可从任意线程（含 tracing
//! layer 回调线程）安全写入。内部锁不会被污染：`report` 等写入路径只调用
//! `push_back` / `pop_front` 等不会 panic 的操作，锁中毒时统一走兜底分支。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use windows::Win32::System::SystemInformation::GetLocalTime;

/// 全局单例容量；足够覆盖一次会话里用户真正关心的历史。
const SHARED_CAPACITY: usize = 200;

/// 诊断级别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    /// 普通信息，仅记录。
    Info,
    /// 警告：功能降级但仍可用。
    Warn,
    /// 错误：功能失败或不可用。
    Error,
}

impl Severity {
    /// Slint 整数编码：0 信息、1 警告、2 错误。
    pub(crate) fn code(self) -> i32 {
        match self {
            Self::Info => 0,
            Self::Warn => 1,
            Self::Error => 2,
        }
    }

    /// 级别中文名，用于复制文本与列表。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Info => "信息",
            Self::Warn => "警告",
            Self::Error => "错误",
        }
    }
}

/// 一条用户可见的诊断。
#[derive(Clone, Debug)]
pub(crate) struct Diagnostic {
    /// 单调递增的稳定标识，用于删除与列表 diff。
    pub(crate) id: u64,
    /// 级别。
    pub(crate) severity: Severity,
    /// 模块标签，如 "audio.capture"。
    pub(crate) source: String,
    /// 用户短句。
    pub(crate) title: String,
    /// 技术细节 / HRESULT。
    pub(crate) detail: String,
    /// 建议动作，可为空。
    pub(crate) hint: String,
    /// 本地时间 "HH:MM:SS"。
    pub(crate) time: String,
}

/// 全局诊断环形缓冲。
pub(crate) struct DiagnosticHub {
    /// 环形缓冲，最旧在前、最新在后。
    entries: Mutex<VecDeque<Diagnostic>>,
    /// 自上次 `mark_seen` 以来新增的条数。
    unseen: AtomicU64,
    /// 下一条诊断的 id。
    next_id: AtomicU64,
    /// 最大条数。
    capacity: usize,
}

impl DiagnosticHub {
    /// 创建容量为 `capacity` 的中心；容量为 0 时按 1 处理，保证至少留一条。
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
            unseen: AtomicU64::new(0),
            next_id: AtomicU64::new(1),
            capacity: capacity.max(1),
        }
    }

    /// 追加一条诊断；满容量时丢弃最旧。
    ///
    /// 若最新一条的 severity/source/title 全同，则只更新其 time 不新增，
    /// 避免同一问题在短时间内刷屏；此时未读计数也不增加。
    pub(crate) fn report(
        &self,
        severity: Severity,
        source: &str,
        title: &str,
        detail: &str,
        hint: &str,
    ) {
        let time = local_time();
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if let Some(latest) = entries.back_mut() {
            let same =
                latest.severity == severity && latest.source == source && latest.title == title;
            if same {
                latest.time = time;
                return;
            }
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        while entries.len() >= self.capacity {
            entries.pop_front();
        }
        entries.push_back(Diagnostic {
            id,
            severity,
            source: source.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            hint: hint.to_string(),
            time,
        });
        self.unseen.fetch_add(1, Ordering::Relaxed);
    }

    /// 快照，最新在前。
    pub(crate) fn snapshot(&self) -> Vec<Diagnostic> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        entries.iter().rev().cloned().collect()
    }

    /// 未读计数。
    pub(crate) fn unseen(&self) -> usize {
        self.unseen.load(Ordering::Relaxed) as usize
    }

    /// 标记全部已读。
    pub(crate) fn mark_seen(&self) {
        self.unseen.store(0, Ordering::Relaxed);
    }

    /// 清空；未读计数一并归零。
    pub(crate) fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        self.unseen.store(0, Ordering::Relaxed);
    }

    /// 按 id 删除；不存在时不做任何事。
    pub(crate) fn dismiss(&self, id: u64) {
        if let Ok(mut entries) = self.entries.lock()
            && let Some(index) = entries.iter().position(|entry| entry.id == id)
        {
            entries.remove(index);
        }
    }
}

/// 全局单例；容量 200。
pub(crate) fn shared() -> &'static Arc<DiagnosticHub> {
    static HUB: LazyLock<Arc<DiagnosticHub>> =
        LazyLock::new(|| Arc::new(DiagnosticHub::new(SHARED_CAPACITY)));
    &HUB
}

/// 取本地时间 "HH:MM:SS"。
fn local_time() -> String {
    // `GetLocalTime` 无失败路径，总是回填 SYSTEMTIME，因此这里不需要错误分支。
    let time = unsafe { GetLocalTime() };
    format!("{:02}:{:02}:{:02}", time.wHour, time.wMinute, time.wSecond)
}
