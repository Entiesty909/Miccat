//! 本机试听输出的引擎门面；UI 不直接持有 PreviewEngine。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use crate::audio::decode::MediaInfo;
use crate::audio::device;
use crate::audio::preview::PreviewEngine;
use crate::audio::sfx::StreamChunk;
use crate::error::Error;

// 试听输出的当前状态由下方共享结构保存。
#[derive(Default)]
/// 试听设备的唯一所有权容器；引擎对象只通过此锁访问 PreviewEngine。
pub(crate) struct MonitorState {
    /// 已打开的试听引擎；`None` 表示试听关闭。
    engine: Option<PreviewEngine>,
    /// `engine` 对应的播放设备 ID；引擎释放时清空，用于判断是否需要重开。
    device_id: Option<String>,
    /// 文件试听增益持久保留，设备重新打开后立即恢复。
    volume: Option<f32>,
    /// 阻止已请求关闭的异步打开在停机后重新占用设备。
    generation: u64,
}

/// 引擎内部共享的试听输出句柄。
/// 试听状态的共享句柄；所有设备打开、流投递和停止操作都经此锁串行化。
pub(crate) type SharedMonitor = Arc<Mutex<MonitorState>>;
/// 创建空的试听输出句柄。
pub(crate) fn shared() -> SharedMonitor {
    Arc::new(Mutex::new(MonitorState::default()))
}

/// 异步打开或关闭试听设备，不阻塞 Slint 回调。
///
/// 已经打开同一设备时直接返回：重开会在解码线程播放途中替换引擎，
/// 把刚开始的流连同旧引擎一起丢掉，表现为「第二次播放没声音」。
pub(crate) fn set_default_monitor(monitor: &SharedMonitor, enabled: bool) {
    if !enabled {
        close(monitor);
        return;
    }
    // 打开设备放到后台线程，避免 Slint 回调被 WASAPI 初始化阻塞。
    // 线程只持有共享句柄；设备打开完成后写回引擎，函数结束即释放临时资源。
    let generation = monitor
        .lock()
        .map(|state| state.generation)
        .unwrap_or_default();
    let monitor = monitor.clone();
    thread::spawn(move || {
        let Ok(id) = device::default_render_id() else {
            tracing::warn!("试听：取不到系统默认播放设备");
            return;
        };
        if already_open(&monitor, &id) {
            return;
        }
        // 设备打开完成后再次确认，防止并发异步请求覆盖刚落位的同设备引擎。
        let next = match PreviewEngine::open(&id) {
            Ok(engine) => Some(engine),
            Err(error) => {
                tracing::warn!("试听设备打开失败（{id}）：{error}");
                None
            }
        };
        // 写回共享状态失败时，临时 PreviewEngine 随线程退出释放；未就绪状态可由后续播放重试。
        let Ok(mut state) = monitor.lock() else {
            return;
        };
        if state.generation != generation {
            return;
        }
        // 打开期间可能已有同设备的引擎落位；不要覆盖它。
        if state.engine.is_some() && state.device_id.as_deref() == Some(id.as_str()) {
            return;
        }
        if let Some(engine) = next.as_ref() {
            engine.set_volume(state.volume.unwrap_or(0.5));
        }
        state.device_id = Some(id);
        state.engine = next;
    });
}

/// 关闭试听输出并释放设备。
fn close(monitor: &SharedMonitor) {
    // 丢弃 PreviewEngine 会释放 WASAPI 播放占用；同时清除匹配 ID。
    if let Ok(mut state) = monitor.lock() {
        state.generation = state.generation.wrapping_add(1);
        state.engine = None;
        state.device_id = None;
        // 试听对象在锁保护下清空；Drop 会释放 WASAPI 播放占用。
    }
}

/// 释放试听占用的播放设备；停止转发或切换路由目标前调用。
///
/// 试听与转发可能指向同一端点（或同一驱动的另一端点），不先归还设备
/// 就会在 `Initialize` 收到 `AUDCLNT_E_DEVICE_IN_USE`。
pub(crate) fn release(monitor: &SharedMonitor) {
    close(monitor);
}

