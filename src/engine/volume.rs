//! AudioEngine 源增益的原子读写接口。

use std::sync::atomic::Ordering;

use super::AudioEngine;

impl AudioEngine {
    /// 设置进程源线性增益。
    pub fn set_volume(&self, value: f32) {
        self.volume
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// 设置物理麦克风源线性增益。
    pub fn set_mic_volume(&self, value: f32) {
        self.mic_volume
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// 设置最终输出总线线性增益。
    pub fn set_master_volume(&self, value: f32) {
        self.master
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// 设置音频库文件源线性增益。
    pub fn set_sfx_volume(&self, value: f32) {
        self.sfx_volume
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// 返回当前进程源线性增益。
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

    fn read_gain(&self, gain: &std::sync::Arc<std::sync::atomic::AtomicU32>) -> f32 {
        f32::from_bits(gain.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }
}
