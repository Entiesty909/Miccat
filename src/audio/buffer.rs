//! Capture → Render 的 SPSC 环形缓冲。音频线程不分配。

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// 生产端退让时的轮询间隔：够短以跟上消费端，又不空转。
const POLL: Duration = Duration::from_millis(2);
/// 消费端停摆判定上限；超过则生产端改为丢最旧保实时。
const STALL_LIMIT: Duration = Duration::from_millis(250);

/// 单生产者单消费者 f32 交错采样环。
///
/// 容量为 2 的幂。写满时丢掉最旧数据，把延迟钉在环长度内。
pub struct SpscRing {
    /// 采样存储。SPSC + 原子头尾指针，故用 UnsafeCell 做内部可变。
    buf: UnsafeCell<Box<[f32]>>,
    /// `capacity - 1`，用于取模。
    mask: usize,
    /// 生产者写入位置（单调递增）。
    write: AtomicUsize,
    /// 消费者读取位置（单调递增）。
    read: AtomicUsize,
}

// SAFETY: 只有一个生产者写入 `write` 所指槽位、一个消费者读取 `read` 所指槽位；两端
// 通过 Release/Acquire 发布索引后，不会同时修改同一个样本槽位。
unsafe impl Send for SpscRing {}
unsafe impl Sync for SpscRing {}

/// 环缓冲实现维护单调索引；索引回绕依赖容量为二的幂且只保留一个空槽。
impl SpscRing {
    /// 按最少采样数分配，向上取 2 的幂，至少 4096。
    pub fn with_capacity_samples(min_samples: usize) -> Self {
        // 2 的幂才能用 mask 代替取模。
        // `mask` 把单调索引映射到物理槽位，避免实时路径执行取模运算。
        let cap = min_samples.next_power_of_two().max(4096);
        Self {
            buf: UnsafeCell::new(vec![0.0; cap].into_boxed_slice()),
            mask: cap - 1,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    /// 环容量（采样数，含一个空槽约定下实际可用 cap-1）。
    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// 当前已占用采样数。
    pub fn len(&self) -> usize {
        // 先获取两端索引，差值在无符号回绕下仍表示环内积压量。
        let w = self.write.load(Ordering::Acquire);
        let r = self.read.load(Ordering::Acquire);
        w.wrapping_sub(r)
    }
    /// 丢弃当前积压样本，切换独立播放源时使用。
    pub fn clear(&self) {
        // 发布端把读取位置追到当前写入位置，消费者随后看到的是空环。
        let write = self.write.load(Ordering::Acquire);
        self.read.store(write, Ordering::Release);
    }

    /// 生产端按消费进度写入：环内积压超过 `lead` 时退让等待，让节拍由消费端决定。
    ///
    /// `lead` 是允许的积压上限（采样数），即生产端最多领先消费端这么多，避免
    /// 写满后 `push_latest` 丢弃最旧数据导致消费端只听到被覆盖的一小段。
    /// 消费端停摆超过 `STALL_LIMIT` 时不再等待，交回 `push_latest` 保实时。
    pub fn push_paced(&self, src: &[f32], lead: usize, stop: &AtomicBool) {
        let deadline = Instant::now() + STALL_LIMIT;
        // 生产者只在积压超过节拍上限时让出线程，避免正常情况下覆盖未播放样本。
        while self.len() + src.len() > lead && !stop.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(POLL);
        }
        self.push_latest(src);
    }

    /// 写入最新采样。空间不足时先丢最旧，保证实时路径不阻塞。
    pub fn push_latest(&self, src: &[f32]) {
        if src.is_empty() {
            return;
        }
        let cap = self.capacity();
        // 单包比环还大：只留最后 cap-1 个采样。
        if src.len() >= cap {
            let src = &src[src.len() - (cap - 1)..];
            self.overwrite(src);
            return;
        }
        // 读取端以 Acquire 观察消费者已释放的槽位，再决定本次可写空间。
        let mut w = self.write.load(Ordering::Relaxed);
        let r = self.read.load(Ordering::Acquire);
        let used = w.wrapping_sub(r);
        let free = cap.wrapping_sub(used).saturating_sub(1);
        if src.len() > free {
            let drop = src.len() - free;
            self.read.store(r.wrapping_add(drop), Ordering::Release);
        }
        // SAFETY: SPSC 约束保证此时只有生产者访问写入槽；消费者只读已发布索引之前的数据。
        let buf = unsafe { &mut *self.buf.get() };
        for &sample in src {
            buf[w & self.mask] = sample;
            w = w.wrapping_add(1);
        }
        self.write.store(w, Ordering::Release);
    }

    /// 覆盖式写入，读写指针一起跳到最新窗口。
    fn overwrite(&self, src: &[f32]) {
        // SAFETY: 调用者仍持有唯一生产端访问权；`src` 只覆盖即将发布的新窗口。
        let mut w = self.write.load(Ordering::Relaxed);
        // SAFETY: 生产者独占写端；`src` 已被限制为可容纳的最新窗口。
        let buf = unsafe { &mut *self.buf.get() };
        for &sample in src {
            buf[w & self.mask] = sample;
            w = w.wrapping_add(1);
        }
        self.write.store(w, Ordering::Release);
        self.read
            .store(w.wrapping_sub(src.len()), Ordering::Release);
    }

    /// 从环弹出到 `dst`。返回实际弹出数量；不足部分由调用方填静音。
    pub fn pop(&self, dst: &mut [f32]) -> usize {
        // Acquire 确保消费者只读取生产者 Release 发布完成的样本。
        let mut r = self.read.load(Ordering::Relaxed);
        let w = self.write.load(Ordering::Acquire);
        let available = w.wrapping_sub(r).min(dst.len());
        // SAFETY: 消费者独占读取端；生产者不会改写尚未通过 `write` 发布的槽位。
        let buf = unsafe { &*self.buf.get() };
        for sample in dst.iter_mut().take(available) {
            *sample = buf[r & self.mask];
            r = r.wrapping_add(1);
        }
        self.read.store(r, Ordering::Release);
        available
    }
}
