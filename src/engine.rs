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
/// 进程源可占用的固定捕获槽位数；槽位索引同时用于独立增益和环形缓冲。
pub(crate) const PROCESS_SLOTS: usize = 4;
/// 路由文件源可占用的固定槽位数；播放试听使用额外的 `PLAYBACK_SLOT`。
pub(crate) const FILE_SOURCE_SLOTS: usize = 4;
/// 音频库独立播放流在 SFX mixer 中使用的保留槽位。
pub(crate) const PLAYBACK_SLOT: usize = FILE_SOURCE_SLOTS;
/// SFX mixer 的总槽位数，包含路由文件槽和独立播放槽。
pub(crate) const SFX_SLOTS: usize = FILE_SOURCE_SLOTS + 1;

/// 引擎对外暴露的权威生命周期状态；UI 查询应以此快照为准。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineStatus {
    /// 尚未建立输出或所有源均已停止。
    Idle,
    /// 输出、fanout 和源线程已建立并可接收音频。
    Running,
    /// 捕获进程已退出，后续音频不再来自该进程。
    ProcessExited,
    /// render 或 capture 设备已断开或不可用。
    DeviceGone,
    /// 发生无法归类的引擎错误，字符串供 UI 展示。
    Error(String),
}
/// 一组把每类源映射到 render 目标索引的路由快照。
#[derive(Clone)]
pub(crate) struct SourceOutputRoutes {
    /// 每个进程槽承接的 render 下标。
    pub(crate) process: Vec<usize>,
    /// 每个音频文件槽承接的 render 下标。
    pub(crate) file: Vec<usize>,
    /// 麦克风源承接的 render 下标。
    pub(crate) mic: usize,
}

/// 单个 render 目标在 fanout 中使用的格式、输入环和启用掩码。
pub(super) struct TargetState {
    /// 该物理目标接受的采样格式。
    pub(super) format: AudioFormat,
    /// 每个进程槽对应的目标格式 SPSC 环。
    pub(super) process_rings: Vec<Arc<SpscRing>>,
    /// 物理麦克风对应的目标格式 SPSC 环。
    pub(super) mic_ring: Arc<SpscRing>,
    /// 每个 SFX 槽对应的目标格式 SPSC 环。
    pub(super) sfx_rings: Vec<Arc<SpscRing>>,
    /// 进程槽是否路由到该目标；长度与 `process_rings` 对齐。
    pub(super) process_enabled: Vec<bool>,
    /// 麦克风是否路由到该目标。
    pub(super) mic_enabled: bool,
    /// SFX 槽是否路由到该目标；长度与 `sfx_rings` 对齐。
    pub(super) sfx_enabled: Vec<bool>,
    /// 目标 render 线程；停止时必须先发停止信号再 join。
    pub(super) render: JoinHandle<()>,
}

/// 所有输出线程和源环的共享生命周期；替换输出时整体取出并回收。
struct OutputState {
    /// fanout 与目标 render 线程共同观察的停止标志。
    stop: Arc<AtomicBool>,
    /// canonical 混音格式下的进程源环。
    process_rings: Vec<Arc<SpscRing>>,
    /// canonical 混音格式下的麦克风源环。
    mic_ring: Arc<SpscRing>,
    /// fanout 读取源环时使用的统一格式。
    render_format: AudioFormat,
    /// 打开第一个设备时获得的 WASAPI 混音格式。
    mix_format: windows::Win32::Media::Audio::WAVEFORMATEX,
    /// 在源环与目标环之间分发数据的线程。
    fanout: JoinHandle<()>,
    /// 各物理目标的目标格式环、启用掩码及 render 线程。
    targets: Vec<TargetState>,
}

/// 一个进程捕获线程及其停止、峰值遥测状态。
struct ProcessThread {
    /// 捕获循环使用的停止标志。
    stop: Arc<AtomicBool>,
    /// 捕获循环写入、UI 读取后交换清零的峰值。
    peak: Arc<AtomicU32>,
    /// 线程句柄；停止源时必须 join 以回收 COM/设备资源。
    thread: JoinHandle<()>,
}

/// 进程、麦克风和 SFX 源线程的集中所有权表。
#[derive(Default)]
struct SourceThreads {
    /// 当前活动的进程捕获线程，顺序对应已分配的进程槽。
    process: Vec<ProcessThread>,
    /// 麦克风捕获线程的停止标志。
    mic_stop: Option<Arc<AtomicBool>>,
    /// 麦克风捕获线程写入的峰值。
    mic_peak: Option<Arc<AtomicU32>>,
    /// 麦克风捕获线程句柄。
    mic: Option<JoinHandle<()>>,
    /// 向 SFX mixer 发送播放、停止和暂停命令的发送端。
    sfx_tx: Option<std::sync::mpsc::Sender<crate::audio::sfx::SfxCommand>>,
    /// SFX mixer 的停止标志。
    sfx_stop: Option<Arc<AtomicBool>>,
    /// SFX mixer 写入的最近块峰值。
    sfx_peak: Option<Arc<AtomicU32>>,
    /// 每个 SFX 槽状态的原子编码，避免读取线程参与混音。
    sfx_states: Option<Arc<Vec<AtomicU8>>>,
    /// 每个 SFX 槽已播放位置，单位为 100 纳秒；混音线程无锁发布给 UI。
    sfx_positions: Option<Arc<Vec<std::sync::atomic::AtomicU64>>>,
    /// SFX mixer 线程句柄。
    sfx: Option<JoinHandle<()>>,
}

