use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::audio::device::{DestDevice, RenderDevice};
use crate::config;
use crate::engine::AudioEngine;
use crate::hotkey::HotkeyServer;
use crate::process::ProcessInfo;

use super::cache::UiCache;
use super::persist::SaveQueue;
use super::{AddSourceWindow, AudioEditWindow};
/// 所有 UI 设备源的后台枚举结果。
pub(super) struct EnumeratedLists {
    /// 可选择的本地进程。
    pub(super) processes: Vec<ProcessInfo>,
    /// 可混入的物理麦克风。
    pub(super) mics: Vec<RenderDevice>,
    /// 当前筛选下的真实 WASAPI 输出端点。
    pub(super) destinations: Vec<DestDevice>,
}

/// 跨线程传递进程和设备刷新结果。
#[derive(Default)]
pub(super) struct ListRefresh {
    /// 是否已有刷新任务或待消费结果。
    pub(super) pending: AtomicBool,
    /// 后台线程完成的列表或用户可见错误。
    pub(super) result: Mutex<Option<Result<EnumeratedLists, String>>>,
}

pub(super) struct AppWindows {
    /// 添加源的独立顶层窗口。
    pub(super) add_source: Rc<AddSourceWindow>,
    /// 音频条目编辑的独立顶层窗口。
    pub(super) audio_edit: Rc<AudioEditWindow>,
}
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
    /// 配置防抖写入与退出时 flush 的队列。
    pub(super) save_queue: Rc<SaveQueue>,
    /// 独立可见的添加源和编辑窗口。
    pub(super) windows: Rc<AppWindows>,
    /// 进程/设备列表枚举任务及其待消费结果。
    pub(super) list_refresh: Arc<ListRefresh>,
}
