//! 音频库分类管理、分类重排和条目归类的 Slint 回调注册。

use super::super::*;

/// 绑定音频库分类、拖放和编辑对话框事件；修改先落缓存，再排队保存配置。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
    register_3(ui, state);
    register_drag(ui, state);
    register_entry_drag(ui, state);
    register_edit_dialogs(ui, state);
}

/// 处理分类选择、新建分类，并将当前分类分页重置到第一页。
fn register_1(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        // 分类索引来自 Slint 模型，路径列表是缓存分类与特殊分类组合后的可见顺序。
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
    // 右键入口的负索引表示空白区域，用于新建顶层分类；非负索引才对应已有路径。
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
        // 新分类继承当前选择的父路径，配置与缓存同时更新后刷新分类模型。
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

/// 处理分类重命名；保留父路径并拒绝保留字或同名路径冲突。
fn register_2(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        // 重命名必须同时迁移条目路径和图标映射，否则 UI 与持久化分类会分叉。
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

/// 删除当前分类及其条目，并清理分类图标路径。
fn register_3(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        // 删除后选择第一个可见分类，保证分类下拉框不残留已删除路径。
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

/// 绑定分类拖放事件；DataTransfer 只携带分类索引，实际路径从缓存解析。
fn register_drag(ui: &MainWindow, state: &AppState) {
    let state_for_drop = state.clone();
    let ui_weak = ui.as_weak();
    // 拖放载荷只传递稳定的分类行索引，避免把缓存路径直接暴露给 Slint 数据传输。
    // 生成阶段不读取分类名称，真正的路径解析延迟到 can_drop/commit 阶段完成。
    ui.on_audio_category_to_transfer(|index| {
        let payload = slint::SharedString::from(format!("miccat-category:{index}"));
        slint::DataTransfer::from(payload)
    });
    // 预检查与提交共用同一套位置枚举，UI 拒绝态不会改变缓存。
    ui.on_audio_category_can_drop({
        let cache = state.cache.clone();
        move |data, target_index, position| can_drop_category(&data, target_index, position, &cache)
    });
    // 提交时再次从载荷解析并校验，不能依赖 can_drop 回调之前的结果。
    ui.on_audio_category_drop(move |data, target_index, position| {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        commit_category_drop(data, target_index, position, &ui, &state_for_drop);
    });
}

/// Slint 1.18 仅公开 text/image/paths 与应用内 user_data；MIME 身份通过强类型封装。
const AUDIO_ENTRY_MIME: &str = "application/x-miccat-audio-entry";

/// 应用内拖放数据：MIME 身份与十进制 UTF-8 可见行下标共同定义载荷。
struct AudioEntryTransfer {
    /// 防止其它拖放载荷被误认为音频条目。
    mime_type: &'static str,
    /// 当前页下标而非配置下标，落下时重新解析当前筛选/排序/分页。
    payload: Vec<u8>,
}

/// 绑定音频条目拖入分类的同应用数据传输、预检查和提交。
fn register_entry_drag(ui: &MainWindow, state: &AppState) {
    let source_cache = state.cache.clone();
    ui.on_audio_entry_to_transfer(move |index| {
        let Ok(index) = usize::try_from(index) else {
            return slint::DataTransfer::default();
        };
        if super::super::library::visible_entry(&source_cache, index).is_none() {
            return slint::DataTransfer::default();
        }
        let payload = index.to_string();
        let mut transfer = slint::DataTransfer::from(slint::SharedString::from(payload.as_str()));
        transfer.set_user_data(std::rc::Rc::new(AudioEntryTransfer {
            mime_type: AUDIO_ENTRY_MIME,
            payload: payload.into_bytes(),
        }));
        transfer
    });

    let cache = state.cache.clone();
    ui.on_audio_entry_category_can_drop(move |data, target| {
        let Some(index) = audio_entry_drag_index(&data) else {
            return false;
        };
        super::super::library::entry_category_drop_target(&cache.borrow(), index, target).is_some()
    });

    let state_for_drop = state.clone();
    let ui_weak = ui.as_weak();
    ui.on_audio_entry_category_drop(move |data, target| {
        let Some(index) = audio_entry_drag_index(&data) else {
            return;
        };
        let Some(ui) = ui_weak.upgrade() else { return };
        // 预检查与真正提交之间筛选/分页可能改变，提交必须重新解析并校验。
        let changed = {
            let mut cache = state_for_drop.cache.borrow_mut();
            let mut config = state_for_drop.app_config.borrow_mut();
            change_entry_category(&mut cache, &mut config, index, target)
        };
        if changed {
            state_for_drop.save_queue.schedule();
            super::super::library::update_audio_categories(
                &ui,
                &state_for_drop.cache,
                &state_for_drop.app_config.borrow(),
            );
            update_audio_list(&ui, &state_for_drop.cache);
        }
    });
}

/// 仅解析本应用携带指定 MIME 的十进制 UTF-8 字节，拒绝分类与外部纯文本。
fn audio_entry_drag_index(data: &slint::DataTransfer) -> Option<usize> {
    let transfer = data.user_data()?.downcast::<AudioEntryTransfer>().ok()?;
    if transfer.mime_type != AUDIO_ENTRY_MIME {
        return None;
    }
    let payload = std::str::from_utf8(&transfer.payload).ok()?;
    let index = payload.parse::<usize>().ok()?;
    (index.to_string() == payload).then_some(index)
}

/// 校验成功才同步缓存与配置，非法目标时不得改动持久化配置快照。
fn change_entry_category(
    cache: &mut UiCache,
    config: &mut config::AppConfig,
    visible_index: usize,
    target_index: i32,
) -> bool {
    let Some((actual, category)) =
        super::super::library::entry_category_drop_target(cache, visible_index, target_index)
    else {
        return false;
    };
    cache.audio_entries[actual].category = category;
    config.audio_entries = cache.audio_entries.clone();
    true
}

/// 校验分类拖放目标，禁止无效索引和库层判定的循环/非法移动。
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
    // 可见路径列表包含特殊分类，因此拖放索引必须按当前模型顺序映射。
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

/// 提交分类拖放，更新子树路径、当前选择、缓存以及配置保存队列。
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
        // 被移动分类及其后代都要同步改写当前选择，防止选择指向旧路径。
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

/// 载荷只接受本应用前缀，防止外部拖放数据被误解释为分类索引。
/// 从拖放载荷读取分类索引；非本应用前缀的数据必须拒绝。
fn category_drag_index(data: &slint::DataTransfer) -> Option<usize> {
    let payload = data.plain_text().ok()?;
    payload.strip_prefix("miccat-category:")?.parse().ok()
}

/// 位置值由 Slint 的拖放区域约定产生，未知值必须返回 None 而不是猜测语义。
/// 将 Slint 的 0/1/2 位置值转换为前置、移入、后置语义。
fn category_drop_position(value: i32) -> Option<crate::library::CategoryDropPosition> {
    match value {
        // 0 表示插入目标分类之前，保持同级路径。
        0 => Some(crate::library::CategoryDropPosition::Before),
        // 1 表示移入目标分类，源分类成为其子分类。
        1 => Some(crate::library::CategoryDropPosition::Into),
        // 2 表示插入目标分类之后，仍保持同级路径。
        2 => Some(crate::library::CategoryDropPosition::After),
        _ => None,
    }
}

/// 绑定分类/音频编辑窗口的打开、取消、自定义图标与提交回调。
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
    // 特殊分类不可编辑；普通分类读取其持久化图标并同步两个窗口实例。
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
        super::super::window_lifecycle::show_audio_edit_window(&state_for_category);
    });
    let cache_for_entry = state.cache.clone();
    let edit_for_entry = edit_window.clone();
    let state_for_entry = state.clone();
    let ui_weak_entry = ui.as_weak();
    // 音频编辑使用可见索引转实际索引，避免分页/搜索后修改错误条目。
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
        super::super::window_lifecycle::show_audio_edit_window(&state_for_entry);
    });
    // 取消只关闭对话框，不触碰缓存；未提交的编辑值因此不会泄漏到列表。
    let ui_weak_cancel = ui.as_weak();
    ui.on_audio_edit_cancelled(move || {
        if let Some(ui) = ui_weak_cancel.upgrade() {
            ui.set_audio_edit_dialog_open(false);
        }
    });
    let edit_for_icon = edit_window.clone();
    let ui_weak_icon = ui.as_weak();
    // 文件选择器返回的路径同时更新预览图和编辑窗口状态，提交时再持久化。
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
    // 提交分类时迁移图标键；提交条目时只改名称、字形和可选自定义图标路径。
    let ui_weak_accept = ui.as_weak();
    // 接受回调根据对话框模式分流：分类迁移路径，条目只更新自身元数据。
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

