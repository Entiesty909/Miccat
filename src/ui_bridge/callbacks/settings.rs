//! settings 相关 Slint 回调注册。

use super::super::*;
use crate::vbcable;
use std::thread;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
}

fn register_1(ui: &MainWindow, state: &AppState) {
    let app_config = state.app_config.clone();
    let engine = state.engine.clone();
    {
        let save_queue = state.save_queue.clone();
        let engine = engine.clone();
        let app_config = app_config.clone();
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
        ui.on_begin_hotkey_capture(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_listening_hotkey(true);
            }
        });
    }
}

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
        ui.on_hotkey_captured(move |text, ctrl, shift, alt| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            ui.set_listening_hotkey(false);
            let Some(vk) = config::slint_key_to_vk(text.as_str()) else {
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
            *hotkey.borrow_mut() = hk;
            super::super::input::resync_bindings(&ui, &server, &app_config);
            let mut cfg = app_config.borrow_mut();
            cfg.hotkeys.toggle_route = hk;
            save_queue.schedule();
            ui.set_hotkey_label(hk.label().into());
        });
    }
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let hotkey = hotkey.clone();
        let server = hotkey_server.clone();
        let app_config = app_config.clone();
        ui.on_reset_hotkey(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let hk = config::Hotkey::default();
            *hotkey.borrow_mut() = hk;
            super::super::input::resync_bindings(&ui, &server, &app_config);
            let mut config = app_config.borrow_mut();
            config.hotkeys.toggle_route = hk;
            save_queue.schedule();
            ui.set_listening_hotkey(false);
            ui.set_hotkey_label(hk.label().into());
        });
    }
    register_rows(ui, state);
}

/// 上一首/下一首/停止播放三行的录制与重置。
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
        ui.on_hotkey_row_captured(move |row, text, ctrl, shift, alt| {
            let Some(ui) = ui_weak.upgrade() else { return };
            set_row_listening(&ui, row, false);
            if text == "\u{1b}" {
                return;
            }
            let Some(vk) = config::slint_key_to_vk(text.as_str()) else {
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
            if mods == 0 {
                ui.set_error_text("单键全局热键会拦截正常输入，建议加 Ctrl 或 Alt".into());
                return;
            }
            if let Some(reason) = row_conflict(&app_config, row, hk) {
                ui.set_error_text(reason.into());
                return;
            }
            {
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
        ui.on_reset_hotkey_row(move |row| {
            let Some(ui) = ui_weak.upgrade() else { return };
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

/// 刷新三行的显示文案。
pub(crate) fn refresh_row_labels(ui: &MainWindow, app_config: &Rc<RefCell<config::AppConfig>>) {
    let config = app_config.borrow();
    let state = ui.global::<HotkeyState>();
    state.set_previous(
        config
            .hotkeys
            .previous_track
            .map(|hk| hk.label())
            .unwrap_or_default()
            .into(),
    );
    state.set_next(
        config
            .hotkeys
            .next_track
            .map(|hk| hk.label())
            .unwrap_or_default()
            .into(),
    );
    state.set_stop(
        config
            .hotkeys
            .stop_playback
            .map(|hk| hk.label())
            .unwrap_or_default()
            .into(),
    );
}

/// 行热键与其它全局绑定的重复检测。
fn row_conflict(
    app_config: &Rc<RefCell<config::AppConfig>>,
    row: i32,
    binding: config::Hotkey,
) -> Option<String> {
    let config = app_config.borrow();
    let same = |other: &config::Hotkey| other.mods == binding.mods && other.vk == binding.vk;
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
    if config
        .audio_entries
        .iter()
        .any(|entry| entry.hotkey_binding.as_ref().is_some_and(same))
    {
        return Some("该快捷键已分配给某个音频条目".into());
    }
    None
}
