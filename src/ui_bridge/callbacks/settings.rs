//! settings 相关 Slint 回调注册。

use super::super::*;
use crate::vbcable;
use std::thread;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
    register_3(ui, state);
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
                            ui.set_error_text(e.user_message().into());
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
    let cache = state.cache.clone();
    let hotkey = state.hotkey.clone();
    let hotkey_server = state.hotkey_server.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let hotkey = hotkey.clone();
        let server = hotkey_server.clone();
        let app_config = app_config.clone();
        let cache = cache.clone();
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
            if let Some(index) = cache.borrow_mut().audio_hotkey_index.take() {
                let category = cache.borrow().audio_category.clone();
                let search = cache.borrow().audio_search.to_ascii_lowercase();
                let mut cfg = app_config.borrow_mut();
                if let Some(entry) = cfg
                    .audio_entries
                    .iter_mut()
                    .filter(|e| category_matches(e, &category))
                    .filter(|e| search.is_empty() || e.name.to_ascii_lowercase().contains(&search))
                    .nth(index)
                {
                    entry.hotkey = Some(hk);
                }
                save_queue.schedule();
                ui.set_error_text(format!("音频热键已设置：{}", hk.label()).into());
            } else {
                *hotkey.borrow_mut() = hk;
                server.set(hk);
                let mut cfg = app_config.borrow_mut();
                cfg.hotkeys.toggle_route = hk;
                save_queue.schedule();
                ui.set_hotkey_label(hk.label().into());
            }
        });
    }
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let hotkey = hotkey.clone();
        let server = hotkey_server.clone();
        let app_config = app_config.clone();
        ui.on_reset_hotkey(move || {
            let hk = config::Hotkey::default();
            *hotkey.borrow_mut() = hk;
            server.set(hk);
            let mut config = app_config.borrow_mut();
            config.hotkeys.toggle_route = hk;
            save_queue.schedule();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_listening_hotkey(false);
                ui.set_hotkey_label(hk.label().into());
            }
        });
    }
}

fn register_3(ui: &MainWindow, state: &AppState) {
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let app_config = app_config.clone();
        ui.on_playback_key_mode_toggled(move |mode| {
            let mut config = app_config.borrow_mut();
            let current_key = config.playback_key.map(|a| a.key).unwrap_or_default();
            let mode = match mode {
                1 => config::PlaybackKeyMode::Press,
                2 => config::PlaybackKeyMode::Hold,
                _ => config::PlaybackKeyMode::None,
            };
            config.playback_key = Some(config::PlaybackKeyAction {
                mode,
                key: current_key,
            });
            save_queue.schedule();
        });
    }
}
