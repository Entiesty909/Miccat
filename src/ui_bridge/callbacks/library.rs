//! library 相关 Slint 回调注册。

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
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_category_changed(move |index| {
            if let Some(category) = category_paths(&cache).get(index as usize).cloned() {
                cache.borrow_mut().audio_category = category;
            }
            if let Some(ui) = ui_weak.upgrade() {
                update_audio_list(&ui, &cache);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        ui.on_audio_category_context(move |index| {
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
            let (categories, new_path) = {
                let cache_state = cache.borrow();
                let new_path = crate::library::new_category(
                    &cache_state.audio_categories,
                    &cache_state.audio_entries,
                    &cache_state.audio_category,
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
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
                if let Some(index) = category_paths(&cache)
                    .iter()
                    .position(|path| path == &new_path)
                {
                    ui.set_audio_category_index(index as i32);
                    cache.borrow_mut().audio_category = new_path;
                }
                update_audio_list(&ui, &cache);
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
        ui.on_category_rename(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let index = ui.get_audio_category_index() as usize;
            let Some(old) = category_paths(&cache).get(index).cloned() else {
                return;
            };
            if index < 2 {
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
            let (categories, entries) = {
                let mut c = cache.borrow_mut();
                let mut categories = c.audio_categories.clone();
                let mut entries = c.audio_entries.clone();
                crate::library::rename_category(&mut categories, &mut entries, &old, &new_path);
                c.audio_categories = categories.clone();
                c.audio_entries = entries.clone();
                (categories, entries)
            };
            let mut cfg = app_config.borrow_mut();
            cfg.audio_categories = categories;
            cfg.audio_entries = entries;
            save_queue.schedule();
            drop(cfg);
            cache.borrow_mut().audio_category = new_path;
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
            let index = ui.get_audio_category_index() as usize;
            let Some(name) = category_paths(&cache).get(index).cloned() else {
                return;
            };
            if index < 2 {
                return;
            }
            let (categories, entries) = {
                let mut c = cache.borrow_mut();
                let mut categories = c.audio_categories.clone();
                let mut entries = c.audio_entries.clone();
                crate::library::delete_category(&mut categories, &mut entries, &name);
                c.audio_categories = categories.clone();
                c.audio_entries = entries.clone();
                c.audio_category = "全部".into();
                (categories, entries)
            };
            let mut cfg = app_config.borrow_mut();
            cfg.audio_categories = categories;
            cfg.audio_entries = entries;
            save_queue.schedule();
            drop(cfg);
            ui.set_audio_category_index(0);
            ui.set_audio_category_edit_name("".into());
            ui.set_audio_category_menu_open(false);
            ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
            update_audio_list(&ui, &cache);
        });
    }
}