#[cfg(test)]
mod tests {
    use super::{
        AUDIO_ENTRY_MIME, AudioEntryTransfer, audio_entry_drag_index, change_entry_category,
    };
    use crate::config::{self, AudioEntry};
    use crate::ui_bridge::UiCache;
    use std::rc::Rc;

    /// 测试用条目不对应真实文件，拖放仅修改配置内的分类路径。
    fn entry(category: &str) -> AudioEntry {
        AudioEntry {
            name: "sound".into(),
            path: String::new(),
            icon_path: None,
            icon_glyph: String::new(),
            category: category.into(),
            duration_hns: 0,
            file_size_bytes: 0,
            sample_rate: 0,
            channels: 0,
            loop_playback: false,
            hotkey: None,
            hotkey_binding: None,
        }
    }

    /// 拖动载荷必须有本应用 MIME 身份及规范十进制 UTF-8 下标。
    #[test]
    fn audio_entry_transfer_rejects_category_and_malformed_payloads() {
        let mut data = slint::DataTransfer::from(slint::SharedString::from("0"));
        assert_eq!(audio_entry_drag_index(&data), None);
        data.set_user_data(Rc::new(AudioEntryTransfer {
            mime_type: AUDIO_ENTRY_MIME,
            payload: b"0".to_vec(),
        }));
        assert_eq!(audio_entry_drag_index(&data), Some(0));
        data.set_user_data(Rc::new(AudioEntryTransfer {
            mime_type: AUDIO_ENTRY_MIME,
            payload: b"01".to_vec(),
        }));
        assert_eq!(audio_entry_drag_index(&data), None);
        data.set_user_data(Rc::new(AudioEntryTransfer {
            mime_type: "other",
            payload: b"0".to_vec(),
        }));
        assert_eq!(audio_entry_drag_index(&data), None);
    }

    /// 非法目标拒绝提交且配置数组保持原样，有效目标才落配置。
    #[test]
    fn invalid_entry_category_drop_does_not_write_config() {
        let mut cache = UiCache::default();
        cache.audio_category = "全部".into();
        cache.audio_sort_ascending = true;
        cache.audio_categories = vec!["全部".into(), "未分类".into(), "效果音".into()];
        cache.audio_entries = vec![entry("未分类")];
        let mut config = config::AppConfig::default();
        config.audio_entries = cache.audio_entries.clone();

        assert!(!change_entry_category(&mut cache, &mut config, 0, 0));
        assert!(!change_entry_category(&mut cache, &mut config, 0, 1));
        assert!(!change_entry_category(&mut cache, &mut config, 0, 99));
        assert!(!change_entry_category(&mut cache, &mut config, 20, 2));
        assert_eq!(config.audio_entries[0].category, "未分类");
        assert_eq!(cache.audio_entries[0].category, "未分类");
        assert!(change_entry_category(&mut cache, &mut config, 0, 2));
        assert_eq!(config.audio_entries[0].category, "效果音");
        assert_eq!(cache.audio_entries[0].category, "效果音");
    }
}