/// 是否已经打开该设备的试听引擎。
fn already_open(monitor: &SharedMonitor, id: &str) -> bool {
    monitor
        .lock()
        .map(|state| state.engine.is_some() && state.device_id.as_deref() == Some(id))
        .unwrap_or(false)
}

/// 确保试听输出已就绪；尚未打开时同步打开。
///
/// `set_default_monitor` 是异步的，而 `play_file` 立刻开始解码投递：
/// 试听引擎还没落位时 `play_stream` 会返回 Err 并被吞掉，用户听到的是
/// 「显示播放中但没有声音」。这里在投递前把设备准备好。
///
/// 只在真正需要打开时才会阻塞（通常是首次播放），已打开时是纯读锁。
/// 同步打开完成后才写入共享状态，调用方随后投递的流不会落到尚未初始化的引擎。
pub(crate) fn ensure_default_monitor(monitor: &SharedMonitor, enabled: bool) {
    if !enabled {
        return;
    }
    let Ok(id) = device::default_render_id() else {
        tracing::warn!("试听：取不到系统默认播放设备");
        return;
    };
    if already_open(monitor, &id) {
        return;
    }
    // 同步打开只在首次使用时阻塞；后续播放复用已有设备避免丢流。
    let next = match PreviewEngine::open(&id) {
        Ok(engine) => Some(engine),
        Err(error) => {
            tracing::warn!("试听设备打开失败（{id}）：{error}");
            None
        }
    };
    // 只有成功拿到锁才发布结果；失败时保留 None，使后续调用仍可重试。
    if let Ok(mut state) = monitor.lock() {
        if let Some(engine) = next.as_ref() {
            engine.set_volume(state.volume.unwrap_or(0.5));
        }
        state.device_id = Some(id);
        state.engine = next;
    }
}

/// 设置试听音量；音量本身只写入原子值。
pub(crate) fn set_volume(monitor: &SharedMonitor, value: f32) {
    if let Ok(mut state) = monitor.lock() {
        let value = value.clamp(0.0, 1.0);
        state.volume = Some(value);
        if let Some(preview) = state.engine.as_ref() {
            preview.set_volume(value);
        }
    }
}

/// 请求停止当前试听源，不在调用线程等待解码线程。
pub(crate) fn request_stop(monitor: &SharedMonitor) {
    if let Ok(state) = monitor.lock()
        && let Some(preview) = state.engine.as_ref()
    {
        preview.request_stop_source();
    }
}

/// 将一个已解码流交给试听输出线程。
pub(crate) fn play_stream(
    monitor: &SharedMonitor,
    info: MediaInfo,
    rx: std::sync::mpsc::Receiver<StreamChunk>,
    looped: bool,
) -> Result<(), Error> {
    // receiver 的所有权在调用成功后移交给 PreviewEngine；试听线程随后独立消费它。
    // 此处只持有状态锁完成一次性查找，避免把播放等待带入 UI 调用线程。
    // 持锁期间仅提交流，不等待播放线程；PreviewEngine 接管 receiver 的消费。
    let state = monitor
        .lock()
        .map_err(|_| Error::RenderInit("monitor lock poisoned"))?;
    let Some(preview) = state.engine.as_ref() else {
        return Err(Error::RenderInit("monitor is disabled"));
    };
    preview.play_stream(info, rx, looped);
    Ok(())
}

use super::AudioEngine;
/// AudioEngine 的本机试听配置入口。
impl AudioEngine {
    /// 异步切换系统默认播放设备试听。
    pub fn set_monitor_default(&self, enabled: bool) {
        set_default_monitor(&self.monitor, enabled);
    }

    /// 确保试听就绪后再开始播放；用于播放/恢复路径。
    pub fn ensure_monitor_default(&self, enabled: bool) {
        ensure_default_monitor(&self.monitor, enabled);
    }

