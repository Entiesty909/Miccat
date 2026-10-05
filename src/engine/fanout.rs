//! 多输出 fan-out、格式转换和目标 render 线程工厂。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::audio::buffer::SpscRing;
use crate::audio::com;
use crate::audio::format::{AudioFormat, Converter};
use crate::audio::render::run_render_loop;
use crate::error::Error;

use super::{AudioEngine, EngineStatus, TargetState};

pub(super) struct FanoutTarget {
    process_rings: Vec<Arc<SpscRing>>,
    mic_ring: Arc<SpscRing>,
    sfx_ring: Arc<SpscRing>,
    converters: Vec<Converter>,
    mic_converter: Converter,
    sfx_converter: Converter,
    process_enabled: bool,
    mic_enabled: bool,
    sfx_enabled: bool,
    /// 该目标格式下允许的积压采样数（约 100 ms），fan-out 靠它退让。
    lead: usize,
    /// 转换暂存，避免每块分配。
    scratch: Vec<f32>,
}

impl FanoutTarget {
    pub(super) fn new(target: &TargetState, src: AudioFormat) -> Self {
        let dst = target.format;
        let frames = (dst.sample_rate as usize / 100).max(64);
        Self {
            process_rings: target.process_rings.clone(),
            mic_ring: target.mic_ring.clone(),
            sfx_ring: target.sfx_ring.clone(),
            converters: (0..target.process_rings.len())
                .map(|_| Converter::new(src, dst))
                .collect(),
            mic_converter: Converter::new(src, dst),
            sfx_converter: Converter::new(src, dst),
            process_enabled: target.process_enabled,
            mic_enabled: target.mic_enabled,
            sfx_enabled: target.sfx_enabled,
            lead: frames * dst.channels as usize * 10,
            scratch: Vec::with_capacity(frames * dst.channels as usize),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_target_render(
    engine: &AudioEngine,
    client: crate::audio::render::RenderClient,
    process: Vec<Arc<SpscRing>>,
    mic: Arc<SpscRing>,
    sfx: Arc<SpscRing>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<EngineStatus>>,
    running: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, Error> {
    let volume = engine.volume.clone();
    let mic_volume = engine.mic_volume.clone();
    let sfx_volume = engine.sfx_volume.clone();
    let master = engine.master.clone();
    thread::Builder::new()
        .name("wasapi-render-target".into())
        .spawn(move || {
            let _com = com::init_mta();
            run_render_loop(
                client,
                process,
                Some(mic),
                Some(sfx),
                volume,
                mic_volume,
                sfx_volume,
                master,
                stop,
                move |e| {
                    let mapped = match e {
                        Error::DeviceNotFound => EngineStatus::DeviceGone,
                        other => EngineStatus::Error(other.user_message()),
                    };
                    if let Ok(mut g) = status.lock() {
                        *g = mapped;
                    }
                    running.store(false, Ordering::Release);
                },
            );
        })
        .map_err(|_| Error::RenderInit("spawn output target"))
}

pub(super) fn run_fanout(
    source: Vec<Arc<SpscRing>>,
    mic: Arc<SpscRing>,
    sfx: Arc<SpscRing>,
    mut targets: Vec<FanoutTarget>,
    stop: Arc<AtomicBool>,
    format: AudioFormat,
) {
    let frames = (format.sample_rate as usize / 100).max(64);
    let samples = frames * format.channels as usize;
    let mut blocks = vec![vec![0.0; samples]; source.len()];
    let mut mic_block = vec![0.0; samples];
    let mut sfx_block = vec![0.0; samples];
    while !stop.load(Ordering::Acquire) {
        for (ring, block) in source.iter().zip(blocks.iter_mut()) {
            let n = ring.pop(block);
            block[n..].fill(0.0);
        }
        let mic_n = mic.pop(&mut mic_block);
        mic_block[mic_n..].fill(0.0);
        let sfx_n = sfx.pop(&mut sfx_block);
        sfx_block[sfx_n..].fill(0.0);
        for target in &mut targets {
            let FanoutTarget {
                process_rings,
                mic_ring,
                sfx_ring,
                converters,
                mic_converter,
                sfx_converter,
                process_enabled,
                mic_enabled,
                sfx_enabled,
                lead,
                scratch,
            } = target;
            if *process_enabled {
                for (index, block) in blocks.iter().enumerate() {
                    converters[index].convert_f32(block, frames, scratch);
                    process_rings[index].push_paced(scratch, *lead, &stop);
                }
            }
            if *mic_enabled {
                mic_converter.convert_f32(&mic_block, frames, scratch);
                mic_ring.push_paced(scratch, *lead, &stop);
            }
            if *sfx_enabled {
                sfx_converter.convert_f32(&sfx_block, frames, scratch);
                sfx_ring.push_paced(scratch, *lead, &stop);
            }
        }
    }
}
