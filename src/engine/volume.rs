//! AudioEngine 源增益的原子读写接口。
//!
//! 增益以 `f32` 的位模式存入 `AtomicU32`：控制线程负责写入，render 线程只需原子读取，
//! 不必为每个音频块加锁。进程槽和 SFX 槽各自持有原子值，互不影响。

use std::sync::atomic::Ordering;

use super::AudioEngine;

/// AudioEngine 的源增益实现；每个槽位独立存储，render 线程只做原子读。
impl AudioEngine {
    /// 设置指定进程卡片的线性增益。
    /// 写入槽位增益；值以线性倍率表示并限制在 0 到 1。
    pub fn set_process_volume(&self, slot: usize, value: f32) {
        // 无效槽位不创建新状态，调用方可安全地处理动态槽位集合。
        if let Some(gain) = self.process_volumes.get(slot) {
            // 先钳制再转位，确保 render 线程永远看到合法的线性倍率。
            gain.store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    /// 返回指定进程卡片的线性增益。
    /// 读取槽位增益；槽位不存在时返回满增益以保持旧调用兼容。
    pub fn process_volume(&self, slot: usize) -> f32 {
        self.process_volumes
            .get(slot)
            .map(|gain| self.read_gain(gain))
            // 缺少槽位按默认满增益处理，避免旧路由被意外静音。
            .unwrap_or(1.0)
    }

    /// 设置所有进程卡片的默认增益，兼容旧配置入口。
    /// 同步更新兼容旧配置的默认值及所有进程槽，不改变槽位独立性。
    pub fn set_volume(&self, value: f32) {
        // 旧配置入口同时更新默认值和现有槽位；之后新槽位仍可独立调节。
        let value = value.clamp(0.0, 1.0);
        self.volume.store(value.to_bits(), Ordering::Relaxed);
        for gain in &self.process_volumes {
            gain.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// 设置物理麦克风源线性增益。
    /// 写入物理麦克风源增益；Relaxed 足以传递单个数值。
    pub fn set_mic_volume(&self, value: f32) {
        // 麦克风是独立源，不复用进程或 SFX 的槽位增益。
        self.mic_volume
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// 设置最终输出总线线性增益。
    /// 写入最终输出总线增益，应用于所有已路由源的混音结果。
    pub fn set_master_volume(&self, value: f32) {
        // 总线增益在各源混音之后应用，因此不会改变单个卡片的独立音量。
        self.master
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// 设置所有音频卡片的默认增益，兼容旧配置入口。
    /// 同步更新兼容旧配置的 SFX 默认值及所有音频槽。
    pub fn set_sfx_volume(&self, value: f32) {
        // 兼容入口广播到现有槽位；单槽 API 仍可在之后覆盖某一张卡片。
        let value = value.clamp(0.0, 1.0);
        self.sfx_volume.store(value.to_bits(), Ordering::Relaxed);
        for gain in &self.sfx_volumes {
            gain.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// 设置指定音频卡片的线性增益。
    /// 写入指定音频槽增益；只影响该槽，不会停止或重建其流。
    pub fn set_sfx_volume_for_slot(&self, slot: usize, value: f32) {
        // 只更新目标槽的原子值，保证其他音频卡片继续使用各自的增益。
        if let Some(gain) = self.sfx_volumes.get(slot) {
            gain.store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    /// 返回指定音频卡片的线性增益。
    /// 读取指定音频槽增益；槽位不存在时返回满增益。
    pub fn sfx_volume_for_slot(&self, slot: usize) -> f32 {
        self.sfx_volumes
            .get(slot)
            .map(|gain| self.read_gain(gain))
            // 与进程槽保持一致：未知槽位不应把兼容调用静音。
            .unwrap_or(1.0)
    }
    /// 返回当前进程源线性增益。
    /// 返回兼容旧 API 的进程默认增益，不代表某个独立进程槽的覆盖值。
    pub fn volume(&self) -> f32 {
        self.read_gain(&self.volume)
    }

    /// 返回当前物理麦克风源线性增益。
    pub fn mic_volume(&self) -> f32 {
        self.read_gain(&self.mic_volume)
    }

    /// 返回当前音频库文件源线性增益。
    pub fn sfx_volume(&self) -> f32 {
        self.read_gain(&self.sfx_volume)
    }

    /// 返回最终输出总线线性增益。
    pub fn master_volume(&self) -> f32 {
        self.read_gain(&self.master)
    }

    /// 以位模式原子读取线性增益，并再次钳制外部可能写入的异常值。
    fn read_gain(&self, gain: &std::sync::Arc<std::sync::atomic::AtomicU32>) -> f32 {
        // Relaxed 保证单次数值读写的原子性；增益本身不承载其他同步信号。
        f32::from_bits(gain.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }
}
