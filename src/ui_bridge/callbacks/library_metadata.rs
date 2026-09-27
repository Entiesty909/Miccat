//! 音频库条目名称编辑回调。

use super::super::*;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_edit_name(ui, state);
    register_save_name(ui, state);
}

fn register_edit_name(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_edit_tags(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let visible = ui.get_audio_entry_index().max(0) as usize;
            if let Some(index) = visible_entry(&cache, visible) {
                let cache = cache.borrow();
                if let Some(entry) = cache.audio_entries.get(index) {
                    ui.set_audio_entry_edit_name(entry.name.clone().into());
                }
            }
        }
    });
}

fn register_save_name(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let ui_weak = ui.as_weak();
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
        let mut cache_state = cache.borrow_mut();
        if let Some(actual) = actual
            && let Some(entry) = cache_state.audio_entries.get_mut(actual)
        {
            entry.name = name;
        }
        let entries = cache_state.audio_entries.clone();
        drop(cache_state);
        app_config.borrow_mut().audio_entries = entries;
        save_queue.schedule();
        update_audio_list(&ui, &cache);
        ui.set_error_text("音频名称已更新".into());
    });
}
