//! library_entries 相关 Slint 回调注册。

use super::super::*;

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
            let validate_info = match crate::audio::decode::validate_file(&path) {
                Ok(info) => info,
                Err(error) => {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_error_text(error.user_message().into());
                    }
                    return;
                }
            };
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("音频")
                .to_string();
            let entry = config::AudioEntry {
                name,
                path: path.to_string_lossy().into_owned(),
                category: "未分类".into(),
                duration_hns: validate_info.duration_hns,
                loop_playback: false,
                hotkey: None,
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
        ui.on_audio_search_changed(move |text| {
            cache.borrow_mut().audio_search = text.to_string();
            if let Some(ui) = ui_weak.upgrade() {
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
}
