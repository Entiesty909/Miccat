//! library 相关 Slint 回调注册。

use super::super::*;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
    register_3(ui, state);
    register_drag(ui, state);
    register_loop(ui, state);
    register_edit_dialogs(ui, state);
}

fn register_1(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_category_changed(move |index| {
            let category = category_paths(&cache).get(index as usize).cloned();
            let mut view = cache.borrow_mut();
            view.audio_page = 0;
            if let Some(category) = category {
                view.audio_category = category;
            }
            drop(view);
            if let Some(ui) = ui_weak.upgrade() {
                update_audio_list(&ui, &cache);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_category_context(move |index| {
            if index < 0 {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_audio_category_index(-1);
                    ui.set_audio_category_edit_name("".into());
                    ui.set_audio_category_menu_open(true);
                }
                return;
            }
            let path = category_paths(&cache).get(index as usize).cloned();
            if let Some(path) = path {
                cache.borrow_mut().audio_category = path.clone();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_audio_category_index(index);
                    ui.set_audio_category_edit_name(category_leaf(&path).into());
                    ui.set_audio_category_menu_open(true);
                    update_audio_list(&ui, &cache);
                }
            }
        });
    }
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        ui.on_audio_category_new(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let selected_index = ui.get_audio_category_index();
            let (categories, new_path) = {
                let cache_state = cache.borrow();
                let selected = if selected_index < 0 {
                    ""
                } else {
                    cache_state.audio_category.as_str()
                };
                let new_path = crate::library::new_category(
                    &cache_state.audio_categories,
                    &cache_state.audio_entries,
                    selected,
                );
                drop(cache_state);
                let mut cache_state = cache.borrow_mut();
                cache_state.audio_categories.push(new_path.clone());
                (cache_state.audio_categories.clone(), new_path)
            };
            {
                let mut cfg = app_config.borrow_mut();
                cfg.audio_categories = categories;
                save_queue.schedule();
            }
            super::super::library::update_audio_categories(&ui, &cache, &app_config.borrow());
            if let Some(index) = category_paths(&cache)
                .iter()
                .position(|path| path == &new_path)
            {
                ui.set_audio_category_index(index as i32);
                cache.borrow_mut().audio_category = new_path;
            }
            update_audio_list(&ui, &cache);
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
        ui.on_category_rename(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let Some(old) = ({
                let selected = cache.borrow().audio_category.clone();
                (!selected.is_empty()).then_some(selected)
            }) else {
                return;
            };
            if old == "全部" || old == "未分类" {
                return;
            }
            let leaf = ui.get_audio_category_edit_name().trim().replace('/', "／");
            if leaf.is_empty() {
                return;
            }
            let new_path = old
                .rsplit_once('/')
                .map(|(parent, _)| format!("{parent}/{leaf}"))
                .unwrap_or(leaf);
            if new_path == old
                || category_paths(&cache)
                    .iter()
                    .any(|path| path != &old && path == &new_path)
            {
                return;
            }
            let (categories, entries, selected_index) = {
                let mut c = cache.borrow_mut();
                let mut categories = c.audio_categories.clone();
                let mut entries = c.audio_entries.clone();
                crate::library::rename_category(&mut categories, &mut entries, &old, &new_path);
                c.audio_categories = categories.clone();
                c.audio_entries = entries.clone();
                c.audio_category = new_path.clone();
                let paths = crate::library::category_paths(&categories, &entries);
                let selected_index = paths
                    .iter()
                    .position(|path| path == &new_path)
                    .map_or(-1, |index| index as i32);
                (categories, entries, selected_index)
            };
            let mut cfg = app_config.borrow_mut();
            cfg.audio_categories = categories;
            cfg.audio_entries = entries;
            save_queue.schedule();
            drop(cfg);
            ui.set_audio_category_index(selected_index);
            ui.set_audio_category_edit_name(category_leaf(&new_path).into());
            super::super::library::update_audio_categories(&ui, &cache, &app_config.borrow());
            update_audio_list(&ui, &cache);
        });
    }
}