    /// 设置试听音量。
    pub fn set_monitor_volume(&self, value: f32) {
        set_volume(&self.monitor, value);
    }
}

/// 后台工作线程只操作控制面的输出状态；实时线程从不获取这些锁。
pub(super) struct RouteMicContext {
    /// 当前输出图；监听 worker 只在控制线程读取。
    pub(super) output: Arc<Mutex<Option<super::OutputState>>>,
    /// 物理麦捕获线程的共享表，用于切换时停止旧生产者。
    pub(super) sources: Arc<Mutex<super::SourceThreads>>,
    /// 串行化输出图替换与捕获线程回收，避免双生产者。
    pub(super) guard: Arc<Mutex<()>>,
    /// 输出图已经完整提交的标志。
    pub(super) route_active: Arc<AtomicBool>,
    /// 当前麦克风是否已经作为转发源启用。
    pub(super) mic_forwarding: Arc<AtomicBool>,
    /// 捕获线程是否仍向监听环生产有效样本。
    pub(super) mic_active: Arc<AtomicBool>,
}

impl RouteMicContext {
    /// 检查已建输出图是否有默认物理播放端的监听环；只读共享控制面。
    fn has_preview_target(&self) -> bool {
        self.output
            .lock()
            .ok()
            .and_then(|output| {
                output.as_ref().map(|graph| {
                    graph
                        .targets
                        .iter()
                        .any(|target| target.mic_preview_ring.is_some())
                })
            })
            .unwrap_or(false)
    }

    /// 按当前预览增益异步切换路由图中的物理麦捕获；返回是否可直接写入监听环。
    fn refresh(&self, selected: Option<&str>, volume: &AtomicU32) -> bool {
        let Ok(_guard) = self.guard.lock() else {
            return false;
        };
        if !self.route_active.load(Ordering::Acquire) {
            return false;
        }
        let has_target = self.has_preview_target();
        // 转发中的捕获已经由路由启动路径建立；这里仅报告监听环存在，避免替换它。
        if self.mic_forwarding.load(Ordering::Acquire) {
            return has_target;
        }
        let selected = if f32::from_bits(volume.load(Ordering::Relaxed)) > 0.0 {
            selected
        } else {
            None
        };
        let selected = selected.filter(|_| has_target);
        let existing = self
            .sources
            .lock()
            .ok()
            .and_then(|sources| sources.mic_source_id.clone());
        if existing.as_deref() == selected {
            return has_target;
        }
        self.mic_active.store(false, Ordering::Release);
        let old = self.sources.lock().ok().map(|mut sources| {
            sources.mic_source_id = None;
            sources.mic_peak = None;
            (sources.mic_stop.take(), sources.mic.take())
        });
        if let Some((stop, thread)) = old {
            if let Some(stop) = stop {
                stop.store(true, Ordering::Release);
            }
            if let Some(thread) = thread {
                let _ = thread.join();
            }
        }
        let Some(id) = selected else {
            return has_target;
        };
        let graph = self.output.lock().ok().and_then(|output| {
            output.as_ref().map(|graph| {
                (
                    graph.mic_ring.clone(),
                    graph.render_format,
                    graph.mix_format,
                )
            })
        });
        let Some((ring, format, mix)) = graph else {
            return has_target;
        };
        if let Err(error) = super::start_route_mic_capture(
            id,
            ring,
            format,
            mix,
            &self.sources,
            self.mic_active.clone(),
        ) {
            tracing::warn!("麦克风本机试听捕获启动失败：{error}");
        }
        has_target
    }
}

/// 独立于路由状态的物理麦本机试听。控制线程异步处理设备开关和线程回收。
pub(super) struct MicPreviewController {
    /// 用户选择的物理麦设备 ID；空值表示不监听。
    selected: Arc<Mutex<Option<String>>>,
    /// 控制命令通道，不承载音频数据。
    tx: mpsc::Sender<MicPreviewCommand>,
    /// 控制线程句柄；析构时发送关闭命令并 join。
    worker: Mutex<Option<JoinHandle<()>>>,
}

