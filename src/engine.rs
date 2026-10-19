//! 音频引擎：多进程源 + 物理麦 + SFX，fan-out 到多个独立输出设备。

mod fanout;
mod monitor;
mod output;
mod playback;
mod status;
mod volume;
pub(crate) use playback::PlaybackTarget;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
/// 单个路由音频源的混音状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FileSourceState {
    /// 没有活动流。
    #[default]
    Stopped,
    /// 槽位正在输出音频。
    Playing,
    /// 槽位暂停，解码位置保留。
    Paused,
    /// 流已自然播放完毕。
    Finished,
}

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::audio::buffer::SpscRing;
use crate::audio::capture::{open_mic_capture, open_process_capture, run_capture_loop};
use crate::audio::com;
use crate::audio::format::{AudioFormat, Converter, SampleKind};
use crate::error::Error;
pub(crate) const PROCESS_SLOTS: usize = 4;
pub(crate) const FILE_SOURCE_SLOTS: usize = 4;
pub(crate) const PLAYBACK_SLOT: usize = FILE_SOURCE_SLOTS;
pub(crate) const SFX_SLOTS: usize = FILE_SOURCE_SLOTS + 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineStatus {
    Idle,
    Running,
    ProcessExited,
    DeviceGone,
    Error(String),
}
#[derive(Clone)]
pub(crate) struct SourceOutputRoutes {
    /// 每个进程槽承接的 render 下标。
    pub(crate) process: Vec<usize>,
    /// 每个音频文件槽承接的 render 下标。
    pub(crate) file: Vec<usize>,
    /// 麦克风源承接的 render 下标。
    pub(crate) mic: usize,
}

pub(super) struct TargetState {
    pub(super) format: AudioFormat,
    pub(super) process_rings: Vec<Arc<SpscRing>>,
    pub(super) mic_ring: Arc<SpscRing>,
    pub(super) sfx_rings: Vec<Arc<SpscRing>>,
    pub(super) process_enabled: Vec<bool>,
    pub(super) mic_enabled: bool,
    pub(super) sfx_enabled: Vec<bool>,
    pub(super) render: JoinHandle<()>,
}

struct OutputState {
    stop: Arc<AtomicBool>,
    process_rings: Vec<Arc<SpscRing>>,
    mic_ring: Arc<SpscRing>,
    render_format: AudioFormat,
    mix_format: windows::Win32::Media::Audio::WAVEFORMATEX,
    fanout: JoinHandle<()>,
    targets: Vec<TargetState>,
}

