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

impl From<&AudioEntry> for FilePlaybackRequest {
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
pub(crate) struct PlaybackControl {
    cancel: Option<Arc<AtomicBool>>,
    request: Option<FilePlaybackRequest>,
    started: Option<(Instant, u64)>,
    duration_hns: u64,
    playing: bool,
    error: Option<String>,
}

/// 创建共享播放控制状态。
pub(crate) fn shared() -> Arc<Mutex<PlaybackControl>> {
    Arc::new(Mutex::new(PlaybackControl::default()))
}

/// 启动文件播放；耗时解码工作全部在引擎线程中执行。
pub(crate) fn start(
    control: &Arc<Mutex<PlaybackControl>>,
    monitor_handle: &SharedMonitor,
    sfx_sender: Option<Sender<SfxCommand>>,
    request: FilePlaybackRequest,
    start_hns: u64,
) -> Result<(), Error> {
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

fn start_streams(
    control: &Arc<Mutex<PlaybackControl>>,
    monitor_handle: &SharedMonitor,
    sfx_sender: Option<Sender<SfxCommand>>,
    request: FilePlaybackRequest,
    start_hns: u64,
    cancel: Arc<AtomicBool>,
) {
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

    let Some(info) = info else {
        finish_with_error(control, Error::Format("audio stream could not be opened"));
        return;
    };
    update_info(control, info);

    if let (Some(sender), Some(rx)) = (sfx_sender, sfx_stream) {
        let _ = sender.send(SfxCommand::PlayStream {
            slot: super::PLAYBACK_SLOT,
            sample_rate: info.sample_rate,
            channels: info.channels,
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
}

fn update_info(control: &Arc<Mutex<PlaybackControl>>, info: MediaInfo) {
    if let Ok(mut state) = control.lock() {
        state.duration_hns = info.duration_hns;
    }
}

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
    if let Ok(mut state) = control.lock() {
        if let Some(cancel) = state.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        state.playing = false;
        state.started = None;
    }
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

impl AudioEngine {
    /// 按指定目的地播放音频库条目。
    pub fn play_file(
        &self,
        entry: &crate::config::AudioEntry,
        start_hns: u64,
        target: PlaybackTarget,
    ) -> Result<(), Error> {
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
    pub(crate) fn sfx_sender(&self) -> Result<Sender<SfxCommand>, Error> {
        self.sources
            .lock()
            .map_err(|_| Error::RenderInit("source lock poisoned"))?
            .sfx_tx
            .clone()
            .ok_or(Error::RenderInit("sfx is closed"))
    }
}

impl AudioEngine {
    /// 把一个音频条目追加到当前路由 SFX 混音器，不停止其它文件流。
    pub fn play_file_source(
        &self,
        entry: &crate::config::AudioEntry,
        slot: usize,
    ) -> Result<(), Error> {
        let sender = self.sfx_sender()?;
        let request = FilePlaybackRequest {
            target: PlaybackTarget::Route,
            ..FilePlaybackRequest::from(entry)
        };
        let path = request.path.clone();
        let looped = request.looped;
        thread::Builder::new()
            .name("engine-file-source".into())
            .spawn(move || {
                let cancel = Arc::new(AtomicBool::new(false));
                let Ok((info, rx)) = decode::stream_to_channel(path, 0, looped, cancel) else {
                    return;
                };
                let _ = sender.send(SfxCommand::PlayStream {
                    slot,
                    sample_rate: info.sample_rate,
                    channels: info.channels,
                    volume: 1.0,
                    rx,
                });
            })
            .map(|_| ())
            .map_err(|_| Error::RenderInit("spawn file source"))
    }
    /// 停止指定音频卡片的路由流，不影响其他卡片。
    pub fn stop_file_source(&self, slot: usize) -> Result<(), Error> {
        self.sfx_sender()?
            .send(SfxCommand::StopSlot(slot))
            .map_err(|_| Error::RenderInit("sfx is closed"))
    }
}
