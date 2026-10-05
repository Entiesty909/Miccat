//! 音频库行选择、分类修改、分页、排序、搜索和应用内快捷键。

use super::super::library::visible_rank;
use super::super::*;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_selection(ui, state);
    register_category(ui, state);
    register_page(ui, state);
    register_sort(ui, state);
    register_search(ui, state);
    register_audio_shortcuts(ui, state);
}

fn register_selection(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_selected(move |visible_index| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let Some(actual_index) = visible_entry(&cache, visible_index.max(0) as usize) else {
            return;
        };
        let entry_state = {
            let cache = cache.borrow();
            let Some(entry) = cache.audio_entries.get(actual_index) else {
                return;
            };
            (entry.category.clone(), entry.loop_playback)
        };
        if let Some(category_index) = category_paths(&cache)
            .iter()
            .position(|path| path == &entry_state.0)
        {
            ui.set_audio_entry_category_index(category_index as i32);
        }
        ui.set_player_loop(entry_state.1);
        ui.set_audio_entry_index(visible_index);
    });
}

fn register_category(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let ui_weak = ui.as_weak();
    ui.on_entry_category_changed(move |category_index| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let visible_index = ui.get_audio_entry_index().max(0) as usize;
        let Some(actual_index) = visible_entry(&cache, visible_index) else {
            return;
        };
        let Some(category) = category_paths(&cache)
            .get(category_index.max(0) as usize)
            .cloned()
        else {
            return;
        };
        {
            let mut cache_state = cache.borrow_mut();
            let Some(entry) = cache_state.audio_entries.get_mut(actual_index) else {
                return;
            };
            entry.category = category;
            app_config.borrow_mut().audio_entries = cache_state.audio_entries.clone();
        }
        save_queue.schedule();
        update_audio_list(&ui, &cache);
        ui.set_audio_entry_index(visible_index as i32);
    });
}

fn register_page(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_page_changed(move |page| {
        let Some(ui) = ui_weak.upgrade() else { return };
        cache.borrow_mut().audio_page = page.max(0) as usize;
        update_audio_list(&ui, &cache);
    });
}

fn register_sort(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_sort_toggled(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        {
            let mut cache = cache.borrow_mut();
            cache.audio_sort_ascending = !cache.audio_sort_ascending;
            cache.audio_page = 0;
        }
        update_audio_list(&ui, &cache);
    });
}

fn register_search(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_search_edited(move |search| {
        let Some(ui) = ui_weak.upgrade() else { return };
        {
            let mut cache = cache.borrow_mut();
            cache.audio_search = search.to_string();
            cache.audio_page = 0;
        }
        update_audio_list(&ui, &cache);
    });
}

fn register_audio_shortcuts(ui: &MainWindow, state: &AppState) {
    register_audio_shortcut_capture(ui, state);
    register_audio_shortcut_dispatch(ui, state);
}

fn register_audio_shortcut_capture(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let hotkey_server = state.hotkey_server.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_hotkey_bind(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_audio_hotkey_listening(ui.get_audio_entry_index() >= 0);
            ui.set_error_text("按下新的全局快捷键（可带 Ctrl/Alt/Shift）".into());
            super::super::diagnostics::report_info(&ui, "按下新的全局快捷键");
        }
    });

    let cache = cache.clone();
    let app_config = app_config.clone();
    let save_queue = save_queue.clone();
    let hotkey_server = hotkey_server.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_hotkey_binding_captured(move |text, ctrl, shift, alt| {
        let Some(ui) = ui_weak.upgrade() else { return };
        ui.set_audio_hotkey_listening(false);
        if text == "\u{1b}" {
            ui.set_error_text("".into());
            return;
        }
        let Some(vk) = crate::config::slint_key_to_vk(text.as_str()) else {
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
        let binding = config::Hotkey { mods, vk };
        // 无修饰键的单键做成全局会吞掉正常打字，先给出明确警告。
        if mods == 0 {
            ui.set_error_text("单键全局热键会拦截正常输入，建议加 Ctrl 或 Alt".into());
            return;
        }
        let visible_index = ui.get_audio_entry_index().max(0) as usize;
        let Some(actual_index) = visible_entry(&cache, visible_index) else {
            return;
        };
        if let Some(reason) = binding_conflict(&app_config, actual_index, binding) {
            ui.set_error_text(reason.into());
            return;
        }
        {
            let mut cache_state = cache.borrow_mut();
            let Some(entry) = cache_state.audio_entries.get_mut(actual_index) else {
                return;
            };
            entry.hotkey_binding = Some(binding);
            app_config.borrow_mut().audio_entries = cache_state.audio_entries.clone();
        }
        save_queue.schedule();
        // 立即重注册，并检查是否被外部程序占用。
        super::super::input::resync_bindings(&ui, &hotkey_server, &app_config);
        update_audio_list(&ui, &cache);
        ui.set_audio_entry_index(visible_index as i32);
    });
}

