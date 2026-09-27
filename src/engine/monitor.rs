//! 本机试听输出的引擎门面；UI 不直接持有 PreviewEngine。

use std::sync::{Arc, Mutex};
use std::thread;

use crate::audio::decode::MediaInfo;
use crate::audio::device;
use crate::audio::preview::PreviewEngine;
use crate::audio::sfx::StreamChunk;
use crate::error::Error;

/// 引擎内部共享的试听输出句柄。
pub(crate) type SharedMonitor = Arc<Mutex<Option<PreviewEngine>>>;

/// 创建空的试听输出句柄。
pub(crate) fn shared() -> SharedMonitor {
    Arc::new(Mutex::new(None))
}

/// 异步打开或关闭试听设备，不阻塞 Slint 回调。
pub(crate) fn set_monitor(monitor: &SharedMonitor, device_id: Option<String>) {
    let monitor = monitor.clone();
    thread::spawn(move || {
        let next = device_id.and_then(|id| PreviewEngine::open(&id).ok());
        if let Ok(mut current) = monitor.lock() {
            *current = next;
        }
    });
}

/// 异步打开系统默认播放设备作为试听目标。
pub(crate) fn set_default_monitor(monitor: &SharedMonitor, enabled: bool) {
    if !enabled {
        set_monitor(monitor, None);
        return;
    }
    let monitor = monitor.clone();
    thread::spawn(move || {
        let next = device::default_render_id()
            .and_then(|id| PreviewEngine::open(&id))
            .ok();
        if let Ok(mut current) = monitor.lock() {
            *current = next;
        }
    });
}

/// 设置试听音量；音量本身只写入原子值。
pub(crate) fn set_volume(monitor: &SharedMonitor, value: f32) {
    if let Ok(current) = monitor.lock() {
        if let Some(preview) = current.as_ref() {
            preview.set_volume(value);
        }
    }
}

/// 请求停止当前试听源，不在调用线程等待解码线程。
pub(crate) fn request_stop(monitor: &SharedMonitor) {
    if let Ok(current) = monitor.lock() {
        if let Some(preview) = current.as_ref() {
            preview.request_stop_source();
        }
    }
}

/// 将一个已解码流交给试听输出线程。
pub(crate) fn play_stream(
    monitor: &SharedMonitor,
    info: MediaInfo,
    rx: std::sync::mpsc::Receiver<StreamChunk>,
    looped: bool,
) -> Result<(), Error> {
    let current = monitor
        .lock()
        .map_err(|_| Error::RenderInit("monitor lock poisoned"))?;
    let Some(preview) = current.as_ref() else {
        return Err(Error::RenderInit("monitor is disabled"));
    };
    preview.play_stream(info, rx, looped);
    Ok(())
}

use super::AudioEngine;

impl AudioEngine {
    /// 异步切换试听目标；`None` 关闭试听。
    pub fn set_monitor(&self, device_id: Option<String>) {
        set_monitor(&self.monitor, device_id);
    }

    /// 异步切换系统默认播放设备试听。
    pub fn set_monitor_default(&self, enabled: bool) {
        set_default_monitor(&self.monitor, enabled);
    }

    /// 设置试听音量。
    pub fn set_monitor_volume(&self, value: f32) {
        set_volume(&self.monitor, value);
    }
}
