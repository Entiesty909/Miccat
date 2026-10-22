//! 引擎内部文件播放：解码、SFX/试听分发和可轮询播放状态。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::audio::decode::{self, MediaInfo};
use crate::audio::sfx::SfxCommand;
use crate::config::AudioEntry;
use crate::error::Error;

/// 音频库播放流的目的地组合。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PlaybackTarget {
    /// 同时发送到路由和本机试听。
    #[default]
    Both,
    /// 只发送到本机默认播放设备。
    Monitor,
    /// 只发送到当前路由输出。
    Route,
}

use super::monitor::{self, SharedMonitor};

/// 引擎接收的文件播放请求。
#[derive(Clone)]
pub(crate) struct FilePlaybackRequest {
    /// 文件路径。
    pub(crate) path: PathBuf,
    /// UI 显示名称。
    pub(crate) name: String,
    /// 配置中的媒体时长；解码成功后以媒体元数据校正。
    pub(crate) duration_hns: u64,
    /// 是否循环。
    pub(crate) looped: bool,
    /// 本次播放启用的路由与本机试听目标。
    pub(crate) target: PlaybackTarget,
}

/// 将配置条目转换为引擎内部请求；目标由调用方按播放场景覆盖。
impl From<&AudioEntry> for FilePlaybackRequest {
    /// 从配置复制文件播放参数，并把目标初始化为双路；具体场景可由调用方覆盖目标。
    fn from(entry: &AudioEntry) -> Self {
        Self {
            path: PathBuf::from(&entry.path),
            name: entry.name.clone(),
            duration_hns: entry.duration_hns,
            looped: entry.loop_playback,
            target: PlaybackTarget::Both,
        }
    }
}

/// UI 轮询用的文件播放快照。
#[derive(Clone, Default)]
pub(crate) struct FilePlaybackState {
    /// 当前是否仍在播放。
    pub(crate) playing: bool,
    /// 显示名称。
    pub(crate) name: String,
    /// 文件路径。
    pub(crate) path: String,
    /// 是否循环。
    pub(crate) looped: bool,
    /// 总时长，100 ns 为单位。
    pub(crate) duration_hns: u64,
    /// 当前媒体位置，100 ns 为单位。
    pub(crate) position_hns: u64,
    /// 解码或试听错误短句。
    pub(crate) error: Option<String>,
}

#[derive(Default)]
/// 文件播放控制的共享可变状态；取消令牌由最新播放拥有，旧解码线程只观察它。
pub(crate) struct PlaybackControl {
    /// 当前解码线程的取消令牌。
    cancel: Option<Arc<AtomicBool>>,
    /// 当前请求元数据，供 UI 快照复用名称、路径和循环设置。
    request: Option<FilePlaybackRequest>,
    /// 单调时钟起点及其媒体偏移（100 ns），避免墙上时钟回拨影响进度。
    started: Option<(Instant, u64)>,
    /// 解码后校正的媒体总时长，单位为 100 ns。
    duration_hns: u64,
    /// 对外报告的播放标志；停止或错误时立即清除。
    playing: bool,
    /// 最近一次解码/投递失败的用户可读信息。
    error: Option<String>,
}

/// 创建共享播放控制状态；初始没有活动解码线程。
pub(crate) fn shared() -> Arc<Mutex<PlaybackControl>> {
    Arc::new(Mutex::new(PlaybackControl::default()))
}

