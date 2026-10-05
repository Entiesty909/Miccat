//! 桥接 Slint 热键捕获与系统热键服务。

use super::*;
use crate::hotkey::{ENTRY_ID_BASE, HotkeyServer};
use std::collections::BTreeMap;
use std::sync::Arc;

/// 内置动作：开始/停止转发。
pub(super) const ACTION_TOGGLE_ROUTE: i32 = 1;
/// 内置动作：上一首。
pub(super) const ACTION_PREVIOUS: i32 = 2;
/// 内置动作：下一首。
pub(super) const ACTION_NEXT: i32 = 3;
/// 内置动作：停止播放。
pub(super) const ACTION_STOP_PLAYBACK: i32 = 4;

/// 启动安全系统热键服务；回调按绑定 id 分发到对应动作。
pub(super) fn start_hotkey(
    ui: &MainWindow,
    hotkey: &Rc<RefCell<config::Hotkey>>,
    bindings: BTreeMap<i32, config::Hotkey>,
) -> HotkeyServer {
    let ui_weak = ui.as_weak();
    let _ = hotkey;
    HotkeyServer::start(
        crate::hotkey::HotkeyMode::System,
        bindings,
        Arc::new(move |id| {
            let ui_weak = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = ui_weak.upgrade() else { return };
                dispatch(&ui, id);
            });
        }),
    )
}

/// 按绑定 id 触发对应动作。
fn dispatch(ui: &MainWindow, id: i32) {
    match id {
        ACTION_TOGGLE_ROUTE => ui.invoke_start_stop(),
        ACTION_PREVIOUS => ui.invoke_player_previous(),
        ACTION_NEXT => ui.invoke_player_next(),
        ACTION_STOP_PLAYBACK => ui.invoke_audio_stop(),
        _ if id >= ENTRY_ID_BASE => {
            let index = id - ENTRY_ID_BASE;
            ui.invoke_audio_hotkey_index(index);
        }
        _ => {}
    }
}

/// 重注册绑定并把注册失败回报给 UI。
pub(super) fn resync_bindings(
    ui: &MainWindow,
    server: &HotkeyServer,
    app_config: &Rc<RefCell<config::AppConfig>>,
) {
    server.set_bindings(collect_bindings(&app_config.borrow()));
    let failures = server.take_failures();
    if let Some(failure) = failures.first() {
        ui.set_error_text(format!("热键注册失败：{}", failure.reason).into());
        super::diagnostics::report_warning(ui, &failure.reason);
    }
}

/// 把配置里的热键汇总成「id → 组合」的绑定表。
pub(super) fn collect_bindings(config: &config::AppConfig) -> BTreeMap<i32, config::Hotkey> {
    let mut bindings = BTreeMap::new();
    bindings.insert(ACTION_TOGGLE_ROUTE, config.hotkeys.toggle_route);
    if let Some(hk) = config.hotkeys.previous_track {
        bindings.insert(ACTION_PREVIOUS, hk);
    }
    if let Some(hk) = config.hotkeys.next_track {
        bindings.insert(ACTION_NEXT, hk);
    }
    if let Some(hk) = config.hotkeys.stop_playback {
        bindings.insert(ACTION_STOP_PLAYBACK, hk);
    }
    for (index, entry) in config.audio_entries.iter().enumerate() {
        if let Some(hk) = entry.hotkey_binding {
            bindings.insert(ENTRY_ID_BASE + index as i32, hk);
        }
    }
    bindings
}