fn register_3(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        ui.on_category_delete(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let Some(name) = ({
                let selected = cache.borrow().audio_category.clone();
                (!selected.is_empty()).then_some(selected)
            }) else {
                return;
            };
            let (categories, entries, selected_index) = {
                let mut c = cache.borrow_mut();
                let mut categories = c.audio_categories.clone();
                let mut entries = c.audio_entries.clone();
                crate::library::delete_category(&mut categories, &mut entries, &name);
                c.audio_categories = categories.clone();
                c.audio_entries = entries.clone();
                let paths = crate::library::category_paths(&categories, &entries);
                let selected_category = paths.first().cloned().unwrap_or_default();
                let selected_index = if paths.is_empty() { -1 } else { 0 };
                c.audio_category = selected_category;
                (categories, entries, selected_index)
            };
            let mut cfg = app_config.borrow_mut();
            cfg.audio_categories = categories;
            cfg.audio_entries = entries;
            crate::library::delete_category_icon_paths(&mut cfg.audio_category_icons, &name);
            save_queue.schedule();
            drop(cfg);
            ui.set_audio_category_index(selected_index);
            ui.set_audio_category_edit_name("".into());
            ui.set_audio_category_menu_open(false);
            super::super::library::update_audio_categories(&ui, &cache, &app_config.borrow());
            update_audio_list(&ui, &cache);
        });
    }
}

fn register_drag(ui: &MainWindow, state: &AppState) {
    let state_for_drop = state.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_category_to_transfer(|index| {
        let payload = slint::SharedString::from(format!("miccat-category:{index}"));
        slint::DataTransfer::from(payload)
    });
    ui.on_audio_category_can_drop({
        let cache = state.cache.clone();
        move |data, target_index, position| can_drop_category(&data, target_index, position, &cache)
    });
    ui.on_audio_category_drop(move |data, target_index, position| {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        commit_category_drop(data, target_index, position, &ui, &state_for_drop);
    });
}

fn can_drop_category(
    data: &slint::DataTransfer,
    target_index: i32,
    position: i32,
    cache: &std::rc::Rc<std::cell::RefCell<UiCache>>,
) -> bool {
    let Some(source_index) = category_drag_index(data) else {
        return false;
    };
    let Some(position) = category_drop_position(position) else {
        return false;
    };
    let paths = category_paths(cache);
    let Some(source) = paths.get(source_index) else {
        return false;
    };
    let Some(target) = paths.get(target_index as usize) else {
        return false;
    };
    let cache = cache.borrow();
    crate::library::can_move_category(
        &cache.audio_categories,
        &cache.audio_entries,
        source,
        target,
        position,
    )
}

fn commit_category_drop(
    data: slint::DataTransfer,
    target_index: i32,
    position: i32,
    ui: &MainWindow,
    state: &AppState,
) {
    let Some(source_index) = category_drag_index(&data) else {
        return;
    };
    let Some(position) = category_drop_position(position) else {
        return;
    };
    let (source, target) = {
        let paths = category_paths(&state.cache);
        let Some(source) = paths.get(source_index).cloned() else {
            return;
        };
        let Some(target) = paths.get(target_index as usize).cloned() else {
            return;
        };
        (source, target)
    };
    let (categories, entries, selected, selected_index) = {
        let mut cache_state = state.cache.borrow_mut();
        let UiCache {
            audio_categories,
            audio_entries,
            audio_category,
            ..
        } = &mut *cache_state;
        let Some(destination) = crate::library::move_category(
            audio_categories,
            audio_entries,
            &source,
            &target,
            position,
        ) else {
            return;
        };
        let prefix = format!("{source}/");
        let selected = if audio_category.as_str() == source {
            destination.clone()
        } else if audio_category.starts_with(&prefix) {
            format!("{destination}{}", &audio_category[source.len()..])
        } else {
            audio_category.clone()
        };
        *audio_category = selected.clone();
        let paths = crate::library::category_paths(audio_categories, audio_entries);
        let selected_index = paths
            .iter()
            .position(|path| path == &selected)
            .map_or(-1, |index| index as i32);
        (
            audio_categories.clone(),
            audio_entries.clone(),
            selected,
            selected_index,
        )
    };
    {
        let mut config = state.app_config.borrow_mut();
        config.audio_categories = categories;
        config.audio_entries = entries;
        state.save_queue.schedule();
    }
    super::super::library::update_audio_categories(ui, &state.cache, &state.app_config.borrow());
    ui.set_audio_category_index(selected_index);
    if selected.is_empty() {
        ui.set_audio_category_index(-1);
    }
    update_audio_list(ui, &state.cache);
}

