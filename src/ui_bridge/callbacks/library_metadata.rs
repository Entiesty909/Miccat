//! library_metadata 相关 Slint 回调注册。

use super::super::*;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
}

fn register_1(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        ui.on_audio_entry_remove_hotkey(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let visible = ui.get_audio_entry_index().max(0) as usize;
            let (category, search) = {
                let cache = cache.borrow();
                (
                    cache.audio_category.clone(),
                    cache.audio_search.to_ascii_lowercase(),
                )
            };
            let actual = {
                let config = app_config.borrow();
                visible_entry_in(&config.audio_entries, &category, &search, visible)
            };
            let mut config = app_config.borrow_mut();
            if let Some(actual) = actual {
                if let Some(entry) = config.audio_entries.get_mut(actual) {
                    entry.hotkey = None;
                }
            }
            save_queue.schedule();
            ui.set_error_text("音频热键已移除".into());
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_entry_edit_tags(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let visible = ui.get_audio_entry_index().max(0) as usize;
                if let Some(index) = visible_entry(&cache, visible) {
                    let c = cache.borrow();
                    if let Some(entry) = c.audio_entries.get(index) {
                        ui.set_audio_entry_edit_name(entry.name.clone().into());
                    }
                }
            }
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
        ui.on_audio_entry_save_tags(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let visible = ui.get_audio_entry_index().max(0) as usize;
            let name = ui.get_audio_entry_edit_name().trim().to_string();
            if name.is_empty() {
                return;
            }
            let actual = visible_entry(&cache, visible);
            let mut c = cache.borrow_mut();
            if let Some(actual) = actual {
                if let Some(entry) = c.audio_entries.get_mut(actual) {
                    entry.name = name;
                }
            }
            let entries = c.audio_entries.clone();
            drop(c);
            app_config.borrow_mut().audio_entries = entries;
            save_queue.schedule();
            update_audio_list(&ui, &cache);
            ui.set_error_text("音频名称已更新".into());
        });
    }
}
