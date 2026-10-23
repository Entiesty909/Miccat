//! library_entries 相关 Slint 回调注册。

use super::super::*;
use slint::Model;

/// 绑定音频文件导入、删除、资源管理器操作、路由添加和快捷键解绑。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_1(ui, state);
    register_2(ui, state);
    register_3(ui, state);
}

/// 处理文件选择与导入；校验元数据后写入缓存，再排队保存配置。
fn register_1(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        // 导入前校验解码能力和时长，失败通过统一诊断上报而不写入半成品条目。
        // 文件选择取消时不改变缓存；成功路径必须先通过统一校验再创建条目。
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
            // 未显式选择分类时由库层归入默认分类，避免把空路径写入持久化条目。
            let category = {
                let cache_state = cache.borrow();
                crate::library::import_category(&cache_state.audio_category)
            };
            // AudioEntry 的时间单位为 100ns，文件大小为字节，采样率为 Hz。
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
            // 先写缓存再复制到 AppConfig，保存队列只负责异步落盘而不改变当前列表。
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
    // 处理音频条目右键选择，关闭分类菜单并清除双击检测状态。
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        // 右键索引是当前可见行索引，菜单动作沿用该选择并清空双击时间窗口。
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

/// 处理条目删除、以及在资源管理器中定位条目的请求。
fn register_2(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        // visible_entry 将分页/搜索行转换为缓存实际索引，删除后重新生成可见列表。
        // 删除操作按当前可见索引定位真实条目，避免分页或搜索状态下删错行。
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
        // /select 参数只定位文件；直接打开则交给系统关联程序处理。
        // 资源管理器定位只读条目路径，不修改音频库名称或路径字段。
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

/// 绑定条目文件操作、右键菜单添加路由和清除全局热键的回调。
fn register_3(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        // “编辑文件”打开原始路径，条目元数据仍由音频编辑对话框负责更新。
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
        // 菜单选择的是本页的可见行；新增路由必须转为真实库索引，再复用源槽位逻辑。
        ui.on_audio_entry_add_route(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let Ok(visible_index) = usize::try_from(ui.get_audio_entry_index()) else {
                return;
            };
            let Some(actual_index) =
                visible_entry(&cache, visible_index).and_then(|index| i32::try_from(index).ok())
            else {
                return;
            };
            // 音频库菜单始终新建卡片，不继承独立添加源窗口尚未关闭的替换上下文。
            ui.set_source_replace_index(-1);
            ui.invoke_audio_entry_add_route_index(actual_index);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let state_for_update = state.clone();
        // 添加源窗口传来真实库索引；替换按文件槽位计算，新增则在末尾追加。
        ui.on_audio_entry_add_route_index(move |actual_index| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(actual_index) = usize::try_from(actual_index).ok() else {
                return;
            };
            // 替换模式保留文件卡片槽位；新增模式在末尾追加独立的文件播放状态。
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
                // 目录中的条目索引是库缓存索引，不是当前分页或搜索模型的行索引。
                // 同一条目不能重复占用多个文件卡片；拒绝时保持原槽位不变。
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
                if file_slot < cache_state.audio_finished_handled.len() {
                    cache_state.audio_finished_handled[file_slot] = false;
                }
                // 替换时保留原槽；若来自添加源窗口，明确提交该窗口的多选草稿。
                let selected = ui.get_pending_file_output_selected();
                if selected.row_count() > 0 {
                    let ids = cache_state
                        .dests
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| selected.row_data(*index) == Some(true))
                        .map(|(_, device)| device.selection_id().to_owned())
                        .collect::<Vec<_>>();
                    if !ids.is_empty() {
                        let mut config = app_config.borrow_mut();
                        if let Some(slot) = config.output.file_output_ids.get_mut(file_slot) {
                            *slot = ids;
                        }
                    }
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
                // 新文件卡片默认沿用所选输出；后续下拉多选只覆盖自己的配置槽位。
                cache_state.route_audio_entries.push(actual_index);
                cache_state.route_audio_enabled.push(false);
                cache_state.audio_play_modes.push(0);
                cache_state.audio_finished_handled.push(false);
                cache_state.audio_volumes.push(app_config.borrow().mix.sfx);
                cache_state.audio_muted_volumes.push(None);
                let selected = ui.get_pending_file_output_selected();
                let output_ids = cache_state
                    .dests
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| selected.row_data(*index) == Some(true))
                    .map(|(_, device)| device.selection_id().to_owned())
                    .collect::<Vec<_>>();
                let slot = cache_state.route_audio_entries.len() - 1;
                let mut config = app_config.borrow_mut();
                let fallback = config.output.device_ids.clone();
                config.output.file_output_ids.resize(slot + 1, fallback);
                let selected_ids = if output_ids.is_empty() {
                    cache_state
                        .dests
                        .get(ui.get_file_output_index().max(0) as usize)
                        .map(|device| vec![device.selection_id().to_owned()])
                        .unwrap_or_default()
                } else {
                    output_ids
                };
                if !selected_ids.is_empty() {
                    config.output.file_output_ids[slot] = selected_ids;
                }
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
    // 清除快捷键必须同时更新条目字段和全局注册表，避免旧绑定继续触发。
    {
        // 清除该条目的快捷键；此前没有任何解绑入口。
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let hotkey_server = state.hotkey_server.clone();
        // 清除入口同样使用可见到实际索引的映射，保证搜索结果中的快捷键归属正确。
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
                // 旧标签与新组合键字段都置空，随后统一重同步系统级注册表。
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
