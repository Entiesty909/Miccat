//! library_entries 相关 Slint 回调注册。

use super::super::*;
use slint::Model;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
    register_3(ui, state);
}

fn register_1(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        ui.on_add_audio_files(move || {
            let Some(path) = select_audio_file() else {
                return;
            };
            let validate_info = match crate::library::validate_file(&path) {
                Ok(info) => info,
                Err(error) => {
                    if let Some(ui) = ui_weak.upgrade() {
                        super::super::diagnostics::report_error(&ui, &error);
                    }
                    return;
                }
            };
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("音频")
                .to_string();
            let category = {
                let cache_state = cache.borrow();
                crate::library::import_category(&cache_state.audio_category)
            };
            let entry = config::AudioEntry {
                name,
                path: path.to_string_lossy().into_owned(),
                category,
                icon_path: None,
                icon_glyph: "\u{EC4F}".into(),
                duration_hns: validate_info.duration_hns,
                file_size_bytes: std::fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0),
                sample_rate: validate_info.sample_rate,
                channels: validate_info.channels,
                loop_playback: false,
                hotkey: None,
                hotkey_binding: None,
            };
            cache.borrow_mut().audio_entries.push(entry);
            let entries = cache.borrow().audio_entries.clone();
            app_config.borrow_mut().audio_entries = entries;
            save_queue.schedule();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_error_text("".into());
                update_audio_list(&ui, &cache);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_entry_context(move |index| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            ui.set_audio_entry_index(index);
            ui.set_audio_category_menu_open(false);
            cache.borrow_mut().last_audio_click = None;
        });
    }
}

fn register_2(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        ui.on_audio_entry_remove(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let visible = ui.get_audio_entry_index().max(0) as usize;
            let actual = visible_entry(&cache, visible);
            if let Some(actual) = actual {
                let mut c = cache.borrow_mut();
                c.audio_entries.remove(actual);
                let entries = c.audio_entries.clone();
                drop(c);
                app_config.borrow_mut().audio_entries = entries;
                save_queue.schedule();
                update_audio_list(&ui, &cache);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_entry_explorer_rename(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let index = ui.get_audio_entry_index().max(0) as usize;
            let Some(actual) = visible_entry(&cache, index) else {
                return;
            };
            let c = cache.borrow();
            if let Some(entry) = c.audio_entries.get(actual) {
                let _ = std::process::Command::new("explorer.exe")
                    .args(["/select,", &entry.path])
                    .spawn();
            }
        });
    }
}

fn register_3(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_entry_edit_file(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let index = ui.get_audio_entry_index().max(0) as usize;
            let Some(actual) = visible_entry(&cache, index) else {
                return;
            };
            let c = cache.borrow();
            if let Some(entry) = c.audio_entries.get(actual) {
                let _ = std::process::Command::new("explorer.exe")
                    .arg(&entry.path)
                    .spawn();
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let state_for_update = state.clone();
        ui.on_audio_entry_add_route_index(move |actual_index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(actual_index) = usize::try_from(actual_index).ok() else {
                return;
            };
            let replace_index = ui.get_source_replace_index();
            let mut cache_state = cache.borrow_mut();
            if replace_index >= 0 {
                let source_kinds = ui.get_source_kinds();
                let source_index = replace_index as usize;
                let file_slot = (0..source_index)
                    .filter(|row| source_kinds.row_data(*row) == Some(1))
                    .count();
                if source_kinds.row_data(source_index) != Some(1)
                    || file_slot >= cache_state.route_audio_entries.len()
                {
                    return;
                }
                if super::route::sources::duplicate_audio(
                    &cache_state.route_audio_entries,
                    actual_index,
                    Some(file_slot),
                ) {
                    ui.set_error_text("该音频已经添加，不能创建重复音频源".into());
                    return;
                }
                cache_state.route_audio_entries[file_slot] = actual_index;
                if file_slot < cache_state.route_audio_enabled.len() {
                    cache_state.route_audio_enabled[file_slot] = false;
                }
            } else {
                if super::route::sources::duplicate_audio(
                    &cache_state.route_audio_entries,
                    actual_index,
                    None,
                ) {
                    ui.set_error_text("该音频已经添加，不能创建重复音频源".into());
                    return;
                }
                cache_state.route_audio_entries.push(actual_index);
                cache_state.route_audio_enabled.push(false);
                cache_state.audio_volumes.push(app_config.borrow().mix.sfx);
                cache_state.audio_muted_volumes.push(None);
                let output_id = cache_state
                    .dests
                    .get(ui.get_file_output_index().max(0) as usize)
                    .map(|device| device.render_id.clone());
                cache_state.audio_output_ids.push(output_id);
            }
            cache_state.route_audio_entry = Some(actual_index);
            cache_state.route_file_enabled = false;
            cache_state.file_source_hidden = false;
            drop(cache_state);
            super::super::persist::save_process_selection(
                &cache,
                &app_config,
                &state_for_update.save_queue,
            );
            ui.set_source_replace_index(-1);
            ui.set_page(0);
            super::super::sources::update(&ui, &state_for_update);
        });
    }
    {
        // 清除该条目的快捷键；此前没有任何解绑入口。
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let hotkey_server = state.hotkey_server.clone();
        ui.on_audio_entry_hotkey_clear(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let index = ui.get_audio_entry_index().max(0) as usize;
            let Some(actual) = visible_entry(&cache, index) else {
                return;
            };
            {
                let mut cache_state = cache.borrow_mut();
                let Some(entry) = cache_state.audio_entries.get_mut(actual) else {
                    return;
                };
                // 两个字段都要清：旧的「前台单键」和新的「全局组合键」。
                entry.hotkey = None;
                entry.hotkey_binding = None;
                app_config.borrow_mut().audio_entries = cache_state.audio_entries.clone();
            }
            save_queue.schedule();
            // 全局绑定表也要重同步，否则组合键仍在系统里生效。
            super::super::input::resync_bindings(&ui, &hotkey_server, &app_config);
            update_audio_list(&ui, &cache);
            ui.set_audio_entry_index(index as i32);
        });
    }
}
