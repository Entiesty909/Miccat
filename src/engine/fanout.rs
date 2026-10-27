//! 多输出 fan-out、格式转换和目标 render 线程工厂。

use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::audio::buffer::SpscRing;
use crate::audio::com;
use crate::audio::format::{AudioFormat, Converter};
use crate::audio::render::run_render_loop;
use crate::error::Error;

use super::{AudioEngine, EngineStatus, TargetState};

/// 一个输出目标在 fanout 线程中的预计算视图；只保存环引用、格式转换器和路由掩码。
///
/// 该视图只在线程启动前创建，运行期间由 fanout 线程独占可变部分；每个目标都拥有
/// 自己的转换器和暂存区，避免有状态重采样器在多个目标之间竞争。
pub(super) struct FanoutTarget {
    /// 该目标的进程输入环；索引与 `converters`、`process_enabled` 一一对应。
    process_rings: Vec<Arc<SpscRing>>,
    /// 该目标的麦克风输入环，使用目标专属的格式转换器写入。
    mic_ring: Arc<SpscRing>,
    /// 单独写往默认播放设备的物理麦试听环。
    mic_preview_ring: Option<Arc<SpscRing>>,
    /// 该目标的 SFX 槽输入环；索引与 `sfx_converters`、`sfx_enabled` 一一对应。
    sfx_rings: Vec<Arc<SpscRing>>,
    /// 每个进程槽独立的重采样/声道转换器，与 `process_rings` 对齐。
    converters: Vec<Converter>,
    /// 麦克风专用转换器，避免跨目标共享有状态转换器。
    mic_converter: Converter,
    mic_preview_converter: Option<Converter>,
    /// 每个 SFX 槽独立转换器，与 `sfx_rings` 对齐。
    sfx_converters: Vec<Converter>,
    /// 进程槽是否应写入该目标；长度与 `process_rings` 对齐。
    process_enabled: Vec<bool>,
    /// 麦克风是否应写入该目标。
    mic_enabled: bool,
    /// SFX 槽是否应写入该目标；长度与 `sfx_rings` 对齐。
    sfx_enabled: Vec<bool>,
    /// 该目标格式下允许的积压采样数（约 100 ms），fan-out 靠它退让。
    lead: usize,
    /// 转换暂存，复用于目标内的所有源，避免每块分配。
    scratch: Vec<f32>,
}