/// 检查新绑定是否与已有全局热键重复；重复时返回用户可见原因。
///
/// 按 `(mods, vk)` 全等比较：只比 vk 会把 `Ctrl+A` 与 `A` 误判成冲突，
/// 也会漏掉「同键不同修饰符」的真重复。
fn binding_conflict(
    app_config: &Rc<RefCell<config::AppConfig>>,
    actual_index: usize,
    binding: config::Hotkey,
) -> Option<String> {
    let config = app_config.borrow();
    let same = |other: &config::Hotkey| other.mods == binding.mods && other.vk == binding.vk;
    if same(&config.hotkeys.toggle_route) {
        return Some("与「开始/停止转发」热键重复".into());
    }
    if config.hotkeys.previous_track.as_ref().is_some_and(same) {
        return Some("与「上一首」热键重复".into());
    }
    if config.hotkeys.next_track.as_ref().is_some_and(same) {
        return Some("与「下一首」热键重复".into());
    }
    if config.hotkeys.stop_playback.as_ref().is_some_and(same) {
        return Some("与「停止播放」热键重复".into());
    }
    let clash = config
        .audio_entries
        .iter()
        .enumerate()
        .any(|(index, entry)| {
            index != actual_index && entry.hotkey_binding.as_ref().is_some_and(&same)
        });
    if clash {
        return Some("该快捷键已分配给其他音频".into());
    }
    None
}

fn register_audio_shortcut_dispatch(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let engine = state.engine.clone();
    let ui_weak = ui.as_weak();
    // 全局热键按条目下标直接触发播放。
    ui.on_audio_hotkey_index(move |index| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let actual = index.max(0) as usize;
        let category = {
            let cache_state = cache.borrow();
            let Some(entry) = cache_state.audio_entries.get(actual) else {
                return;
            };
            entry.category.clone()
        };
        let categories = category_paths(&cache);
        if let Some(category_index) = categories.iter().position(|path| path == &category) {
            ui.set_audio_category_index(category_index as i32);
            cache.borrow_mut().audio_category = category;
        }
        let target = cache.borrow().playback_target;
        cache.borrow_mut().paused_audio = None;
        engine.stop_file();
        if let Some(entry) = cache.borrow().audio_entries.get(actual).cloned() {
            super::player::play_entry(&ui, &engine, &entry, target);
        }
    });

    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_hotkey_pressed(move |key| {
        let Some(hotkey) = shortcut_label(key.as_str()) else {
            return;
        };
        let Some(actual_index) = cache.borrow().audio_entries.iter().position(|entry| {
            entry
                .hotkey
                .as_deref()
                .is_some_and(|bound| bound.eq_ignore_ascii_case(&hotkey))
        }) else {
            return;
        };
        let Some(ui) = ui_weak.upgrade() else { return };
        let category = cache.borrow().audio_entries[actual_index].category.clone();
        let categories = category_paths(&cache);
        let Some(category_index) = categories.iter().position(|path| path == &category) else {
            return;
        };
        {
            let mut cache_state = cache.borrow_mut();
            cache_state.audio_category = category;
            cache_state.audio_search.clear();
            cache_state.audio_page = 0;
            cache_state.force_audio_play = true;
            cache_state.playback_target = crate::engine::PlaybackTarget::Both;
        }
        let rank = visible_rank(&cache, actual_index).unwrap_or(0);
        cache.borrow_mut().audio_page = rank / super::super::library::PAGE_SIZE;
        ui.set_page(1);
        ui.set_audio_category_index(category_index as i32);
        update_audio_list(&ui, &cache);
        let visible_index =
            visible_rank(&cache, actual_index).unwrap_or(0) % super::super::library::PAGE_SIZE;
        ui.set_audio_entry_index(visible_index as i32);
        ui.invoke_audio_entry_play(0);
    });
}

fn shortcut_label(key: &str) -> Option<String> {
    let virtual_key = crate::config::slint_key_to_vk(key)?;
    match virtual_key {
        0x20 => Some("Space".to_string()),
        0x70..=0x7b => Some(format!("F{}", virtual_key - 0x70 + 1)),
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(virtual_key).map(|key| key.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::shortcut_label;

    #[test]
    fn shortcut_capture_normalizes_function_and_character_keys() {
        assert_eq!(shortcut_label("\u{F001}"), Some("F1".into()));
        assert_eq!(shortcut_label("a"), Some("A".into()));
        assert_eq!(shortcut_label(" "), Some("Space".into()));
        assert_eq!(shortcut_label("Escape"), None);
    }
}
