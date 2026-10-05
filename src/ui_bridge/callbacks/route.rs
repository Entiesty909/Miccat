//! route 相关 Slint 回调注册。
mod refresh;
mod sources;

use super::super::*;
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    refresh::register(ui, state);
    register_1(ui, state);
    register_2(ui, state);
    sources::register(ui, state);
}

fn register_1(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let engine = engine.clone();
        let app_config = app_config.clone();
        ui.on_master_volume_edited(move |v| {
            let value = (v / 100.0).clamp(0.0, 1.0);
            engine.set_master_volume(value);
            let mut config = app_config.borrow_mut();
            config.mix.master = value;
            save_queue.schedule();
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        let save_queue = state.save_queue.clone();
        ui.on_all_devices_toggled(move |on| {
            let Some(ui) = ui_weak.upgrade() else { return };
            cache.borrow_mut().all_devices = on;
            app_config.borrow_mut().output.all_devices = on;
            save_queue.schedule();
            ui.invoke_refresh();
        });
    }
    {
        let cache = cache.clone();
        let app_config = app_config.clone();
        let save_queue = state.save_queue.clone();
        ui.on_device_toggled(move |index, checked| {
            let mut cache = cache.borrow_mut();
            if let Some(slot) = cache.device_selected.get_mut(index as usize) {
                *slot = checked;
            }
            cache.selected_device_ids = cache
                .dests
                .iter()
                .zip(&cache.device_selected)
                .filter(|(_, selected)| **selected)
                .map(|(device, _)| device.render_id.clone())
                .collect();
            app_config.borrow_mut().output.device_ids = cache.selected_device_ids.clone();
            save_queue.schedule();
        });
    }
    {
        let cache = cache.clone();
        let app_config = app_config.clone();
        let save_queue = state.save_queue.clone();
        ui.on_set_default_mic_toggled(move |on| {
            cache.borrow_mut().set_default_mic = on;
            app_config.borrow_mut().output.set_default_mic = on;
            save_queue.schedule();
        });
    }
}

fn register_2(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    {
        let app_config = app_config.clone();
        let cache = cache.clone();
        let save_queue = save_queue.clone();
        ui.on_set_default_render_toggled(move |on| {
            cache.borrow_mut().set_default_render = on;
            app_config.borrow_mut().output.set_default_render = on;
            save_queue.schedule();
        });
    }
    {
        let app_config = app_config.clone();
        let cache = cache.clone();
        let save_queue = save_queue.clone();
        ui.on_set_default_communications_toggled(move |on| {
            cache.borrow_mut().set_default_communications = on;
            app_config.borrow_mut().output.set_default_communications = on;
            save_queue.schedule();
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        let save_queue = save_queue.clone();
        ui.on_process_add(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let index = ui.get_process_index().max(0) as usize;
            let added = {
                let mut cache = cache.borrow_mut();
                if let Some(pid) = cache.pids.get(index).copied().filter(|pid| *pid != 0)
                    && !cache.selected_pids.contains(&pid)
                {
                    cache.selected_pids.push(pid);
                    true
                } else {
                    false
                }
            };
            if added {
                super::super::persist::save_process_selection(&cache, &app_config, &save_queue);
            }
        });
    }
    {
        let engine = state.engine.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let save_queue = save_queue.clone();
        let app_config = app_config.clone();
        ui.on_mic_mix_changed(move |on| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mic_id = cache
                .borrow()
                .mic_ids
                .get(ui.get_mic_index().max(0) as usize)
                .filter(|id| !id.is_empty())
                .cloned();
            let mix_mic = on && mic_id.is_some();
            if mix_mic {
                cache.borrow_mut().mic_source_hidden = false;
            }
            ui.set_mix_mic(mix_mic);
            let mut config = app_config.borrow_mut();
            config.route.mix_mic = mix_mic;
            config.route.mic_id = mic_id.clone();
            save_queue.schedule();
            drop(config);
            // 运行中热切换麦克风：停旧开新，复用已建立的输出。
            if ui.get_running() {
                let source = if mix_mic { mic_id.as_deref() } else { None };
                if let Err(error) = engine.replace_mic_source(source) {
                    tracing::warn!("热切换麦克风失败：{error}");
                    super::super::diagnostics::report_error(&ui, &error);
                }
            }
        });
    }
}
