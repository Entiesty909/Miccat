//! 本机试听输出的引擎门面；UI 不直接持有 PreviewEngine。

use std::sync::{Arc, Mutex};
use std::thread;

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
        // 打开期间可能已有同设备的引擎落位；不要覆盖它。
        if state.engine.is_some() && state.device_id.as_deref() == Some(id.as_str()) {
            return;
        }
        state.device_id = Some(id);
        state.engine = next;
    });
}

/// 关闭试听输出并释放设备。
fn close(monitor: &SharedMonitor) {
    // 丢弃 PreviewEngine 会释放 WASAPI 播放占用；同时清除匹配 ID。
    if let Ok(mut state) = monitor.lock() {
        state.engine = None;
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
        state.device_id = Some(id);
        state.engine = next;
    }
}

/// 设置试听音量；音量本身只写入原子值。
pub(crate) fn set_volume(monitor: &SharedMonitor, value: f32) {
    if let Ok(state) = monitor.lock()
        && let Some(preview) = state.engine.as_ref()
    {
        preview.set_volume(value);
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
