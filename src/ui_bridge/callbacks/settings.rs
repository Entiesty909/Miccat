//! settings 相关 Slint 回调注册。

use super::super::*;
use crate::vbcable;
use std::thread;

/// 绑定预览、虚拟声卡安装/卸载和全局热键设置回调。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
}

/// 处理预览开关、预览音量、VB-Cable 安装卸载及开始录制事件。
fn register_1(ui: &MainWindow, state: &AppState) {
    let app_config = state.app_config.clone();
    let engine = state.engine.clone();
    {
        let save_queue = state.save_queue.clone();
        let engine = engine.clone();
        let app_config = app_config.clone();
        // 预览开关同时控制引擎监听默认设备与持久化配置，试听和路由互不改写。
        ui.on_preview_enabled_toggled(move |enabled| {
            engine.set_monitor_default(enabled);
            app_config.borrow_mut().preview_enabled = enabled;
            save_queue.schedule();
        });
    }
    {
        let save_queue = state.save_queue.clone();
        let engine = engine.clone();
        let app_config = app_config.clone();
        ui.on_preview_volume_edited(move |v| {
            let value = (v / 100.0).clamp(0.0, 1.0);
            engine.set_monitor_volume(value);
            app_config.borrow_mut().preview_volume = value;
            save_queue.schedule();
        });
    }
    {
        let ui_weak = ui.as_weak();
        // 安装/卸载在后台线程执行，完成事件通过 Slint 事件循环回到 UI 线程。
        ui.on_install_cable(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            if ui.get_cable_busy() {
                return;
            }
            ui.set_cable_busy(true);
            ui.set_cable_text("正在下载最新版…".into());
            ui.set_error_text("".into());
            let ui_weak = ui.as_weak();
            thread::spawn(move || {
                // 安装结果在 UI 线程消费，避免后台线程直接触碰 Slint 对象。
                let result = vbcable::install_latest();
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = ui_weak.upgrade() else { return };
                    ui.set_cable_busy(false);
                    match result {
                        Ok(()) => {
                            ui.set_cable_text("安装结束，请重启后点刷新".into());
                            ui.set_error_text("".into());
                        }
                        Err(e) => {
                            tracing::warn!("vb-cable install: {e}");
                            ui.set_cable_text("安装失败".into());
                            super::super::diagnostics::report_error(&ui, &e);
                        }
                    }
                });
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        // 忙碌标志防止重复启动驱动安装器；失败只上报错误并保留可重试状态。
        ui.on_uninstall_cable(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            if ui.get_cable_busy() {
                return;
            }
            ui.set_cable_busy(true);
            ui.set_cable_text("正在卸载…".into());
            ui.set_error_text("".into());
            let ui_weak = ui.as_weak();
            thread::spawn(move || {
                let result = vbcable::uninstall();
                // 卸载完成后只更新设备存在状态；失败保留当前状态，便于用户重试。
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = ui_weak.upgrade() else { return };
                    ui.set_cable_busy(false);
                    match result {
                        Ok(()) => {
                            ui.set_cable_installed(false);
                            ui.set_cable_text("已卸载，请重启后点刷新".into());
                            ui.set_error_text("".into());
                        }
                        Err(e) => {
                            tracing::warn!("vb-cable uninstall: {e}");
                            ui.set_cable_text("卸载失败".into());
                            super::super::diagnostics::report_error(&ui, &e);
                        }
                    }
                });
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        // 进入捕获态后由 AppFocusScope 接收下一次按键，主路由热键仍保持已注册状态。
        ui.on_begin_hotkey_capture(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_listening_hotkey(true);
            }
        });
    }
}