/// 空闲监听控制线程接收的生命周期命令。
enum MicPreviewCommand {
    /// 重新读取设备选择和音量，必要时切换监听会话。
    Refresh,
    /// 暂停独立监听并同步确认设备已释放。
    Pause(mpsc::SyncSender<()>),
    /// 输出图建立失败或停止后恢复独立监听。
    Resume,
    /// 退出控制线程并释放当前会话。
    Shutdown,
}

/// 一次独立物理麦监听会话；捕获与 render 线程共享停止标志。
struct StandaloneMic {
    /// 当前会话使用的物理麦设备 ID。
    source_id: String,
    /// 同时通知捕获和 render 线程退出。
    stop: Arc<AtomicBool>,
    /// 捕获失败后阻止 render 播放残留样本。
    active: Arc<AtomicBool>,
    /// 物理麦捕获线程。
    capture: JoinHandle<()>,
    /// 系统默认播放端 render 线程。
    render: JoinHandle<()>,
}

impl StandaloneMic {
    /// 在后台控制线程中打开物理麦捕获和系统默认播放端 render。
    fn open(source_id: &str, volume: Arc<AtomicU32>) -> Result<Self, Error> {
        use crate::audio::buffer::SpscRing;
        use crate::audio::capture::{open_mic_capture, run_capture_loop};
        use crate::audio::com;
        use crate::audio::format::{AudioFormat, Converter, SampleKind};
        use crate::audio::render::{open_render_device, run_render_loop};

        let _com = com::init_mta()?;
        let render_id = super::output::physical_default_render_id().ok_or(Error::DeviceNotFound)?;
        let client = open_render_device(&render_id)?;
        let target = client.format;
        let mix = *client.mix_format();
        let input = open_mic_capture(source_id, Some(&mix))?;
        let converter = Converter::new(
            input.format,
            AudioFormat {
                sample_rate: target.sample_rate,
                channels: target.channels,
                kind: SampleKind::F32,
            },
        );
        let capacity =
            ((target.sample_rate as usize).saturating_mul(target.channels as usize) / 2).max(8192);
        let ring = Arc::new(SpscRing::with_capacity_samples(capacity));
        let stop = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicBool::new(true));
        let render = {
            let stop = stop.clone();
            let active = active.clone();
            let ring = ring.clone();
            thread::Builder::new()
                .name("mic-preview-render".into())
                .spawn(move || {
                    let _com = com::init_mta();
                    run_render_loop(
                        client,
                        Vec::new(),
                        None,
                        Vec::new(),
                        Vec::new(),
                        None,
                        Some((ring, volume, active)),
                        Vec::new(),
                        Arc::new(AtomicU32::new(1.0f32.to_bits())),
                        None,
                        stop.clone(),
                        move |error| {
                            tracing::warn!("麦克风本机试听播放失败：{error}");
                            stop.store(true, Ordering::Release);
                        },
                    );
                })
                .map_err(|_| Error::RenderInit("spawn mic preview render"))?
        };
        let capture = {
            let stop_for_capture = stop.clone();
            let active = active.clone();
            match thread::Builder::new()
                .name("mic-preview-capture".into())
                .spawn(move || {
                    let _com = com::init_mta();
                    run_capture_loop(
                        input,
                        ring,
                        converter,
                        stop_for_capture.clone(),
                        None,
                        move |error| {
                            tracing::warn!("麦克风本机试听捕获失败：{error}");
                            active.store(false, Ordering::Release);
                            stop_for_capture.store(true, Ordering::Release);
                        },
                    );
                }) {
                Ok(capture) => capture,
                Err(_) => {
                    stop.store(true, Ordering::Release);
                    let _ = render.join();
                    return Err(Error::CaptureInit("spawn mic preview capture"));
                }
            }
        };
        Ok(Self {
            source_id: source_id.to_owned(),
            stop,
            active,
            capture,
            render,
        })
    }

    /// 先发布停止，再回收捕获与 render，避免设备句柄泄漏。
    fn close(self) {
        self.active.store(false, Ordering::Release);
        self.stop.store(true, Ordering::Release);
        let _ = self.capture.join();
        let _ = self.render.join();
    }
}

