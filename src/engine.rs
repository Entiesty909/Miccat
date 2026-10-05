//! 音频引擎：多进程源 + 物理麦 + SFX，fan-out 到多个独立输出设备。

mod fanout;
mod monitor;
mod output;
mod playback;
mod status;
mod volume;
pub(crate) use playback::PlaybackTarget;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::audio::buffer::SpscRing;
use crate::audio::capture::{open_mic_capture, open_process_capture, run_capture_loop};
use crate::audio::com;
use crate::audio::format::{AudioFormat, Converter, SampleKind};
use crate::error::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineStatus {
    Idle,
    Running,
    ProcessExited,
    DeviceGone,
    Error(String),
}
#[derive(Clone, Copy)]
pub(crate) struct SourceOutputRoutes {
    /// 在打开的 render 列表中承接进程源的下标。
    pub(crate) process: usize,
    /// 在打开的 render 列表中承接音频文件源的下标。
    pub(crate) file: usize,
    /// 在打开的 render 列表中承接麦克风源的下标。
    pub(crate) mic: usize,
}

pub(super) struct TargetState {
    pub(super) format: AudioFormat,
    pub(super) process_rings: Vec<Arc<SpscRing>>,
    pub(super) mic_ring: Arc<SpscRing>,
    pub(super) sfx_ring: Arc<SpscRing>,
    pub(super) process_enabled: bool,
    pub(super) mic_enabled: bool,
    pub(super) sfx_enabled: bool,
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
    thread: JoinHandle<()>,
}

#[derive(Default)]
struct SourceThreads {
    process: Vec<ProcessThread>,
    mic_stop: Option<Arc<AtomicBool>>,
    mic: Option<JoinHandle<()>>,
    sfx_tx: Option<std::sync::mpsc::Sender<crate::audio::sfx::SfxCommand>>,
    sfx_stop: Option<Arc<AtomicBool>>,
    sfx: Option<JoinHandle<()>>,
}

pub struct AudioEngine {
    volume: Arc<AtomicU32>,
    mic_volume: Arc<AtomicU32>,
    sfx_volume: Arc<AtomicU32>,
    master: Arc<AtomicU32>,
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
        Self {
            volume: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            mic_volume: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            sfx_volume: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            master: Arc::new(AtomicU32::new(1.0f32.to_bits())),
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
        let thread_stop = stop.clone();
        let status = self.status.clone();
        let thread = thread::Builder::new()
            .name(format!("wasapi-capture-{slot}"))
            .spawn(move || {
                let _com = com::init_mta();
                run_capture_loop(capture, ring, converter, thread_stop, |err| {
                    let mapped = match err {
                        Error::ProcessNotFound(_) => EngineStatus::ProcessExited,
                        Error::DeviceNotFound => EngineStatus::DeviceGone,
                        other => EngineStatus::Error(other.user_message()),
                    };
                    if let Ok(mut g) = status.lock() {
                        *g = mapped;
                    }
                });
            })
            .map_err(|_| Error::CaptureInit("spawn capture thread"))?;
        if let Ok(mut sources) = self.sources.lock() {
            sources.process.push(ProcessThread { stop, thread });
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
        let thread_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("wasapi-mic".into())
            .spawn(move || {
                let _com = com::init_mta();
                run_capture_loop(mic, ring, conv, thread_stop, |_| {});
            })
            .map_err(|_| Error::CaptureInit("spawn mic thread"))?;
        if let Ok(mut s) = self.sources.lock() {
            s.mic_stop = Some(stop);
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
    fn set_status(&self, s: EngineStatus) {
        if let Ok(mut g) = self.status.lock() {
            *g = s;
        }
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
