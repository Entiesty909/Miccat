//! UI 桥接层共享状态：连接音频引擎、配置权威、派生缓存和跨线程刷新结果。
//! 运行态由 `AudioEngine` 持有，持久化值由 `AppConfig` 持有，`UiCache` 只保存界面派生状态。

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
/// 后台枚举线程生成的一次完整设备/进程快照；它只用于替换 UI 模型，不承担状态权威。
pub(super) struct EnumeratedLists {
    /// 本次快照中的本地进程；刷新后由 UI 线程据此重建进程选择列表。
    pub(super) processes: Vec<ProcessInfo>,
    /// 本次快照中的物理输入设备；顺序与刷新时的麦克风下标模型对应。
    pub(super) mics: Vec<RenderDevice>,
    /// 按当前筛选条件枚举出的真实 WASAPI 输出端点；设备 ID 是不透明值。
    pub(super) destinations: Vec<DestDevice>,
}

/// 跨线程传递进程和设备刷新结果；只允许一个待处理刷新，结果由 UI 线程消费。
#[derive(Default)]
pub(super) struct ListRefresh {
    /// 刷新闸门：`false` 到 `true` 的原子转换保证不会并发启动多个枚举线程。
    pub(super) pending: AtomicBool,
    /// 后台线程写入、UI 线程取出的一次性快照或用户可见错误；互斥锁保护所有权转移。
    pub(super) result: Mutex<Option<Result<EnumeratedLists, String>>>,
}

/// 长期持有的独立窗口对象；关闭动作只改变可见性，避免重复注册回调。
pub(super) struct AppWindows {
    /// 添加源的独立顶层窗口；窗口生命周期随桥接状态保持。
    pub(super) add_source: Rc<AddSourceWindow>,
    /// 音频条目编辑的独立顶层窗口；窗口生命周期随桥接状态保持。
    pub(super) audio_edit: Rc<AudioEditWindow>,
}

/// 桥接层共享对象；音频运行时仍由 `AudioEngine` 持有，配置写回由 `SaveQueue` 协调。
#[derive(Clone)]
pub(super) struct AppState {
    /// 音频运行期权威；路由槽位、增益和播放状态必须通过它读取或修改。
    pub(super) engine: Rc<AudioEngine>,
    /// Slint 派生视图状态；刷新后可重建，不能取代引擎或配置中的权威值。
    pub(super) cache: Rc<RefCell<UiCache>>,
    /// 退出或停止路由时需要还原的默认麦克风，元素为进程 PID 与设备 ID。
    pub(super) saved_mics: Rc<RefCell<Vec<(u32, String)>>>,
    /// 当前系统热键配置；实际注册由 `hotkey_server` 执行。
    pub(super) hotkey: Rc<RefCell<config::Hotkey>>,
    /// 系统热键服务；与 UI 状态共享生命周期。
    pub(super) hotkey_server: Rc<HotkeyServer>,
    /// 持久化配置权威；UI 修改后通过保存队列写回磁盘。
    pub(super) app_config: Rc<RefCell<config::AppConfig>>,
    /// 配置防抖写入与退出时 flush 的队列。
    pub(super) save_queue: Rc<SaveQueue>,
    /// 独立可见的添加源和编辑窗口。
    pub(super) windows: Rc<AppWindows>,
    /// 进程/设备列表枚举任务及其待消费结果；不把后台快照直接暴露给 Slint。
    pub(super) list_refresh: Arc<ListRefresh>,
}
