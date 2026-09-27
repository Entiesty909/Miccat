//! 持有桥接层运行期共享对象，缩短回调参数列表。

use std::cell::RefCell;
use std::rc::Rc;

use crate::config;
use crate::engine::AudioEngine;
use crate::hotkey::HotkeyServer;

use super::cache::UiCache;
use super::persist::SaveQueue;

/// 桥接层共享对象；音频运行时仍由 AudioEngine 持有。
#[derive(Clone)]
pub(super) struct AppState {
    /// 音频运行期权威。
    pub(super) engine: Rc<AudioEngine>,
    /// Slint 派生视图状态。
    pub(super) cache: Rc<RefCell<UiCache>>,
    /// 需在退出或停止时还原的默认麦克风。
    pub(super) saved_mics: Rc<RefCell<Vec<(u32, String)>>>,
    /// 当前系统热键值。
    pub(super) hotkey: Rc<RefCell<config::Hotkey>>,
    /// 系统热键服务。
    pub(super) hotkey_server: Rc<HotkeyServer>,
    /// 持久化配置权威。
    pub(super) app_config: Rc<RefCell<config::AppConfig>>,
    /// 延迟写入与退出 flush 的配置队列。
    pub(super) save_queue: Rc<SaveQueue>,
    /// 进程捕获电平历史。
    pub(super) capture_hist: Rc<RefCell<Vec<f32>>>,
    /// 麦克风捕获电平历史。
    pub(super) mic_hist: Rc<RefCell<Vec<f32>>>,
    /// 输出电平历史。
    pub(super) render_hist: Rc<RefCell<Vec<f32>>>,
}
