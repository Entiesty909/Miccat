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

/// 提供编码与展示文本转换；诊断中心只保存这一组有限级别。

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

/// 在固定容量内串行维护诊断顺序，并用原子计数暴露未读状态。

impl DiagnosticHub {
    /// 创建容量为 `capacity` 的中心；容量为 0 时按 1 处理，保证至少留一条。
    pub(crate) fn new(capacity: usize) -> Self {
        // 零容量也按一处理，避免报告成功却无法保留任何诊断。
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
        // 时间格式化放在加锁前，减少持锁时间；后续只在队列上做短操作。
        let time = local_time();
        let Ok(mut entries) = self.entries.lock() else {
            // 诊断不能反过来影响业务线程，锁中毒时直接丢弃本条记录。
            return;
        };
        if let Some(latest) = entries.back_mut() {
            let same =
                latest.severity == severity && latest.source == source && latest.title == title;
            if same {
                // 相同问题只刷新时间，避免短时间内重复占满环形缓冲和未读计数。
                latest.time = time;
                return;
            }
        }
        // 原子序列为每条新增诊断分配稳定 id，快照和删除可据此定位条目。

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        while entries.len() >= self.capacity {
            // 满容量时淘汰最旧项，保持最新故障始终可见。
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
        // 只有真正新增条目才增加未读数；重复合并路径已提前返回。
        self.unseen.fetch_add(1, Ordering::Relaxed);
    }
    /// 快照，最新在前；锁内只做克隆，避免把队列引用泄露到调用方。
    pub(crate) fn snapshot(&self) -> Vec<Diagnostic> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        // 内部按时间正序保留，界面快照反转后可直接从最新问题开始展示。
        entries.iter().rev().cloned().collect()
    }

    /// 当前未读条数；原子读取不必阻塞诊断列表快照。

    pub(crate) fn unseen(&self) -> usize {
        self.unseen.load(Ordering::Relaxed) as usize
    }

    /// 把当前计数归零；已有条目仍保留供用户查看。

    pub(crate) fn mark_seen(&self) {
        self.unseen.store(0, Ordering::Relaxed);
    }

    /// 清空所有条目并同步清除未读状态，保持列表与角标一致。

    pub(crate) fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        self.unseen.store(0, Ordering::Relaxed);
    }

    /// 删除指定条目；未读计数不回退，因为它表示自上次标记以来发生过的新问题。
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
    // LazyLock 只初始化一次；Arc 让 tracing layer 与 UI 共享同一个中心。
    static HUB: LazyLock<Arc<DiagnosticHub>> =
        LazyLock::new(|| Arc::new(DiagnosticHub::new(SHARED_CAPACITY)));
    &HUB
}

/// 取本地时间 "HH:MM:SS"。
fn local_time() -> String {
    // SAFETY: `GetLocalTime` 只返回系统填充的 SYSTEMTIME 值，不接收指针；调用线程无需
    // 额外的 COM apartment 或外部资源生命周期，返回值可立即读取。
    let time = unsafe { GetLocalTime() };
    format!("{:02}:{:02}:{:02}", time.wHour, time.wMinute, time.wSecond)
}