/// 记录请求并异步启动解码；调用线程不等待文件 I/O 或设备投递。
pub(crate) fn start(
    control: &Arc<Mutex<PlaybackControl>>,
    monitor_handle: &SharedMonitor,
    sfx_sender: Option<Sender<SfxCommand>>,
    request: FilePlaybackRequest,
    start_hns: u64,
) -> Result<(), Error> {
    // 取消令牌只属于本次请求；新请求会替换共享槽中的令牌，旧线程仍可安全观察它。
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut state = control
            .lock()
            .map_err(|_| Error::RenderInit("playback lock poisoned"))?;
        state.cancel = Some(cancel.clone());
        state.request = Some(request.clone());
        state.started = Some((Instant::now(), start_hns));
        state.duration_hns = request.duration_hns;
        state.playing = true;
        state.error = None;
    }

    // 播放线程脱离调用线程运行；控制状态和取消令牌是停止、错误与线程退出之间的同步接缝。
    let control = control.clone();
    let monitor_handle = monitor_handle.clone();
    thread::Builder::new()
        .name("engine-file-playback".into())
        .spawn(move || {
            start_streams(
                &control,
                &monitor_handle,
                sfx_sender,
                request,
                start_hns,
                cancel,
            );
        })
        .map_err(|_| Error::RenderInit("spawn file playback"))?;
    Ok(())
}

/// 按目的地分别创建解码流；两个目的地各自消费独立 receiver。
fn start_streams(
    control: &Arc<Mutex<PlaybackControl>>,
    monitor_handle: &SharedMonitor,
    sfx_sender: Option<Sender<SfxCommand>>,
    request: FilePlaybackRequest,
    start_hns: u64,
    cancel: Arc<AtomicBool>,
) {
    // 两路解码共享同一个取消令牌，因此停止一次即可让尚未投递完的两路共同收尾。
    // receiver 分别交给对应的后台消费者；本函数返回后，实际播放仍由各自消费者持有。
    // 路由与试听各自解码，避免一个 receiver 的消费速度影响另一路。
    let mut info = None;
    let mut sfx_stream = None;
    if sfx_sender.is_some()
        && request.target != PlaybackTarget::Monitor
        && let Ok((stream_info, rx)) = decode::stream_to_channel(
            request.path.clone(),
            start_hns,
            request.looped,
            cancel.clone(),
        )
    {
        info = Some(stream_info);
        sfx_stream = Some(rx);
    }

    let mut monitor_stream = None;
    if request.target != PlaybackTarget::Route
        && let Ok((stream_info, rx)) = decode::stream_to_channel(
            request.path.clone(),
            start_hns,
            request.looped,
            cancel.clone(),
        )
    {
        info = Some(stream_info);
        monitor_stream = Some(rx);
    }

    // 至少有一路流成功才更新媒体元数据；两路都失败时错误对外可见。
    let Some(info) = info else {
        finish_with_error(control, Error::Format("audio stream could not be opened"));
        return;
    };
    update_info(control, info);

    // 独立播放槽不占用路由文件槽，停止卡片不会清除这里的流。
    if let (Some(sender), Some(rx)) = (sfx_sender, sfx_stream) {
        // SFX 已关闭时只放弃路由投递，试听流仍按自身生命周期继续交接。
        let _ = sender.send(SfxCommand::PlayStream {
            slot: super::PLAYBACK_SLOT,
            sample_rate: info.sample_rate,
            channels: info.channels,
            start_hns,
            volume: 1.0,
            rx,
        });
    }
    if let Some(rx) = monitor_stream {
        match monitor::play_stream(monitor_handle, info, rx, request.looped) {
            Ok(()) => {}
            Err(error) => {
                // 试听没接住这路流；若路由也没接住，用户会看到「播放中但没声音」。
                tracing::warn!("试听流未投递（{error}）");
                if request.target == PlaybackTarget::Monitor {
                    finish_with_error(control, Error::RenderInit("本机试听设备未就绪"));
                }
            }
        }
    }
    // 解码线程只负责创建并交接流，不等待流播放完毕；播放状态由共享状态和单调时钟继续提供。
}

/// 用解码器提供的元数据校正 UI 时长，替代配置中的估算值。
fn update_info(control: &Arc<Mutex<PlaybackControl>>, info: MediaInfo) {
    if let Ok(mut state) = control.lock() {
        state.duration_hns = info.duration_hns;
    }
}

/// 将后台错误写入共享状态，并终止 UI 所见的播放生命周期。
fn finish_with_error(control: &Arc<Mutex<PlaybackControl>>, error: Error) {
    if let Ok(mut state) = control.lock() {
        state.playing = false;
        state.error = Some(error.user_message());
    }
}