/// 音频引擎的唯一状态持有者；输出、源线程和播放状态均从此处派生。
pub struct AudioEngine {
    /// 每个进程槽独立的线性增益原子，render 线程无锁读取。
    process_volumes: Vec<Arc<AtomicU32>>,
    /// 每个 SFX 槽独立的线性增益原子。
    sfx_volumes: Vec<Arc<AtomicU32>>,
    /// 兼容旧 API 的进程默认增益。
    volume: Arc<AtomicU32>,
    /// 物理麦克风源增益。
    mic_volume: Arc<AtomicU32>,
    /// 兼容旧 API 的 SFX 默认增益。
    sfx_volume: Arc<AtomicU32>,
    /// 最终输出总线增益。
    master: Arc<AtomicU32>,
    /// 最近渲染块的总线峰值，读取时交换为零。
    route_peak: Arc<AtomicU32>,
    /// 输出是否仍在运行；跨线程发布引擎生命周期。
    running: Arc<AtomicBool>,
    /// 引擎状态的唯一权威快照。
    status: Arc<Mutex<EngineStatus>>,
    /// 本机试听输出的共享句柄。
    monitor: monitor::SharedMonitor,
    /// 文件播放状态及取消令牌。
    playback: Arc<Mutex<playback::PlaybackControl>>,
    /// 路由文件槽最近播放请求的代次，后台解码完成时据此丢弃旧 seek 请求。
    file_source_generations: Vec<Arc<Mutex<u64>>>,
    /// 当前输出图；取出后才能安全停止并替换所有线程。
    output: Mutex<Option<OutputState>>,
    /// 当前各类源线程及其命令通道。
    sources: Mutex<SourceThreads>,
}

/// 默认创建一个尚未启动的引擎。
impl Default for AudioEngine {
    /// 使用与 `AudioEngine::new` 相同的空闲状态构造实例。
    fn default() -> Self {
        Self::new()
    }
}

