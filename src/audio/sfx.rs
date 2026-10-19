//! 声板混音线程：读取已经解码的 WAV，在固定 ring 中混合并输出。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::buffer::SpscRing;
use super::format::AudioFormat;
use std::sync::mpsc::{self, Receiver, Sender};

/// 声板控制命令。
pub struct StreamChunk {
    /// 交错 f32 音频块。
    pub samples: Vec<f32>,
    /// 是否到达文件尾。
    pub done: bool,
}

/// 声板控制命令。
pub enum SfxCommand {
    /// 消费后台解码的有界音频流。
    PlayStream {
        /// 目标音频卡片槽位。
        slot: usize,
        sample_rate: u32,
        channels: u16,
        volume: f32,
        rx: Receiver<StreamChunk>,
    },
    /// 停止一个音频卡片槽位。
    StopSlot(usize),
}

/// 创建声板命令通道。
pub fn channel() -> (Sender<SfxCommand>, Receiver<SfxCommand>) {
    mpsc::channel()
}

pub fn run_sfx_loop(
    rings: Vec<Arc<SpscRing>>,
    render: AudioFormat,
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU32>,
    rx: Receiver<SfxCommand>,
) {
    let channels = render.channels as usize;
    let frames = (render.sample_rate as usize / 100).max(64);
    let lead = frames * channels * 10;
    let mut players: Vec<SfxPlayer> = Vec::with_capacity(rings.len());
    let mut blocks = vec![vec![0.0f32; frames * channels]; rings.len()];
    while !stop.load(std::sync::atomic::Ordering::Acquire) {
        while let Ok(command) = rx.try_recv() {
            match command {
                SfxCommand::PlayStream {
                    slot,
                    sample_rate,
                    channels,
                    volume,
                    rx,
                } => {
                    players.retain(|player| player.slot() != slot);
                    players.push(SfxPlayer::Stream(StreamPlayer::new(
                        slot,
                        sample_rate,
                        channels,
                        volume,
                        rx,
                    )));
                }
                SfxCommand::StopSlot(slot) => {
                    players.retain(|player| player.slot() != slot);
                }
            }
        }
        for block in &mut blocks {
            block.fill(0.0);
        }
        players.retain_mut(|player| {
            if let Some(block) = blocks.get_mut(player.slot()) {
                player.mix_into(block, render);
            }
            !player.finished()
        });
        let level = blocks
            .iter()
            .flat_map(|block| block.iter())
            .map(|sample| sample.abs())
            .fold(0.0f32, f32::max);
        peak.fetch_max(level.to_bits(), Ordering::Relaxed);
        for (ring, block) in rings.iter().zip(blocks.iter()) {
            ring.push_paced(block, lead, &stop);
        }
    }
}
enum SfxPlayer {
    Stream(StreamPlayer),
}

impl SfxPlayer {
    fn slot(&self) -> usize {
        match self {
            Self::Stream(player) => player.slot,
        }
    }

    fn finished(&self) -> bool {
        match self {
            Self::Stream(player) => player.finished,
        }
    }

    fn mix_into(&mut self, out: &mut [f32], render: AudioFormat) {
        match self {
            Self::Stream(player) => player.mix_into(out, render),
        }
    }
}
struct StreamPlayer {
    slot: usize,
    sample_rate: u32,
    channels: usize,
    volume: f32,
    rx: Receiver<StreamChunk>,
    buffer: Vec<f32>,
    position: f64,
    done: bool,
    finished: bool,
}
impl StreamPlayer {
    fn new(
        slot: usize,
        sample_rate: u32,
        channels: u16,
        volume: f32,
        rx: Receiver<StreamChunk>,
    ) -> Self {
        Self {
            slot,
            sample_rate,
            channels: channels.max(1) as usize,
            volume: volume.clamp(0.0, 1.0),
            rx,
            buffer: Vec::with_capacity(64_000),
            position: 0.0,
            done: false,
            finished: false,
        }
    }
    fn pump(&mut self) {
        while self.buffer.len() < self.channels * 24_000 && !self.done {
            match self.rx.try_recv() {
                Ok(chunk) => {
                    self.buffer.extend_from_slice(&chunk.samples);
                    self.done = chunk.done;
                }
                Err(_) => break,
            }
        }
    }
    fn mix_into(&mut self, out: &mut [f32], render: AudioFormat) {
        self.pump();
        if self.sample_rate == 0 {
            self.finished = true;
            return;
        }
        let out_channels = render.channels as usize;
        let step = f64::from(self.sample_rate) / f64::from(render.sample_rate);
        for dst in out.chunks_exact_mut(out_channels) {
            let src_frame = self.position.floor() as usize;
            if src_frame >= self.buffer.len() / self.channels {
                if self.done {
                    self.finished = true;
                }
                break;
            }
            let src = &self.buffer[src_frame * self.channels..(src_frame + 1) * self.channels];
            for (channel, value) in dst.iter_mut().enumerate() {
                *value += src[if self.channels == 1 {
                    0
                } else {
                    channel.min(self.channels - 1)
                }] * self.volume;
            }
            self.position += step;
        }
        let drop_frames = self.position.floor() as usize;
        if drop_frames > 2048 {
            let available = self.buffer.len() / self.channels;
            let consumed = drop_frames.min(available);
            if consumed > 0 {
                let drop = consumed * self.channels;
                self.buffer.drain(..drop);
                self.position -= consumed as f64;
            }
        }
        self.pump();
    }
}