/// 请求停止当前文件播放，不等待后台解码线程。
pub(crate) fn stop(
    control: &Arc<Mutex<PlaybackControl>>,
    monitor_handle: &SharedMonitor,
    sfx_sender: Option<Sender<SfxCommand>>,
) {
    // 取消令牌只负责让解码器尽快停止；SFX 与试听各自收到显式停止命令。
    if let Ok(mut state) = control.lock() {
        if let Some(cancel) = state.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        state.playing = false;
        state.started = None;
    }
    // 先停止保留槽，避免新的播放请求复用槽位时被旧流覆盖。
    if let Some(sender) = sfx_sender {
        let _ = sender.send(SfxCommand::StopSlot(super::PLAYBACK_SLOT));
    }
    monitor::request_stop(monitor_handle);
}

/// 返回当前文件播放快照，并按单调时钟计算位置。
pub(crate) fn state(control: &Arc<Mutex<PlaybackControl>>) -> FilePlaybackState {
    let Ok(state) = control.lock() else {
        return FilePlaybackState::default();
    };
    let Some(request) = state.request.as_ref() else {
        return FilePlaybackState::default();
    };
    // 位置由单调时钟推导，不依赖解码线程回报，轮询期间仍能连续更新。
    let mut playing = state.playing;
    let mut position_hns = state.started.map(|(_, position)| position).unwrap_or(0);
    if let Some((started, offset)) = state.started {
        let elapsed = (started.elapsed().as_nanos() / 100) as u64;
        let played = offset.saturating_add(elapsed);
        if state.duration_hns > 0 && !request.looped && played >= state.duration_hns {
            playing = false;
            position_hns = state.duration_hns;
        } else if state.duration_hns > 0 {
            position_hns = played % state.duration_hns;
        } else {
            position_hns = played;
        }
    }
    FilePlaybackState {
        playing,
        name: request.name.clone(),
        path: request.path.to_string_lossy().into_owned(),
        looped: request.looped,
        duration_hns: state.duration_hns,
        position_hns,
        error: state.error.clone(),
    }
}

use super::AudioEngine;

/// AudioEngine 的文件播放 API，负责把公共引擎状态接到内部控制函数。
impl AudioEngine {
    /// 按指定目的地播放音频库条目。
    pub fn play_file(
        &self,
        entry: &crate::config::AudioEntry,
        start_hns: u64,
        target: PlaybackTarget,
    ) -> Result<(), Error> {
        // 文件播放槽是单例；开始新条目前先发停止命令，避免旧流继续占用试听和路由目标。
        self.stop_file();
        let request = FilePlaybackRequest {
            target,
            ..FilePlaybackRequest::from(entry)
        };
        start(
            &self.playback,
            &self.monitor,
            self.sfx_sender().ok(),
            request,
            start_hns,
        )
    }

    /// 请求停止当前文件播放。
    pub fn stop_file(&self) {
        stop(&self.playback, &self.monitor, self.sfx_sender().ok());
    }

    /// 读取当前文件播放状态。
    pub fn file_state(&self) -> FilePlaybackState {
        state(&self.playback)
    }
    /// 复制当前 SFX 命令发送端；通道关闭时把生命周期错误传播给调用方。
    pub(crate) fn sfx_sender(&self) -> Result<Sender<SfxCommand>, Error> {
        self.sources
            .lock()
            .map_err(|_| Error::RenderInit("source lock poisoned"))?
            .sfx_tx
            .clone()
            .ok_or(Error::RenderInit("sfx is closed"))
    }
}