impl MicPreviewController {
    /// 创建空闲监听控制线程；失败时保留可安全关闭的空 worker。
    pub(super) fn new(volume: Arc<AtomicU32>, route: RouteMicContext) -> Self {
        let selected: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let worker_selection = selected.clone();
        let (tx, rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("mic-preview-control".into())
            .spawn(move || {
                let mut paused = false;
                let mut session: Option<StandaloneMic> = None;
                while let Ok(command) = rx.recv() {
                    match command {
                        MicPreviewCommand::Pause(reply) => {
                            paused = true;
                            if let Some(old) = session.take() {
                                old.close();
                            }
                            let _ = reply.send(());
                            continue;
                        }
                        MicPreviewCommand::Resume => paused = false,
                        MicPreviewCommand::Shutdown => break,
                        MicPreviewCommand::Refresh => {}
                    }
                    let selected = worker_selection.lock().ok().and_then(|id| id.clone());
                    let route_active = route.route_active.load(Ordering::Acquire);
                    let route_preview = if paused && route_active {
                        route.refresh(selected.as_deref(), &volume)
                    } else {
                        false
                    };
                    let enabled = f32::from_bits(volume.load(Ordering::Relaxed)) > 0.0;
                    // 路由图已有默认物理播放端时复用其监听环；没有时才允许独立端点补位。
                    if route_preview || !enabled || selected.is_none() || (paused && !route_active)
                    {
                        if let Some(old) = session.take() {
                            old.close();
                        }
                        continue;
                    }
                    let Some(source_id) = selected.as_deref() else {
                        continue;
                    };
                    if session.as_ref().is_some_and(|old| {
                        old.source_id == source_id && !old.stop.load(Ordering::Acquire)
                    }) {
                        continue;
                    }
                    if let Some(old) = session.take() {
                        old.close();
                    }
                    match StandaloneMic::open(source_id, volume.clone()) {
                        Ok(next) => session = Some(next),
                        Err(error) => tracing::warn!("麦克风本机试听启动失败：{error}"),
                    }
                }
                if let Some(old) = session {
                    old.close();
                }
            })
            .ok();
        Self {
            selected,
            tx,
            worker: Mutex::new(worker),
        }
    }

    /// UI 入口仅更新 ID 并投递命令；实际 COM 打开和线程回收都在后台执行。
    pub(super) fn set_source(&self, id: Option<&str>) -> Result<(), Error> {
        *self
            .selected
            .lock()
            .map_err(|_| Error::CaptureInit("mic preview selection lock"))? = id.map(str::to_owned);
        self.refresh();
        Ok(())
    }

    pub(super) fn selected(&self) -> Option<String> {
        self.selected.lock().ok().and_then(|id| id.clone())
    }

    pub(super) fn refresh(&self) {
        let _ = self.tx.send(MicPreviewCommand::Refresh);
    }

    /// 路由创建前同步归还设备，保证独立试听和转发不重复打开同一端点。
    pub(super) fn pause(&self) {
        let (tx, rx) = mpsc::sync_channel(0);
        if self.tx.send(MicPreviewCommand::Pause(tx)).is_ok() {
            let _ = rx.recv();
        }
    }

    pub(super) fn resume(&self) {
        let _ = self.tx.send(MicPreviewCommand::Resume);
    }

    pub(super) fn shutdown(&self) {
        let _ = self.tx.send(MicPreviewCommand::Shutdown);
        if let Some(worker) = self.worker.lock().ok().and_then(|mut slot| slot.take()) {
            let _ = worker.join();
        }
    }
}