fn category_drag_index(data: &slint::DataTransfer) -> Option<usize> {
    let payload = data.plain_text().ok()?;
    payload.strip_prefix("miccat-category:")?.parse().ok()
}

fn category_drop_position(value: i32) -> Option<crate::library::CategoryDropPosition> {
    match value {
        0 => Some(crate::library::CategoryDropPosition::Before),
        1 => Some(crate::library::CategoryDropPosition::Into),
        2 => Some(crate::library::CategoryDropPosition::After),
        _ => None,
    }
}

fn register_loop(ui: &MainWindow, state: &AppState) {
    {
        let engine = state.engine.clone();
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        ui.on_audio_loop_changed(move |looped| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let index = ui.get_audio_entry_index().max(0) as usize;
            let Some(actual) = visible_entry(&cache, index) else {
                return;
            };
            let Some(entry) = cache.borrow().audio_entries.get(actual).cloned() else {
                return;
            };
            let state = engine.file_state();
            let mut updated = entry;
            updated.loop_playback = looped;
            cache.borrow_mut().audio_entries[actual] = updated.clone();
            app_config.borrow_mut().audio_entries = cache.borrow().audio_entries.clone();
            save_queue.schedule();
            if state.playing && state.path == updated.path {
                let target = cache.borrow().playback_target;
                if engine
                    .play_file(&updated, state.position_hns, target)
                    .is_ok()
                {
                    ui.set_player_paused(false);
                }
            }
        });
    }
}

