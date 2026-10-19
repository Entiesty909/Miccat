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
        sample_rate: u32,
        channels: u16,
        volume: f32,
        rx: Receiver<StreamChunk>,
    },
    /// 清空所有声音。
    StopAll,
}

/// 创建声板命令通道。
pub fn channel() -> (Sender<SfxCommand>, Receiver<SfxCommand>) {
    mpsc::channel()
}

/// SFX mixer：从有界流构建固定块并记录峰值。
pub fn run_sfx_loop(
    ring: Arc<SpscRing>,
    render: AudioFormat,
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU32>,
    rx: Receiver<SfxCommand>,
) {
    let channels = render.channels as usize;
    let frames = (render.sample_rate as usize / 100).max(64);
    // 允许领先 render 端 10 块（约 100 ms）；超过就等 render 消费，不用固定 sleep 追时间。
    // 固定 sleep 会因 Windows 计时器粒度慢于实时，把环慢慢掏空；写满则由 push_latest
    // 丢最旧并顶掉读指针，听起来就是同一小段被反复覆盖。
    let lead = frames * channels * 10;
    let mut players: Vec<SfxPlayer> = Vec::with_capacity(1);
    let mut block = vec![0.0f32; frames * channels];
    while !stop.load(std::sync::atomic::Ordering::Acquire) {
        while let Ok(command) = rx.try_recv() {
            match command {
                SfxCommand::PlayStream {
                    sample_rate,
                    channels,
                    volume,
                    rx,
                } => {
                    players.clear();
                    players.push(SfxPlayer::Stream(StreamPlayer::new(
                        sample_rate,
                        channels,
                        volume,
                        rx,
                    )));
                }
                SfxCommand::StopAll => players.clear(),
            }
        }
        block.fill(0.0);
        players.retain_mut(|player| {
            player.mix_into(&mut block, render);
            !player.finished()
        });
        let level = block
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0f32, f32::max);
        peak.fetch_max(level.to_bits(), Ordering::Relaxed);
        ring.push_paced(&block, lead, &stop);
    }
}
enum SfxPlayer {
    Stream(StreamPlayer),
}

impl SfxPlayer {
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
    fn new(sample_rate: u32, channels: u16, volume: f32, rx: Receiver<StreamChunk>) -> Self {
        Self {
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