/// 构造全部共享状态；原子增益默认值为 1，线程尚未启动。
impl AudioEngine {
    /// 分配固定槽位及其无锁增益，并建立空的输出、源和试听状态。
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
            file_source_generations: (0..FILE_SOURCE_SLOTS)
                .map(|_| Arc::new(Mutex::new(0)))
                .collect(),
        }
    }

    /// 替换默认进程源；先停止旧捕获线程，避免同一槽位残留生产者。
    pub fn set_process_source(&self, pid: u32) -> Result<(), Error> {
        self.stop_process_source();
        self.add_process_source(pid, 0)
    }

    /// 运行中热切换物理麦：停旧开新，复用已建立的输出。
    pub fn replace_mic_source(&self, mic_id: Option<&str>) -> Result<(), Error> {
        self.set_mic_source(mic_id)
    }
    /// 在指定槽位创建进程捕获线程，并把捕获错误映射到权威引擎状态。
    pub fn add_process_source(&self, pid: u32, slot: usize) -> Result<(), Error> {
        // 调用线程先加入 MTA，保证打开进程环回设备的 COM 前提成立。
        let _com = com::init_mta()?;
        // 槽位索引同时选择唯一生产环；输出未建立时不得启动孤立捕获线程。
        let (rings, _, format, mix) = self.output_routing()?;
        let ring = rings
            .get(slot)
            .cloned()
            .ok_or(Error::CaptureInit("process source slots full"))?;
        // 捕获格式转换到 render 的 canonical F32 格式，避免实时线程再分配转换器。
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
        // 每个捕获线程独立初始化 COM，并只通过原子停止标志与引擎协作。
        let thread = thread::Builder::new()
            .name(format!("wasapi-capture-{slot}"))
            .spawn(move || {
                let _com = com::init_mta();
                // 捕获层的底层错误在这里归一化，UI 只依赖 EngineStatus 而不解析 HRESULT。
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
        // 线程句柄和停止令牌交给引擎集中持有，停止时才能先发信号再 join。
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
    /// 替换物理麦克风源；停止旧线程后将新流接入现有输出环。
    pub fn set_mic_source(&self, mic_id: Option<&str>) -> Result<(), Error> {
        // 先关闭旧生产者，确保替换期间不会有两个线程写入同一麦克风环。
        let _com = com::init_mta()?;
        self.stop_mic_source();
        let Some(id) = mic_id else {
            // None 表示明确卸载麦克风，保留其他源和输出图不变。
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
        // 麦克风捕获线程拥有自己的 COM apartment；停止只由原子标志发布。
        let thread = thread::Builder::new()
            .name("wasapi-mic".into())
            .spawn(move || {
                let _com = com::init_mta();
                run_capture_loop(mic, ring, conv, thread_stop, Some(thread_peak), |_| {});
            })
            .map_err(|_| Error::CaptureInit("spawn mic thread"))?;
        // 保存句柄与遥测原子，供停止和 UI 峰值读取路径使用。
        if let Ok(mut s) = self.sources.lock() {
            s.mic_stop = Some(stop);
            s.mic_peak = Some(peak);
            s.mic = Some(thread);
        }
        Ok(())
    }
    /// 停止所有源和输出线程，按依赖顺序释放设备并恢复空闲状态。
    pub fn stop(&self) {
        // 先停止会读写输出环的源线程，再停止 fanout/render，保证 join 时没有新生产者。
        self.stop_file();
        self.stop_process_source();
        self.stop_mic_source();
        self.stop_sfx_source();
        // 试听引擎持有系统默认播放设备；不释放会让随后的转发 Initialize
        // 撞上 AUDCLNT_E_DEVICE_IN_USE（0x8889000A）。
        monitor::release(&self.monitor);
        if let Some(o) = self.output.lock().ok().and_then(|mut g| g.take()) {
            // 输出图取出后先通知 fanout，再等待它和各目标 render 退出，避免环仍被读取。
            o.stop.store(true, Ordering::Release);
            let _ = o.fanout.join();
            for t in o.targets {
                let _ = t.render.join();
            }
        }
        // 发布 false 后，状态查询不会再把已释放的输出图报告为运行中。
        self.running.store(false, Ordering::Release);
        if matches!(self.status(), EngineStatus::Running) {
            self.set_status(EngineStatus::Idle);
        }
    }
    #[allow(clippy::type_complexity)]
    /// 返回当前输出图的共享输入环与格式；没有打开输出时传播可读错误。
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
        // 源线程只允许在输出图建立后启动，因此这里返回当前图的共享环和格式。
        let o = self
            .output
            .lock()
            .map_err(|_| Error::RenderInit("output lock poisoned"))?;
        let o = o.as_ref().ok_or(Error::RenderInit("output is closed"))?;
        Ok((
            o.process_rings.clone(),
            o.mic_ring.clone(),
            o.render_format,
            o.mix_format,
        ))
    }
    /// 取出所有进程线程后逐一发停止信号并 join，确保旧生产者已退出。
    fn stop_process_source(&self) {
        // 先从共享表取走句柄，释放 sources 锁后再等待，避免线程退出路径反向取锁。
        let v = self
            .sources
            .lock()
            .ok()
            .map(|mut s| std::mem::take(&mut s.process))
            .unwrap_or_default();
        for p in v {
            // Release 发布停止请求，join 确认该槽位不再向环写入。
            p.stop.store(true, Ordering::Release);
            let _ = p.thread.join();
        }
    }
    /// 停止麦克风线程；停止标志与句柄分开取出，避免持锁等待线程退出。
    fn stop_mic_source(&self) {
        // 停止令牌和句柄一起移出共享表，再在无锁状态下等待线程结束。
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
    /// 清除每个 SFX 槽状态，再停止并 join mixer，防止旧流继续被报告为活动。
    fn stop_sfx_source(&self) {
        if let Ok(sources) = self.sources.lock()
            && let Some(states) = &sources.sfx_states
        {
            // 先清零槽位状态，防止 mixer 退出后 UI 仍观察到旧流处于活动态。
            for state in states.iter() {
                state.store(0, Ordering::Relaxed);
            }
        }
        if let Ok(sources) = self.sources.lock()
            && let Some(positions) = &sources.sfx_positions
        {
            // 输出图释放后进度必须回到零，避免新路由首帧显示旧文件位置。
            for position in positions.iter() {
                position.store(0, Ordering::Relaxed);
            }
        }
        // 取出停止令牌和句柄后再 join，避免在 sources 锁内等待 mixer 回收。
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
        if let Ok(mut sources) = self.sources.lock() {
            sources.sfx_tx = None;
            sources.sfx_states = None;
            sources.sfx_positions = None;
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
    /// 更新唯一权威状态；锁失败时保持原状态，避免在错误路径中 panic。
    fn set_status(&self, s: EngineStatus) {
        // 状态写入集中在此处，查询方只需读取同一快照而不推断线程是否存活。
        if let Ok(mut g) = self.status.lock() {
            *g = s;
        }
    }
    /// 读取最近渲染块的总线峰值并清零采样窗口。
    pub fn route_peak(&self) -> f32 {
        f32::from_bits(self.route_peak.swap(0.0f32.to_bits(), Ordering::Relaxed)).clamp(0.0, 1.0)
    }
}

/// 释放引擎拥有的所有线程、设备和播放资源。
impl Drop for AudioEngine {
    /// 释放时复用统一停止路径，保证线程和设备按依赖顺序退出。
    fn drop(&mut self) {
        self.stop();
    }
}
