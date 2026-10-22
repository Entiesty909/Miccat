//! 声板混音线程：读取已经解码的 WAV，在固定 ring 中混合并输出。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use super::buffer::SpscRing;
use super::format::AudioFormat;
use std::sync::mpsc::{self, Receiver, Sender};

/// 后台解码线程与 SFX 混音线程之间传递的有界音频块。
pub struct StreamChunk {
    // samples 的长度必须始终是声道数的整数倍；消费者按交错帧推进游标。
    /// 按帧交错排列的归一化 f32 样本；所有权随消息转移给消费者。
    pub samples: Vec<f32>,
    /// 是否已经到达源流末尾；空块也可作为 EOF 标记。
    pub done: bool,
}

/// 声板控制命令；命令只改变指定 slot，不得清理其他音频卡片。
pub enum SfxCommand {
    /// 消费后台解码的有界音频流。
    PlayStream {
        // slot 是唯一控制键，替换播放器时只回收同一槽位的旧接收端。
        /// 目标音频卡片槽位，与 rings/states 的下标一一对应。
        slot: usize,
        /// 源流采样率，单位为 Hz。
        sample_rate: u32,
        /// 源流声道数；交错样本每帧包含该数量的值。
        channels: u16,
        /// 流起始媒体位置，100 ns；seek 后进度不能重新从零计数。
        start_hns: u64,
        /// 初始线性音量，接收后限制在 0..=1。
        volume: f32,
        /// 解码线程发送的有界块接收端。
        rx: Receiver<StreamChunk>,
    },
    /// 停止一个音频卡片槽位，不影响其他槽位。
    StopSlot(usize),
    // 暂停只改变指定播放器状态，其他槽位继续由本轮混音处理。
    /// 暂停或恢复一个音频卡片槽位，暂停时保留其播放游标。
    SetPaused { slot: usize, paused: bool },
}

/// 创建命令通道；发送端运行在控制线程，接收端只由 SFX 混音线程消费。
pub fn channel() -> (Sender<SfxCommand>, Receiver<SfxCommand>) {
    // 无界控制通道不会阻塞实时混音线程；命令在下一轮 try_recv 时批量应用。
    mpsc::channel()
}

/// 播放器统一抽象；当前仅有流式播放器，但槽位控制不依赖具体来源。
enum SfxPlayer {
    /// 由有界解码通道供给的实时流。
    Stream(StreamPlayer),
}

