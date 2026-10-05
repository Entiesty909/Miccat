//! library 相关 Slint 回调注册。

use super::super::*;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
    register_3(ui, state);
    register_drag(ui, state);
    register_loop(ui, state);
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
                    ui.set_audio_category_edit_name("".into());
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
            ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
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
            ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
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
            save_queue.schedule();
            drop(cfg);
            ui.set_audio_category_index(selected_index);
            ui.set_audio_category_edit_name("".into());
            ui.set_audio_category_menu_open(false);
            ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
            update_audio_list(&ui, &cache);
        });
    }
}

fn register_drag(ui: &MainWindow, state: &AppState) {
    let state_for_drop = state.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_category_to_transfer(|index| {
        let payload = slint::SharedString::from(format!("processmic-category:{index}"));
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
    ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&state.cache))));
    ui.set_audio_category_index(selected_index);
    if selected.is_empty() {
        ui.set_audio_category_index(-1);
    }
    update_audio_list(ui, &state.cache);
}

fn category_drag_index(data: &slint::DataTransfer) -> Option<usize> {
    let payload = data.plain_text().ok()?;
    payload.strip_prefix("processmic-category:")?.parse().ok()
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
