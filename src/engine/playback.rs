//! 引擎内部文件播放：解码、SFX/试听分发和可轮询播放状态。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::audio::decode::{self, MediaInfo};
use crate::audio::sfx::{SfxCommand, SfxMode};
use crate::config::AudioEntry;
use crate::error::Error;

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
}

impl From<&AudioEntry> for FilePlaybackRequest {
    fn from(entry: &AudioEntry) -> Self {
        Self {
            path: PathBuf::from(&entry.path),
            name: entry.name.clone(),
            duration_hns: entry.duration_hns,
            looped: entry.loop_playback,
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

pub(crate) struct PlaybackControl {
    cancel: Option<Arc<AtomicBool>>,
    request: Option<FilePlaybackRequest>,
    started: Option<(Instant, u64)>,
    duration_hns: u64,
    playing: bool,
    error: Option<String>,
}

impl Default for PlaybackControl {
    fn default() -> Self {
        Self {
            cancel: None,
            request: None,
            started: None,
            duration_hns: 0,
            playing: false,
            error: None,
        }
    }
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
    if sfx_sender.is_some() {
        if let Ok((stream_info, rx)) = decode::stream_to_channel(
            request.path.clone(),
            start_hns,
            request.looped,
            cancel.clone(),
        ) {
            info = Some(stream_info);
            sfx_stream = Some(rx);
        }
    }

    let mut monitor_stream = None;
    if let Ok((stream_info, rx)) = decode::stream_to_channel(
        request.path.clone(),
        start_hns,
        request.looped,
        cancel.clone(),
    ) {
        info = Some(stream_info);
        monitor_stream = Some(rx);
    }

    let Some(info) = info else {
        finish_with_error(control, Error::Format("audio stream could not be opened"));
        return;
    };
    update_info(control, info);

    if let (Some(sender), Some(rx)) = (sfx_sender, sfx_stream) {
        let mode = if request.looped {
            SfxMode::Loop
        } else {
            SfxMode::Once
        };
        let _ = sender.send(SfxCommand::PlayStream {
            sample_rate: info.sample_rate,
            channels: info.channels,
            mode,
            volume: 1.0,
            rx,
        });
    }
    if let Some(rx) = monitor_stream {
        if monitor::play_stream(monitor_handle, info, rx, request.looped).is_err() {
            // 试听关闭不是播放失败；SFX 仍可能已经接收了该流。
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
        let _ = sender.send(SfxCommand::StopAll);
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
    /// 播放音频库条目；解码和音频线程由引擎管理。
    pub fn play_file(
        &self,
        entry: &crate::config::AudioEntry,
        start_hns: u64,
    ) -> Result<(), Error> {
        self.stop_file();
        start(
            &self.playback,
            &self.monitor,
            self.sfx_sender().ok(),
            FilePlaybackRequest::from(entry),
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
}