/// AudioEngine 的路由文件槽 API；每个槽独立发送 SFX 命令，slot 对应音频卡片下标。
impl AudioEngine {
    /// 在指定音频卡片槽位从媒体位置播放；可保持原暂停态，循环设置由卡片控制。
    pub fn play_file_source(
        &self,
        entry: &crate::config::AudioEntry,
        slot: usize,
        looped: bool,
        start_hns: u64,
        paused: bool,
    ) -> Result<(), Error> {
        let generation = self
            .file_source_generations
            .get(slot)
            .cloned()
            .ok_or(Error::RenderInit("file source slot out of range"))?;
        let ticket = {
            let mut current = generation
                .lock()
                .map_err(|_| Error::RenderInit("file source lock poisoned"))?;
            *current = current.wrapping_add(1);
            *current
        };
        let sender = self.sfx_sender()?;
        let request = FilePlaybackRequest {
            target: PlaybackTarget::Route,
            looped,
            ..FilePlaybackRequest::from(entry)
        };
        let path = request.path.clone();
        // 该线程只负责解码并把 receiver 交给指定槽；槽位停止由 SFX 命令完成，不阻塞调用方。
        thread::Builder::new()
            .name("engine-file-source".into())
            .spawn(move || {
                // 解码线程只向指定槽投递；失败时 sender 生命周期由调用方观察。
                let cancel = Arc::new(AtomicBool::new(false));
                let Ok((info, rx)) = decode::stream_to_channel(path, start_hns, looped, cancel)
                else {
                    return;
                };
                // 旧解码请求不得在后续 seek/stop 之后覆盖同一槽位；检查与投递共用短锁。
                if let Ok(current) = generation.lock()
                    && *current == ticket
                {
                    let _ = sender.send(SfxCommand::PlayStream {
                        slot,
                        sample_rate: info.sample_rate,
                        channels: info.channels,
                        start_hns,
                        volume: 1.0,
                        rx,
                    });
                    if paused {
                        let _ = sender.send(SfxCommand::SetPaused { slot, paused: true });
                    }
                }
            })
            .map(|_| ())
            .map_err(|_| Error::RenderInit("spawn file source"))
    }
    /// 停止指定音频卡片槽位的路由流，不影响其他卡片。
    pub fn stop_file_source(&self, slot: usize) -> Result<(), Error> {
        let sender = self.sfx_sender()?;
        let generation = self
            .file_source_generations
            .get(slot)
            .ok_or(Error::RenderInit("file source slot out of range"))?;
        let mut current = generation
            .lock()
            .map_err(|_| Error::RenderInit("file source lock poisoned"))?;
        *current = current.wrapping_add(1);
        // 在同一短锁内投递停止命令，阻止尚在解码的旧请求随后重新占用该槽。
        sender
            .send(SfxCommand::StopSlot(slot))
            .map_err(|_| Error::RenderInit("sfx is closed"))
    }
    /// 读取指定路由音频卡片槽位状态，不包含音频库独立播放槽。
    pub(crate) fn file_source_state(&self, slot: usize) -> super::FileSourceState {
        let state = self
            .sources
            .lock()
            .ok()
            .and_then(|sources| {
                sources
                    .sfx_states
                    .as_ref()
                    .and_then(|states| states.get(slot))
                    .map(|state| state.load(Ordering::Relaxed))
            })
            .unwrap_or(0);
        // 未知编码按停止处理，保证新增状态值不会被误认为仍在播放。
        match state {
            1 => super::FileSourceState::Playing,
            2 => super::FileSourceState::Paused,
            3 => super::FileSourceState::Finished,
            _ => super::FileSourceState::Stopped,
        }
    }
    /// 读取指定路由文件槽已播放位置，单位为 100 纳秒。
    pub(crate) fn file_source_position_hns(&self, slot: usize) -> u64 {
        self.sources
            .lock()
            .ok()
            .and_then(|sources| {
                sources
                    .sfx_positions
                    .as_ref()
                    .and_then(|positions| positions.get(slot))
                    .map(|position| position.load(Ordering::Relaxed))
            })
            .unwrap_or(0)
    }

    /// 暂停或恢复指定路由音频卡片槽位，保留当前解码位置。
    pub(crate) fn set_file_source_paused(&self, slot: usize, paused: bool) -> Result<(), Error> {
        self.sfx_sender()?
            .send(SfxCommand::SetPaused { slot, paused })
            .map_err(|_| Error::RenderInit("sfx is closed"))
    }
}