/// 从目标状态复制不可变路由配置，并为每个源槽预建转换器。
impl FanoutTarget {
    /// 构造一个目标视图；转换器带状态，因此必须在线程创建前按目标独立分配。
    pub(super) fn new(target: &TargetState, src: AudioFormat) -> Self {
        let dst = target.format;
        // 以 10 ms 为基础块；过低采样率仍保留最小块，避免环路过于频繁唤醒。
        let frames = (dst.sample_rate as usize / 100).max(64);
        Self {
            process_rings: target.process_rings.clone(),
            mic_ring: target.mic_ring.clone(),
            sfx_rings: target.sfx_rings.clone(),
            mic_preview_ring: target.mic_preview_ring.clone(),
            // 转换器数量必须与对应输入环数量一致，后续才能按槽位直接索引。
            converters: (0..target.process_rings.len())
                .map(|_| Converter::new(src, dst))
                .collect(),
            mic_converter: Converter::new(src, dst),
            mic_preview_converter: target
                .mic_preview_ring
                .as_ref()
                .map(|_| Converter::new(src, dst)),
            sfx_converters: (0..target.sfx_rings.len())
                .map(|_| Converter::new(src, dst))
                .collect(),
            process_enabled: target.process_enabled.clone(),
            mic_enabled: target.mic_enabled,
            sfx_enabled: target.sfx_enabled.clone(),
            // 允许目标环保留约 100 ms，目标落后时由 paced push 施加背压。
            lead: frames * dst.channels as usize * 10,
            scratch: Vec::with_capacity(frames * dst.channels as usize),
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// 启动单个目标的 render 线程；错误通过共享状态传播，并同步清除运行标志。
pub(super) fn spawn_target_render(
    engine: &AudioEngine,
    client: crate::audio::render::RenderClient,
    process: Vec<Arc<SpscRing>>,
    mic: Arc<SpscRing>,
    mic_preview_ring: Option<Arc<SpscRing>>,
    routed: bool,
    sfx: Vec<Arc<SpscRing>>,
    process_volumes: Vec<Arc<AtomicU32>>,
    sfx_volumes: Vec<Arc<AtomicU32>>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<EngineStatus>>,
    running: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, Error> {
    // 将共享增益句柄复制到目标线程；AtomicU32 允许 render 线程无锁读取最新值。
    let mic_volume = engine.mic_volume.clone();
    let mic_preview = mic_preview_ring.map(|ring| {
        (
            ring,
            engine.mic_preview_volume.clone(),
            engine.mic_active.clone(),
        )
    });
    let master = engine.master.clone();
    let route_peak = routed.then(|| engine.route_peak.clone());
    thread::Builder::new()
        .name("wasapi-render-target".into())
        .spawn(move || {
            // WASAPI/COM 对线程有单独的初始化要求，必须在线程入口而非创建者线程初始化。
            let _com = com::init_mta();
            run_render_loop(
                client,
                process,
                routed.then_some(mic),
                sfx,
                process_volumes,
                Some(mic_volume),
                mic_preview,
                sfx_volumes,
                master,
                route_peak,
                stop,
                move |e| {
                    // render 错误由回调统一映射为引擎状态，供控制线程观察。
                    if !routed {
                        tracing::warn!("麦克风本机试听播放设备中断：{e}");
                        return;
                    }
                    let mapped = match e {
                        Error::DeviceNotFound => EngineStatus::DeviceGone,
                        other => EngineStatus::Error(other.user_message()),
                    };
                    if let Ok(mut g) = status.lock() {
                        *g = mapped;
                    }
                    // 发布停止状态，防止上层继续把该目标视为运行中。
                    running.store(false, Ordering::Release);
                },
            );
        })
        .map_err(|_| Error::RenderInit("spawn output target"))
}

/// 从各 canonical 源环取出固定块，按目标路由转换后以 paced push 写入目标环。
pub(super) fn run_fanout(
    source: Vec<Arc<SpscRing>>,
    mic: Arc<SpscRing>,
    sfx: Vec<Arc<SpscRing>>,
    mic_active: Arc<AtomicBool>,
    mic_preview_volume: Arc<AtomicU32>,
    mut targets: Vec<FanoutTarget>,
    stop: Arc<AtomicBool>,
    format: AudioFormat,
) {
    // 所有源先在 canonical 格式按统一块大小消费，再分别转换到目标格式。
    let frames = (format.sample_rate as usize / 100).max(64);
    let samples = frames * format.channels as usize;
    // 每个源槽预分配一个块；循环内只复用这些缓冲，避免实时线程分配。
    let mut blocks = vec![vec![0.0; samples]; source.len()];
    let mut mic_block = vec![0.0; samples];
    let mut sfx_blocks = vec![vec![0.0; samples]; sfx.len()];
    // stop 由引擎释放路径置位；循环退出后 fanout 句柄才能安全 join。
    while !stop.load(Ordering::Acquire) {
        // 每轮先完整消费源环；未填满的尾部清零，避免旧块数据重复播放。
        for (ring, block) in source.iter().zip(blocks.iter_mut()) {
            let n = ring.pop(block);
            block[n..].fill(0.0);
        }
        let mic_n = mic.pop(&mut mic_block);
        mic_block[mic_n..].fill(0.0);
        for (ring, block) in sfx.iter().zip(sfx_blocks.iter_mut()) {
            let n = ring.pop(block);
            block[n..].fill(0.0);
        }
        // 同一源块可复制到多个目标，但每个目标拥有独立环和转换器。
        for target in &mut targets {
            // 解构目标视图后按槽位处理，保持环、转换器和启用掩码的索引对应关系。
            let FanoutTarget {
                process_rings,
                mic_ring,
                mic_preview_ring,
                sfx_rings,
                converters,
                mic_converter,
                mic_preview_converter,
                sfx_converters,
                process_enabled,
                mic_enabled,
                sfx_enabled,
                lead,
                scratch,
            } = target;
            // 只有启用槽才写入目标环；paced push 在目标落后时让出，避免无限积压。
            for (index, block) in blocks.iter().enumerate() {
                if process_enabled.get(index).copied().unwrap_or(false) {
                    // 转换器将 canonical 块写入目标格式的共享暂存，再交给目标专属环。
                    converters[index].convert_f32(block, frames, scratch);
                    process_rings[index].push_paced(scratch, *lead, &stop);
                }
            }
            // 麦克风沿同一节拍单独路由，保持与进程槽相同的格式转换边界。
            if *mic_enabled {
                mic_converter.convert_f32(&mic_block, frames, scratch);
                mic_ring.push_paced(scratch, *lead, &stop);
            }
            // 与转发麦克风共用唯一 canonical 源块；独立转换器和环不影响转发增益。
            if let (Some(ring), Some(converter)) = (mic_preview_ring, mic_preview_converter)
                && mic_active.load(Ordering::Acquire)
                && f32::from_bits(mic_preview_volume.load(Ordering::Relaxed)) > 0.0
            {
                converter.convert_f32(&mic_block, frames, scratch);
                ring.push_paced(scratch, *lead, &stop);
            }
            // SFX 槽逐个分发，停止或暂停某槽不会清空其他槽的环。
            for (index, block) in sfx_blocks.iter().enumerate() {
                if sfx_enabled.get(index).copied().unwrap_or(false) {
                    sfx_converters[index].convert_f32(block, frames, scratch);
                    sfx_rings[index].push_paced(scratch, *lead, &stop);
                }
            }
        }
    }
}
