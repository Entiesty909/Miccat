//! route 相关 Slint 回调注册。

use super::super::*;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
}

fn register_1(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let engine = engine.clone();
        let app_config = app_config.clone();
        ui.on_volume_edited(move |v| {
            let value = (v / 100.0).clamp(0.0, 1.0);
            engine.set_volume(value);
            let mut config = app_config.borrow_mut();
            config.mix.process = value;
            save_queue.schedule();
        });
    }
    {
        let save_queue = state.save_queue.clone();
        let engine = engine.clone();
        let app_config = app_config.clone();
        ui.on_mic_volume_edited(move |v| {
            let value = (v / 100.0).clamp(0.0, 1.0);
            engine.set_mic_volume(value);
            let mut config = app_config.borrow_mut();
            config.mix.mic = value;
            save_queue.schedule();
        });
    }
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
        ui.on_process_filter_changed(move |text| {
            let Some(ui) = ui_weak.upgrade() else { return };
            cache.borrow_mut().filter = text.to_string();
            refill_processes(&ui, &cache);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        ui.on_all_devices_toggled(move |on| {
            let Some(ui) = ui_weak.upgrade() else { return };
            cache.borrow_mut().all_devices = on;
            app_config.borrow_mut().output.all_devices = on;
            refill_devices(&ui, &cache, false);
        });
    }
    {
        let cache = cache.clone();
        ui.on_device_toggled(move |index, checked| {
            if let Some(slot) = cache.borrow_mut().device_selected.get_mut(index as usize) {
                *slot = checked;
            }
        });
    }
    {
        let cache = cache.clone();
        let app_config = app_config.clone();
        ui.on_set_default_mic_toggled(move |on| {
            cache.borrow_mut().set_default_mic = on;
            app_config.borrow_mut().output.set_default_mic = on;
        });
    }
}

fn register_2(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let app_config = app_config.clone();
        let cache = cache.clone();
        ui.on_set_default_render_toggled(move |on| {
            cache.borrow_mut().set_default_render = on;
            app_config.borrow_mut().output.set_default_render = on;
        });
    }
    {
        let app_config = app_config.clone();
        let cache = cache.clone();
        ui.on_set_default_communications_toggled(move |on| {
            cache.borrow_mut().set_default_communications = on;
            app_config.borrow_mut().output.set_default_communications = on;
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_process_add(move || {
            let index = ui_weak
                .upgrade()
                .map(|ui| ui.get_process_index())
                .unwrap_or(0) as usize;
            let mut c = cache.borrow_mut();
            if let Some(pid) = c.pids.get(index).copied().filter(|pid| *pid != 0) {
                if !c.selected_pids.contains(&pid) {
                    c.selected_pids.push(pid);
                }
                let labels: Vec<String> = c
                    .selected_pids
                    .iter()
                    .filter_map(|pid| {
                        c.processes
                            .iter()
                            .find(|p| p.pid == *pid)
                            .map(|p| format!("{} [{}]", p.name, p.pid))
                    })
                    .collect();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_selected_processes(labels.join("、").into());
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_process_clear(move || {
            cache.borrow_mut().selected_pids.clear();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_selected_processes("".into());
            }
        });
    }
    {
        let save_queue = state.save_queue.clone();
        let app_config = app_config.clone();
        ui.on_mix_mic_toggled(move |on| {
            let mut config = app_config.borrow_mut();
            config.route.mix_mic = on;
            save_queue.schedule();
        });
    }
}
