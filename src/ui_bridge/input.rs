//! 桥接 Slint 热键捕获与系统热键服务。

use super::*;
use crate::hotkey::HotkeyServer;

/// 启动安全系统热键服务。全局 hook 模式在 S6 另行启用。
pub(super) fn start_hotkey(ui: &MainWindow, hotkey: &Rc<RefCell<config::Hotkey>>) -> HotkeyServer {
    let ui_weak = ui.as_weak();
    HotkeyServer::start(
        crate::hotkey::HotkeyMode::System,
        *hotkey.borrow(),
        Arc::new(move || {
            let ui_weak = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.invoke_start_stop();
                }
            });
        }),
    )
}