/// 在实时混音线程内维护每个音频卡片槽位；每个 ring 与 player 通过 slot 下标绑定。
pub fn run_sfx_loop(
    rings: Vec<Arc<SpscRing>>,
    render: AudioFormat,
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU32>,
    states: Arc<Vec<AtomicU8>>,
    positions: Arc<Vec<AtomicU64>>,
    rx: Receiver<SfxCommand>,
) {
    // render 的采样率和声道决定输出块大小，避免热路径硬编码设备格式。
    let channels = render.channels as usize;
    let frames = (render.sample_rate as usize / 100).max(64);
    let lead = frames * channels * 10;
    // 预分配所有槽位的混音块；循环内只清零和覆盖，避免实时线程扩容。
    let mut players: Vec<SfxPlayer> = Vec::with_capacity(rings.len());
    let mut blocks = vec![vec![0.0f32; frames * channels]; rings.len()];
    // blocks 与 rings 按槽位并行排列；即使某槽位没有 player，也必须保留其输出块。
    while !stop.load(std::sync::atomic::Ordering::Acquire) {
        // 命令在混音前批量应用，保证同一轮输出看到一致的槽位状态。
        while let Ok(command) = rx.try_recv() {
            match command {
                SfxCommand::PlayStream {
                    // 新流覆盖同槽位旧流，避免一个卡片同时消费两条解码通道。
                    slot,
                    start_hns,
                    sample_rate,
                    channels,
                    volume,
                    rx,
                } => {
                    players.retain(|player| player.slot() != slot);
                    if let Some(state) = states.get(slot) {
                        // 状态先发布为播放中，UI 可在首个音频块混入前显示一致状态。
                        state.store(1, Ordering::Relaxed);
                    }
                    if let Some(position) = positions.get(slot) {
                        // 新流从请求的媒体位置计数，避免 seek 后进度先跳回零。
                        position.store(start_hns, Ordering::Relaxed);
                    }
                    players.push(SfxPlayer::Stream(StreamPlayer::new(
                        slot,
                        sample_rate,
                        channels,
                        start_hns,
                        volume,
                        positions.clone(),
                        rx,
                    )));
                }
                SfxCommand::StopSlot(slot) => {
                    // 移除播放器会丢弃其未消费缓存，但只影响当前 slot。
                    players.retain(|player| player.slot() != slot);
                    if let Some(state) = states.get(slot) {
                        state.store(0, Ordering::Relaxed);
                    }
                    if let Some(position) = positions.get(slot) {
                        position.store(0, Ordering::Relaxed);
                    }
                }
                SfxCommand::SetPaused { slot, paused } => {
                    // 找不到目标播放器时不创建状态，避免暂停命令凭空改变 UI。
                    if let Some(player) = players.iter_mut().find(|player| player.slot() == slot) {
                        player.set_paused(paused);
                        if let Some(state) = states.get(slot) {
                            state.store(if paused { 2 } else { 1 }, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
        // 每个 player 只写自己的 block，停止一个槽位不会清除其他卡片的缓冲。
        for block in &mut blocks {
            block.fill(0.0);
        }
        players.retain_mut(|player| {
            // retain_mut 在同一遍处理中混音、检测 EOF，并回收已结束播放器。
            if let Some(block) = blocks.get_mut(player.slot()) {
                player.mix_into(block, render);
            }
            let finished = player.finished();
            if finished && let Some(state) = states.get(player.slot()) {
                state.store(3, Ordering::Relaxed);
            }
            !finished
        });
        // 峰值供 UI 观察，采用原子发布且不阻塞音频线程。
        // 峰值只用于观测，不参与混音决策；Relaxed 足以发布近似电平。
        let level = blocks
            .iter()
            .flat_map(|block| block.iter())
            .map(|sample| sample.abs())
            .fold(0.0f32, f32::max);
        peak.fetch_max(level.to_bits(), Ordering::Relaxed);
        // 各槽位独立节流，ring 的下标协议与 players/block 保持一致。
        for (ring, block) in rings.iter().zip(blocks.iter()) {
            // push_paced 按各自 ring 的消费速度施加背压，防止某张卡拖慢或覆盖其他卡。
            ring.push_paced(block, lead, &stop);
        }
    }
}

/// 槽位播放器的统一访问方法；实现块只做状态分派，不拥有跨槽位状态。
impl SfxPlayer {
    /// 返回播放器绑定的卡片槽位，用于与 ring/state 下标对齐。
    fn slot(&self) -> usize {
        match self {
            Self::Stream(player) => player.slot,
        }
    }

    /// 判断源流是否自然结束，结束后由主循环清理该槽位。
    fn finished(&self) -> bool {
        match self {
            Self::Stream(player) => player.finished,
        }
    }

    /// 切换暂停状态；暂停只冻结游标，不丢弃已经缓冲的样本。
    fn set_paused(&mut self, paused: bool) {
        match self {
            Self::Stream(player) => player.paused = paused,
        }
    }

    /// 将当前播放器按目标设备格式混入所属输出块。
    fn mix_into(&mut self, out: &mut [f32], render: AudioFormat) {
        match self {
            Self::Stream(player) => player.mix_into(out, render),
        }
    }
}

/// 一个音频卡片槽位的流式状态；buffer 由混音线程独占，避免实时路径加锁。
struct StreamPlayer {
    /// 与 rings、states 对齐的卡片槽位下标。
    slot: usize,
    /// 解码源采样率，单位为 Hz。
    sample_rate: u32,
    /// 解码源的交错声道数。
    channels: usize,
    /// 线性增益，构造时限制在 0..=1。
    volume: f32,
    /// 后台解码块接收端，try_recv 保证混音线程不阻塞。
    rx: Receiver<StreamChunk>,
    /// 已收到但尚未混入输出的交错源样本。
    buffer: Vec<f32>,
    /// 源帧游标，允许非整数步长完成采样率转换。
    position: f64,
    /// 从流开始累计的源帧数，不受本地缓冲前缀回收影响。
    played_frames: f64,
    /// 与槽位状态并列的进度原子；每个混音块只写一次。
    positions: Arc<Vec<AtomicU64>>,
    /// 是否收到源 EOF 标记。
    done: bool,
    /// 是否已消费完 EOF 后的缓冲，供主循环移除播放器。
    finished: bool,
    /// 暂停时保留 buffer 与 position，仅跳过本轮混音。
    paused: bool,
}

/// 流式播放器的预取、重采样和声道混合实现。
impl StreamPlayer {
    /// 创建一个绑定槽位的流播放器；音量在入口归一化，后续热路径无需重复检查。
    fn new(
        slot: usize,
        sample_rate: u32,
        channels: u16,
        start_hns: u64,
        volume: f32,
        positions: Arc<Vec<AtomicU64>>,
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
            played_frames: start_hns as f64 * f64::from(sample_rate) / 10_000_000.0,
            positions,
            done: false,
            finished: false,
            paused: false,
        }
    }
    /// 尝试把有界通道中的块预取到本地缓存；try_recv 保证实时线程绝不等待解码线程。
    fn pump(&mut self) {
        // 缓存上限按源声道计算，控制延迟和内存占用；done 后不再读取通道。
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
    /// 将缓存中的源帧重采样并混入目标输出；暂停时保留游标和缓存不消费数据。
    fn mix_into(&mut self, out: &mut [f32], render: AudioFormat) {
        if self.paused {
            return;
        }
        self.pump();
        // 先预取再计算输出，允许上一轮恰好耗尽缓存时立即继续而不产生空洞。
        if self.sample_rate == 0 {
            self.finished = true;
            return;
        }
        let out_channels = render.channels as usize;
        // 采样率比值是源帧游标的步长；输出帧数始终由设备格式决定。
        let step = f64::from(self.sample_rate) / f64::from(render.sample_rate);
        for dst in out.chunks_exact_mut(out_channels) {
            // 每次只处理完整目标帧，避免交错输出被半帧写入。
            let src_frame = self.position.floor() as usize;
            if src_frame >= self.buffer.len() / self.channels {
                if self.done {
                    self.finished = true;
                }
                break;
            }
            let src = &self.buffer[src_frame * self.channels..(src_frame + 1) * self.channels];
            // 声道不足时复制最后一个可用源声道，保证单声道源也能填满目标帧。
            for (channel, value) in dst.iter_mut().enumerate() {
                *value += src[if self.channels == 1 {
                    0
                } else {
                    channel.min(self.channels - 1)
                }] * self.volume;
            }
            self.position += step;
            self.played_frames += step;
        }
        let drop_frames = self.position.floor() as usize;
        // 仅在游标积累足够多时回收前缀，避免每个输出块都触发 Vec::drain。
        if drop_frames > 2048 {
            let available = self.buffer.len() / self.channels;
            let consumed = drop_frames.min(available);
            if consumed > 0 {
                let drop = consumed * self.channels;
                self.buffer.drain(..drop);
                self.position -= consumed as f64;
            }
        }
        if let Some(position) = self.positions.get(self.slot) {
            let hns = (self.played_frames * 10_000_000.0 / f64::from(self.sample_rate)) as u64;
            position.store(hns, Ordering::Relaxed);
        }
        self.pump();
        // 混音后再次预取，下一轮可直接使用已到达的块并平滑衔接。
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamChunk, StreamPlayer};
    use crate::audio::format::{AudioFormat, SampleKind};
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, mpsc};

    /// 验证暂停不会推进源游标，恢复后仍从已有缓存继续播放。
    #[test]
    fn paused_stream_holds_position_and_resumes_from_buffered_audio() {
        let (tx, rx) = mpsc::channel();
        tx.send(StreamChunk {
            samples: vec![0.5; 8],
            done: false,
        })
        .unwrap();
        let positions = Arc::new(vec![AtomicU64::new(0)]);
        let mut player = StreamPlayer::new(0, 48_000, 1, 0, 1.0, positions, rx);
        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 1,
            kind: SampleKind::F32,
        };
        let mut output = [0.0; 4];

        player.paused = true;
        // 暂停路径不应取走通道数据；恢复后同一缓存必须生成预期样本。
        player.mix_into(&mut output, format);
        assert_eq!(output, [0.0; 4]);
        assert!(player.buffer.is_empty());

        player.paused = false;
        player.mix_into(&mut output, format);
        assert_eq!(output, [0.5; 4]);
    }
    /// 从中途恢复时对外位置必须保持媒体时间，而不是把当前片段当作零点。
    #[test]
    fn seeked_stream_reports_absolute_media_position() {
        let (tx, rx) = mpsc::channel();
        tx.send(StreamChunk {
            samples: vec![0.25; 8],
            done: false,
        })
        .unwrap();
        let positions = Arc::new(vec![AtomicU64::new(0)]);
        let mut player = StreamPlayer::new(0, 48_000, 1, 5_000_000, 1.0, positions.clone(), rx);
        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 1,
            kind: SampleKind::F32,
        };
        let mut output = [0.0; 4];
        player.mix_into(&mut output, format);
        assert_eq!(output, [0.25; 4]);
        let position = positions[0].load(std::sync::atomic::Ordering::Relaxed);
        assert!((5_000_800..=5_000_900).contains(&position));
    }
}
