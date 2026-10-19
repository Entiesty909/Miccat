//! AudioEngine 源增益的原子读写接口。

use std::sync::atomic::Ordering;

use super::AudioEngine;

impl AudioEngine {
    /// 设置指定进程卡片的线性增益。
    pub fn set_process_volume(&self, slot: usize, value: f32) {
        if let Some(gain) = self.process_volumes.get(slot) {
            gain.store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    /// 返回指定进程卡片的线性增益。
    pub fn process_volume(&self, slot: usize) -> f32 {
        self.process_volumes
            .get(slot)
            .map(|gain| self.read_gain(gain))
            .unwrap_or(1.0)
    }

    /// 设置所有进程卡片的默认增益，兼容旧配置入口。
    pub fn set_volume(&self, value: f32) {
        let value = value.clamp(0.0, 1.0);
        self.volume.store(value.to_bits(), Ordering::Relaxed);
        for gain in &self.process_volumes {
            gain.store(value.to_bits(), Ordering::Relaxed);
        }
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

    /// 设置所有音频卡片的默认增益，兼容旧配置入口。
    pub fn set_sfx_volume(&self, value: f32) {
        let value = value.clamp(0.0, 1.0);
        self.sfx_volume.store(value.to_bits(), Ordering::Relaxed);
        for gain in &self.sfx_volumes {
            gain.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// 设置指定音频卡片的线性增益。
    pub fn set_sfx_volume_for_slot(&self, slot: usize, value: f32) {
        if let Some(gain) = self.sfx_volumes.get(slot) {
            gain.store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    /// 返回指定音频卡片的线性增益。
    pub fn sfx_volume_for_slot(&self, slot: usize) -> f32 {
        self.sfx_volumes
            .get(slot)
            .map(|gain| self.read_gain(gain))
            .unwrap_or(1.0)
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
    /// 返回最终输出总线线性增益。
    pub fn master_volume(&self) -> f32 {
        self.read_gain(&self.master)
    }

    fn read_gain(&self, gain: &std::sync::Arc<std::sync::atomic::AtomicU32>) -> f32 {
        f32::from_bits(gain.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }
}