struct ProcessThread {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU32>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
struct SourceThreads {
    process: Vec<ProcessThread>,
    mic_stop: Option<Arc<AtomicBool>>,
    mic_peak: Option<Arc<AtomicU32>>,
    mic: Option<JoinHandle<()>>,
    sfx_tx: Option<std::sync::mpsc::Sender<crate::audio::sfx::SfxCommand>>,
    sfx_stop: Option<Arc<AtomicBool>>,
    sfx_peak: Option<Arc<AtomicU32>>,
    sfx_states: Option<Arc<Vec<AtomicU8>>>,
    sfx: Option<JoinHandle<()>>,
}

pub struct AudioEngine {
    process_volumes: Vec<Arc<AtomicU32>>,
    sfx_volumes: Vec<Arc<AtomicU32>>,
    volume: Arc<AtomicU32>,
    mic_volume: Arc<AtomicU32>,
    sfx_volume: Arc<AtomicU32>,
    master: Arc<AtomicU32>,
    route_peak: Arc<AtomicU32>,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<EngineStatus>>,
    monitor: monitor::SharedMonitor,
    playback: Arc<Mutex<playback::PlaybackControl>>,
    output: Mutex<Option<OutputState>>,
    sources: Mutex<SourceThreads>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        let default_gain = || Arc::new(AtomicU32::new(1.0f32.to_bits()));
        Self {
            process_volumes: (0..PROCESS_SLOTS).map(|_| default_gain()).collect(),
            sfx_volumes: (0..SFX_SLOTS).map(|_| default_gain()).collect(),
            volume: default_gain(),
            mic_volume: default_gain(),
            sfx_volume: default_gain(),
            master: default_gain(),
            route_peak: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            running: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(EngineStatus::Idle)),
            output: Mutex::new(None),
            sources: Mutex::new(SourceThreads::default()),
            monitor: monitor::shared(),
            playback: playback::shared(),
        }
    }

    pub fn set_process_source(&self, pid: u32) -> Result<(), Error> {
        self.stop_process_source();
        self.add_process_source(pid, 0)
    }

    /// 运行中热切换物理麦：停旧开新，复用已建立的输出。
    pub fn replace_mic_source(&self, mic_id: Option<&str>) -> Result<(), Error> {
        self.set_mic_source(mic_id)
    }
    pub fn add_process_source(&self, pid: u32, slot: usize) -> Result<(), Error> {
        let _com = com::init_mta()?;
        let (rings, _, format, mix) = self.output_routing()?;
        let ring = rings
            .get(slot)
            .cloned()
            .ok_or(Error::CaptureInit("process source slots full"))?;
        let capture = open_process_capture(pid, Some(&mix))?;
        let converter = Converter::new(
            capture.format,
            AudioFormat {
                sample_rate: format.sample_rate,
                channels: format.channels,
                kind: SampleKind::F32,
            },
        );
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let thread_stop = stop.clone();
        let thread_peak = peak.clone();
        let status = self.status.clone();
        let thread = thread::Builder::new()
            .name(format!("wasapi-capture-{slot}"))
            .spawn(move || {
                let _com = com::init_mta();
                run_capture_loop(
                    capture,
                    ring,
                    converter,
                    thread_stop,
                    Some(thread_peak),
                    |err| {
                        let mapped = match err {
                            Error::ProcessNotFound(_) => EngineStatus::ProcessExited,
                            Error::DeviceNotFound => EngineStatus::DeviceGone,
                            other => EngineStatus::Error(other.user_message()),
                        };
                        if let Ok(mut g) = status.lock() {
                            *g = mapped;
                        }
                    },
                );
            })
            .map_err(|_| Error::CaptureInit("spawn capture thread"))?;
        if let Ok(mut sources) = self.sources.lock() {
            sources.process.push(ProcessThread { stop, peak, thread });
        }
        Ok(())
    }
    /// 按当前启用列表重建进程捕获线程，保持已打开的 render 输出。
    pub fn replace_process_sources(&self, pids: &[u32]) -> Result<(), Error> {
        self.stop_process_source();
        for (slot, pid) in pids.iter().copied().enumerate() {
            self.add_process_source(pid, slot)?;
        }
        Ok(())
    }
    pub fn set_mic_source(&self, mic_id: Option<&str>) -> Result<(), Error> {
        let _com = com::init_mta()?;
        self.stop_mic_source();
        let Some(id) = mic_id else {
            return Ok(());
        };
        let (_, ring, format, mix) = self.output_routing()?;
        let mic = open_mic_capture(id, Some(&mix))?;
        let conv = Converter::new(
            mic.format,
            AudioFormat {
                sample_rate: format.sample_rate,
                channels: format.channels,
                kind: SampleKind::F32,
            },
        );
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let thread_stop = stop.clone();
        let thread_peak = peak.clone();
        let thread = thread::Builder::new()
            .name("wasapi-mic".into())
            .spawn(move || {
                let _com = com::init_mta();
                run_capture_loop(mic, ring, conv, thread_stop, Some(thread_peak), |_| {});
            })
            .map_err(|_| Error::CaptureInit("spawn mic thread"))?;
        if let Ok(mut s) = self.sources.lock() {
            s.mic_stop = Some(stop);
            s.mic_peak = Some(peak);
            s.mic = Some(thread);
        }
        Ok(())
    }
    pub fn stop(&self) {
        self.stop_file();
        self.stop_process_source();
        self.stop_mic_source();
        self.stop_sfx_source();
        // 试听引擎持有系统默认播放设备；不释放会让随后的转发 Initialize
        // 撞上 AUDCLNT_E_DEVICE_IN_USE（0x8889000A）。
        monitor::release(&self.monitor);
        if let Some(o) = self.output.lock().ok().and_then(|mut g| g.take()) {
            o.stop.store(true, Ordering::Release);
            let _ = o.fanout.join();
            for t in o.targets {
                let _ = t.render.join();
            }
        }
        self.running.store(false, Ordering::Release);
        if matches!(self.status(), EngineStatus::Running) {
            self.set_status(EngineStatus::Idle);
        }
    }
    #[allow(clippy::type_complexity)]
    fn output_routing(
        &self,
    ) -> Result<
        (
            Vec<Arc<SpscRing>>,
            Arc<SpscRing>,
            AudioFormat,
            windows::Win32::Media::Audio::WAVEFORMATEX,
        ),
        Error,
    > {
        let o = self
            .output
            .lock()
            .map_err(|_| Error::RenderInit("output lock poisoned"))?;
        let o = o.as_ref().ok_or(Error::RenderInit("output is closed"))?;
        Ok((
            o.process_rings.clone(),
            o.mic_ring.clone(),
            o.render_format,
            copy_wave(&o.mix_format),
        ))
    }
    fn stop_process_source(&self) {
        let v = self
            .sources
            .lock()
            .ok()
            .map(|mut s| std::mem::take(&mut s.process))
            .unwrap_or_default();
        for p in v {
            p.stop.store(true, Ordering::Release);
            let _ = p.thread.join();
        }
    }
    fn stop_mic_source(&self) {
        let (s, t) = self
            .sources
            .lock()
            .ok()
            .map(|mut s| (s.mic_stop.take(), s.mic.take()))
            .unwrap_or((None, None));
        if let Some(s) = s {
            s.store(true, Ordering::Release)
        }
        if let Some(t) = t {
            let _ = t.join();
        }
    }
    fn stop_sfx_source(&self) {
        if let Ok(sources) = self.sources.lock()
            && let Some(states) = &sources.sfx_states
        {
            for state in states.iter() {
                state.store(0, Ordering::Relaxed);
            }
        }
        let (s, t) = self
            .sources
            .lock()
            .ok()
            .map(|mut s| (s.sfx_stop.take(), s.sfx.take()))
            .unwrap_or((None, None));
        if let Some(s) = s {
            s.store(true, Ordering::Release)
        }
        if let Some(t) = t {
            let _ = t.join();
        }
        if let Ok(mut s) = self.sources.lock() {
            s.sfx_tx = None;
        }
    }
    /// 读取指定进程源的实时峰值并做一次衰减。
    pub fn process_peak(&self, slot: usize) -> f32 {
        let Ok(sources) = self.sources.lock() else {
            return 0.0;
        };
        let Some(thread) = sources.process.get(slot) else {
            return 0.0;
        };
        let value = f32::from_bits(thread.peak.swap(0.0f32.to_bits(), Ordering::Relaxed));
        value.clamp(0.0, 1.0)
    }

    /// 读取物理麦克风的实时峰值并清零采样窗口。
    pub fn mic_peak(&self) -> f32 {
        let Ok(sources) = self.sources.lock() else {
            return 0.0;
        };
        sources
            .mic_peak
            .as_ref()
            .map(|peak| f32::from_bits(peak.swap(0.0f32.to_bits(), Ordering::Relaxed)))
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
    /// 读取 SFX mixer 最近音频块的峰值并清零窗口。
    pub fn sfx_peak(&self) -> f32 {
        let Ok(sources) = self.sources.lock() else {
            return 0.0;
        };
        sources
            .sfx_peak
            .as_ref()
            .map(|peak| f32::from_bits(peak.swap(0.0f32.to_bits(), Ordering::Relaxed)))
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
    fn set_status(&self, s: EngineStatus) {
        if let Ok(mut g) = self.status.lock() {
            *g = s;
        }
    }
    /// 读取最近渲染块的总线峰值并清零采样窗口。
    pub fn route_peak(&self) -> f32 {
        f32::from_bits(self.route_peak.swap(0.0f32.to_bits(), Ordering::Relaxed)).clamp(0.0, 1.0)
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}
fn copy_wave(
    src: &windows::Win32::Media::Audio::WAVEFORMATEX,
) -> windows::Win32::Media::Audio::WAVEFORMATEX {
    *src
}