/// 处理主路由热键捕获、重置，并注册上一首/下一首/停止播放三行回调。
fn register_2(ui: &MainWindow, state: &AppState) {
    let hotkey = state.hotkey.clone();
    let hotkey_server = state.hotkey_server.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let hotkey = hotkey.clone();
        let server = hotkey_server.clone();
        let app_config = app_config.clone();
        // 修饰键位掩码固定为 Alt=1、Ctrl=2、Shift=4，必须与全局注册端一致。
        ui.on_hotkey_captured(move |text, ctrl, shift, alt| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            ui.set_listening_hotkey(false);
            // 无法映射的按键不写入配置，避免把不可重放的文本保存成全局热键。
            let Some(vk) = super::super::input::slint_key_to_vk(text.as_str()) else {
                return;
            };
            let mut mods = 0u32;
            if alt {
                mods |= 1;
            }
            if ctrl {
                mods |= 2;
            }
            if shift {
                mods |= 4;
            }
            let hk = config::Hotkey { mods, vk };
            // 先刷新运行时注册，再落盘配置，确保新组合键立即生效且重启后保持一致。
            *hotkey.borrow_mut() = hk;
            super::super::input::resync_bindings(&ui, &server, &app_config);
            let mut cfg = app_config.borrow_mut();
            cfg.hotkeys.toggle_route = hk;
            save_queue.schedule();
            ui.set_hotkey_label(crate::presentation::hotkey_label(hk).into());
        });
    }
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let hotkey = hotkey.clone();
        let server = hotkey_server.clone();
        let app_config = app_config.clone();
        // 重置后立即重同步系统热键，避免旧组合键仍留在注册表中。
        ui.on_reset_hotkey(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let hk = config::Hotkey::default();
            *hotkey.borrow_mut() = hk;
            super::super::input::resync_bindings(&ui, &server, &app_config);
            let mut config = app_config.borrow_mut();
            config.hotkeys.toggle_route = hk;
            save_queue.schedule();
            ui.set_listening_hotkey(false);
            ui.set_hotkey_label(crate::presentation::hotkey_label(hk).into());
        });
    }
    register_rows(ui, state);
}

/// 上一首/下一首/停止播放三行的录制与重置。
/// 绑定三行播放器热键的录制、冲突检查和重置操作。
fn register_rows(ui: &MainWindow, state: &AppState) {
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let server = state.hotkey_server.clone();
    {
        let ui_weak = ui.as_weak();
        ui.on_begin_hotkey_row(move |row| {
            let Some(ui) = ui_weak.upgrade() else { return };
            set_row_listening(&ui, row, true);
        });
    }
    {
        let app_config = app_config.clone();
        let save_queue = save_queue.clone();
        let server = server.clone();
        let ui_weak = ui.as_weak();
        // Esc 取消本次录制；无效键或单键热键不写入配置。
        // 停止监听发生在解析之前，Esc 或无效键都不会留下“正在录制”的 UI 状态。
        ui.on_hotkey_row_captured(move |row, text, ctrl, shift, alt| {
            let Some(ui) = ui_weak.upgrade() else { return };
            set_row_listening(&ui, row, false);
            if text == "\u{1b}" {
                return;
            }
            let Some(vk) = super::super::input::slint_key_to_vk(text.as_str()) else {
                ui.set_error_text("不支持该按键".into());
                return;
            };
            let mut mods = 0u32;
            if alt {
                mods |= 1;
            }
            if ctrl {
                mods |= 2;
            }
            if shift {
                mods |= 4;
            }
            let hk = config::Hotkey { mods, vk };
            // 修饰键掩码与主路由热键共用同一协议，冲突判断因此可以直接比较 mods/vk。
            if mods == 0 {
                ui.set_error_text("单键全局热键会拦截正常输入，建议加 Ctrl 或 Alt".into());
                return;
            }
            // 行热键与路由、其他行及音频条目共享命名空间，冲突时不覆盖已有绑定。
            if let Some(reason) = row_conflict(&app_config, row, hk) {
                ui.set_error_text(reason.into());
                return;
            }
            {
                // 只有通过全部校验的组合键才写入对应行，避免覆盖已有有效绑定。
                let mut cfg = app_config.borrow_mut();
                match row {
                    1 => cfg.hotkeys.previous_track = Some(hk),
                    2 => cfg.hotkeys.next_track = Some(hk),
                    _ => cfg.hotkeys.stop_playback = Some(hk),
                }
            }
            save_queue.schedule();
            super::super::input::resync_bindings(&ui, &server, &app_config);
            refresh_row_labels(&ui, &app_config);
        });
    }
    {
        let app_config = app_config.clone();
        let save_queue = save_queue.clone();
        let server = server.clone();
        let ui_weak = ui.as_weak();
        // 重置只清理目标行，再同步全局注册表和三行显示文案。
        ui.on_reset_hotkey_row(move |row| {
            let Some(ui) = ui_weak.upgrade() else { return };
            // 重置只清除目标行，其他播放器行的热键配置保持不变。
            {
                let mut cfg = app_config.borrow_mut();
                match row {
                    1 => cfg.hotkeys.previous_track = None,
                    2 => cfg.hotkeys.next_track = None,
                    _ => cfg.hotkeys.stop_playback = None,
                }
            }
            save_queue.schedule();
            super::super::input::resync_bindings(&ui, &server, &app_config);
            refresh_row_labels(&ui, &app_config);
        });
    }
}