fn register_edit_dialogs(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let edit_window = state.windows.audio_edit.clone();
    let cache_for_category = cache.clone();
    let config_for_category = app_config.clone();
    let edit_for_category = edit_window.clone();
    let state_for_category = state.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_category_edit_requested(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let cache_state = cache_for_category.borrow();
        let Some(path) =
            (!cache_state.audio_category.is_empty()).then_some(cache_state.audio_category.clone())
        else {
            return;
        };
        if path == "全部" || path == "未分类" {
            return;
        }
        let icon = config_for_category
            .borrow()
            .audio_category_icons
            .get(&path)
            .cloned();
        ui.set_audio_edit_category_mode(true);
        ui.set_audio_edit_name(category_leaf(&path).into());
        let image_path = icon
            .as_deref()
            .and_then(|value| value.strip_prefix("file:"));
        let image = image_path
            .map(crate::platform::icon::file_icon)
            .unwrap_or_default();
        ui.set_audio_edit_custom_icon_image(image.clone());
        ui.set_audio_edit_icon_index(library::audio_icon_index(icon.as_deref().unwrap_or("")));
        ui.set_audio_edit_custom_icon_path(
            icon.as_deref()
                .and_then(|value| value.strip_prefix("file:"))
                .unwrap_or("")
                .into(),
        );
        edit_for_category.set_category_mode(true);
        edit_for_category.set_edit_title("编辑分类".into());
        edit_for_category.set_name(category_leaf(&path).into());
        edit_for_category.set_custom_icon_image(image);
        edit_for_category.set_icon_index(library::audio_icon_index(icon.as_deref().unwrap_or("")));
        edit_for_category.set_custom_icon_path(
            icon.as_deref()
                .and_then(|value| value.strip_prefix("file:"))
                .unwrap_or("")
                .into(),
        );
        ui.set_audio_edit_dialog_open(true);
        super::super::show_audio_edit_window(&state_for_category);
    });
    let cache_for_entry = state.cache.clone();
    let edit_for_entry = edit_window.clone();
    let state_for_entry = state.clone();
    let ui_weak_entry = ui.as_weak();
    ui.on_audio_entry_edit_requested(move || {
        let Some(ui) = ui_weak_entry.upgrade() else {
            return;
        };
        let visible = ui.get_audio_entry_index().max(0) as usize;
        let Some(actual) = visible_entry(&cache_for_entry, visible) else {
            return;
        };
        let cache_state = cache_for_entry.borrow();
        let Some(entry) = cache_state.audio_entries.get(actual) else {
            return;
        };
        let image = entry
            .icon_path
            .as_deref()
            .map(crate::platform::icon::file_icon)
            .unwrap_or_default();
        let path = entry.icon_path.clone().unwrap_or_default();
        let icon_index = library::audio_icon_index(&entry.icon_glyph);
        ui.set_audio_edit_category_mode(false);
        ui.set_audio_edit_name(entry.name.clone().into());
        ui.set_audio_edit_icon_index(icon_index);
        ui.set_audio_edit_custom_icon_path(path.clone().into());
        ui.set_audio_edit_custom_icon_image(image.clone());
        edit_for_entry.set_category_mode(false);
        edit_for_entry.set_edit_title("编辑音频".into());
        edit_for_entry.set_name(entry.name.clone().into());
        edit_for_entry.set_icon_index(icon_index);
        edit_for_entry.set_custom_icon_path(path.into());
        edit_for_entry.set_custom_icon_image(image);
        ui.set_audio_edit_dialog_open(true);
        super::super::show_audio_edit_window(&state_for_entry);
    });
    let ui_weak_cancel = ui.as_weak();
    ui.on_audio_edit_cancelled(move || {
        if let Some(ui) = ui_weak_cancel.upgrade() {
            ui.set_audio_edit_dialog_open(false);
        }
    });
    let edit_for_icon = edit_window.clone();
    let ui_weak_icon = ui.as_weak();
    ui.on_audio_edit_custom_icon_requested(move || {
        if let Some(ui) = ui_weak_icon.upgrade()
            && let Some(path) = select_icon_file()
        {
            let custom_path = path.to_string_lossy().into_owned();
            let image = crate::platform::icon::file_icon(&custom_path);
            ui.set_audio_edit_custom_icon_image(image.clone());
            ui.set_audio_edit_custom_icon_path(custom_path.clone().into());
            edit_for_icon.set_custom_icon_image(image);
            edit_for_icon.set_custom_icon_path(custom_path.into());
        }
    });
    let ui_weak_accept = ui.as_weak();
    ui.on_audio_edit_accepted(move |name, icon_index, custom_path| {
        let Some(ui) = ui_weak_accept.upgrade() else {
            return;
        };
        let name = name.trim().replace('/', "／");
        let custom_path = custom_path.trim().to_owned();
        if name.is_empty() {
            return;
        }
        if ui.get_audio_edit_category_mode() {
            let old = cache.borrow().audio_category.clone();
            let new_path = old
                .rsplit_once('/')
                .map(|(parent, _)| format!("{parent}/{name}"))
                .unwrap_or(name);
            if library::category_path_conflicts(&old, &new_path, &category_paths(&cache)) {
                return;
            }
            let (categories, entries) = {
                let mut state = cache.borrow_mut();
                let mut categories = state.audio_categories.clone();
                let mut entries = state.audio_entries.clone();
                crate::library::rename_category(&mut categories, &mut entries, &old, &new_path);
                state.audio_categories = categories.clone();
                state.audio_entries = entries.clone();
                state.audio_category = new_path.clone();
                (categories, entries)
            };
            let icon = if custom_path.is_empty() {
                library::audio_icon_value(icon_index, category_leaf(&new_path))
            } else {
                format!("file:{custom_path}")
            };
            let mut config = app_config.borrow_mut();
            crate::library::rename_category_icon_paths(
                &mut config.audio_category_icons,
                &old,
                &new_path,
            );
            config.audio_categories = categories;
            config.audio_entries = entries;
            config.audio_category_icons.insert(new_path, icon);
            library::update_audio_categories(&ui, &cache, &config);
            update_audio_list(&ui, &cache);
            save_queue.schedule();
        } else {
            let visible = ui.get_audio_entry_index().max(0) as usize;
            let Some(actual) = visible_entry(&cache, visible) else {
                return;
            };
            let entries = {
                let mut state = cache.borrow_mut();
                let Some(entry) = state.audio_entries.get_mut(actual) else {
                    return;
                };
                entry.name = name;
                entry.icon_glyph = library::audio_icon_value(icon_index, &entry.name);
                entry.icon_path = (!custom_path.is_empty()).then(|| custom_path.clone());
                state.audio_entries.clone()
            };
            app_config.borrow_mut().audio_entries = entries;
            save_queue.schedule();
            update_audio_list(&ui, &cache);
            ui.set_audio_entry_index(visible as i32);
        }
        ui.set_audio_edit_dialog_open(false);
    });
}