/// 切换某一行的录制状态，并把当前行传给 AppFocusScope 以接收按键。
/// 切换某一行的「录制中」状态；同时告诉 AppFocusScope 当前在录哪一行。
fn set_row_listening(ui: &MainWindow, row: i32, listening: bool) {
    let state = ui.global::<HotkeyState>();
    match row {
        1 => state.set_listening_previous(listening),
        2 => state.set_listening_next(listening),
        _ => state.set_listening_stop(listening),
    }
    state.set_listening_row(if listening { row } else { -1 });
}

/// 将配置中的三行热键格式化为 Slint 可显示的标签。
/// 刷新三行的显示文案。
pub(crate) fn refresh_row_labels(ui: &MainWindow, app_config: &Rc<RefCell<config::AppConfig>>) {
    let config = app_config.borrow();
    // 未配置的热键显示为空字符串，避免把内部 Option 细节暴露给 Slint。
    let state = ui.global::<HotkeyState>();
    state.set_previous(
        config
            .hotkeys
            .previous_track
            .map(crate::presentation::hotkey_label)
            .unwrap_or_default()
            .into(),
    );
    state.set_next(
        config
            .hotkeys
            .next_track
            .map(crate::presentation::hotkey_label)
            .unwrap_or_default()
            .into(),
    );
    state.set_stop(
        config
            .hotkeys
            .stop_playback
            .map(crate::presentation::hotkey_label)
            .unwrap_or_default()
            .into(),
    );
}

/// 检查行热键与其它全局绑定的冲突；相同 mods/vk 才算同一按键。
/// 行热键与其它全局绑定的重复检测。
fn row_conflict(
    app_config: &Rc<RefCell<config::AppConfig>>,
    row: i32,
    binding: config::Hotkey,
) -> Option<String> {
    let config = app_config.borrow();
    let same = |other: &config::Hotkey| other.mods == binding.mods && other.vk == binding.vk;
    // 当前行要排除自身，否则编辑已有绑定时会被误判为与自己冲突。
    if row != 0 && same(&config.hotkeys.toggle_route) {
        return Some("与「开始/停止转发」热键重复".into());
    }
    let rows = [
        (1, config.hotkeys.previous_track, "上一首"),
        (2, config.hotkeys.next_track, "下一首"),
        (3, config.hotkeys.stop_playback, "停止播放"),
    ];
    for (id, value, name) in rows {
        if id != row && value.as_ref().is_some_and(same) {
            return Some(format!("与「{name}」热键重复"));
        }
    }
    // 音频条目的热键也在同一命名空间内，必须阻止行热键抢占条目快捷键。
    if config
        .audio_entries
        .iter()
        .any(|entry| entry.hotkey_binding.as_ref().is_some_and(same))
    {
        return Some("该快捷键已分配给某个音频条目".into());
    }
    None
}
