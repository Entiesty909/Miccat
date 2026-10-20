//! 独立 Slint 窗口的生命周期、位置持久化和属性同步。

use super::*;

/// 独立窗口在 `AppConfig.windows` 中对应的位置槽位。
#[derive(Clone, Copy)]
enum WindowSlot {
    /// 主窗口在配置中的位置。
    Main,
    /// 添加源窗口在配置中的位置。
    AddSource,
    /// 音频编辑窗口在配置中的位置。
    AudioEdit,
}

/// 注册主窗口、添加源窗口和音频编辑窗口的关闭与转发回调。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    let windows = state.windows.clone();
    let main_weak = ui.as_weak();
    // 所有回调都运行在 Slint UI 线程；窗口只隐藏不销毁，状态由主窗口回调转发并保持单一入口。
    let add_source_weak = windows.add_source.as_weak();
    let audio_edit_weak = windows.audio_edit.as_weak();
    let config_for_main_close = state.app_config.clone();
    let queue_for_main_close = state.save_queue.clone();
    ui.window().on_close_requested(move || {
        // 主窗口关闭时先保存三个窗口的位置，再隐藏而不是销毁 Slint 对象。
        if let Some(main) = main_weak.upgrade() {
            save_window_position(
                main.window(),
                WindowSlot::Main,
                &config_for_main_close,
                &queue_for_main_close,
            );
        }
        if let Some(source) = add_source_weak.upgrade()
            && source.window().is_visible()
        {
            save_and_hide_window(
                &source,
                WindowSlot::AddSource,
                &config_for_main_close,
                &queue_for_main_close,
            );
        }
        if let Some(editor) = audio_edit_weak.upgrade()
            && editor.window().is_visible()
        {
            save_and_hide_window(
                &editor,
                WindowSlot::AudioEdit,
                &config_for_main_close,
                &queue_for_main_close,
            );
        }
        slint::CloseRequestResponse::HideWindow
    });

    // 子窗口关闭时清除替换槽位，避免下一次“添加源”误把操作作用到已关闭卡片。
    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    let config_for_source_close = state.app_config.clone();
    let queue_for_source_close = state.save_queue.clone();
    windows.add_source.window().on_close_requested(move || {
        if let Some(source) = add_source_weak.upgrade() {
            save_window_position(
                source.window(),
                WindowSlot::AddSource,
                &config_for_source_close,
                &queue_for_source_close,
            );
        }
        if let Some(main) = main_weak.upgrade() {
            // 关闭替换窗口时清除旧卡片索引，避免下一次添加误替换。
            main.set_source_replace_index(-1);
        }
        slint::CloseRequestResponse::HideWindow
    });

    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    let config_for_process = state.app_config.clone();
    let queue_for_process = state.save_queue.clone();
    windows.add_source.on_process_add(move || {
        let Some(source) = add_source_weak.upgrade() else {
            return;
        };
        if let Some(main) = main_weak.upgrade() {
            main.set_process_index(source.get_process_index());
            main.invoke_process_add();
        }
        save_and_hide_window(
            &source,
            WindowSlot::AddSource,
            &config_for_process,
            &queue_for_process,
        );
    });

    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    let config_for_mic_add = state.app_config.clone();
    let queue_for_mic_add = state.save_queue.clone();
    windows.add_source.on_mic_add(move || {
        let Some(source) = add_source_weak.upgrade() else {
            return;
        };
        if let Some(main) = main_weak.upgrade() {
            main.set_mic_index(source.get_mic_index());
            main.invoke_mic_mix_changed(true);
            main.set_source_replace_index(-1);
        }
        save_and_hide_window(
            &source,
            WindowSlot::AddSource,
            &config_for_mic_add,
            &queue_for_mic_add,
        );
    });

    // 刷新先驱动主窗口重新枚举，再把同一份模型投影到子窗口，避免两套列表分叉。
    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    let cache_for_source_sync = state.cache.clone();
    let config_for_source_sync = state.app_config.clone();
    windows.add_source.on_refresh(move || {
        if let Some(main) = main_weak.upgrade() {
            main.invoke_refresh();
            if let Some(source) = add_source_weak.upgrade() {
                sync_add_source_window(
                    &main,
                    &source,
                    &cache_for_source_sync,
                    &config_for_source_sync,
                );
            }
        }
    });

    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    let cache_for_file_sync = state.cache.clone();
    let config_for_file_sync = state.app_config.clone();
    windows.add_source.on_add_files(move || {
        if let Some(main) = main_weak.upgrade() {
            main.invoke_add_audio_files();
            if let Some(source) = add_source_weak.upgrade() {
                sync_add_source_window(&main, &source, &cache_for_file_sync, &config_for_file_sync);
                source.set_refresh_surface(!source.get_refresh_surface());
                refresh_window_after_show(&source);
            }
        }
    });

    // 添加音频条目后立即隐藏窗口；真正的重复检查和路由变更仍由主窗口处理。
    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    let config_for_file_add = state.app_config.clone();
    let queue_for_file_add = state.save_queue.clone();
    windows.add_source.on_audio_entry_add_route(move |index| {
        if let Some(source) = add_source_weak.upgrade() {
            save_and_hide_window(
                &source,
                WindowSlot::AddSource,
                &config_for_file_add,
                &queue_for_file_add,
            );
        }
        if let Some(main) = main_weak.upgrade() {
            main.invoke_audio_entry_add_route_index(index);
        }
    });

    // 路由子窗口只采集编辑值；主窗口仍是音量与设备路由状态的唯一写入入口。
    let main_weak = ui.as_weak();
    windows
        .add_source
        .on_audio_file_volume_edited(move |value| {
            if let Some(main) = main_weak.upgrade() {
                main.invoke_audio_file_volume_edited(value);
            }
        });
    let main_weak = ui.as_weak();
    windows
        .add_source
        .on_audio_file_output_changed(move |index| {
            if let Some(main) = main_weak.upgrade() {
                main.set_file_output_index(index);
                main.invoke_file_output_changed(index);
            }
        });
    let main_weak = ui.as_weak();
    windows.add_source.on_mic_volume_edited(move |value| {
        if let Some(main) = main_weak.upgrade() {
            main.invoke_microphone_input_volume_edited(value);
        }
    });
    // 麦克风混音事件携带子窗口当前索引，确保主窗口更新的是同一输入源。
    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    windows.add_source.on_mic_mix_changed(move |value| {
        if let Some(main) = main_weak.upgrade()
            && let Some(source) = add_source_weak.upgrade()
        {
            main.set_mic_index(source.get_mic_index());
            main.invoke_mic_mix_changed(value);
        }
    });

    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    let config_for_library = state.app_config.clone();
    let queue_for_library = state.save_queue.clone();
    windows.add_source.on_open_library(move || {
        if let Some(main) = main_weak.upgrade() {
            main.set_page(1);
        }
        if let Some(source) = add_source_weak.upgrade() {
            save_and_hide_window(
                &source,
                WindowSlot::AddSource,
                &config_for_library,
                &queue_for_library,
            );
        }
    });

    // 编辑窗口的接受/取消都通过主窗口回调完成，编辑器本身只负责收集表单值。
    let audio_edit_weak = windows.audio_edit.as_weak();
    let main_weak = ui.as_weak();
    let config_for_edit_close = state.app_config.clone();
    let queue_for_edit_close = state.save_queue.clone();
    windows.audio_edit.window().on_close_requested(move || {
        if let Some(editor) = audio_edit_weak.upgrade() {
            save_window_position(
                editor.window(),
                WindowSlot::AudioEdit,
                &config_for_edit_close,
                &queue_for_edit_close,
            );
        }
        if let Some(main) = main_weak.upgrade() {
            main.invoke_audio_edit_cancelled();
        }
        slint::CloseRequestResponse::HideWindow
    });

    let audio_edit_weak = windows.audio_edit.as_weak();
    let main_weak = ui.as_weak();
    let config_for_accept = state.app_config.clone();
    let queue_for_accept = state.save_queue.clone();
    windows.audio_edit.on_accepted(move |name, icon, path| {
        if let Some(main) = main_weak.upgrade() {
            main.invoke_audio_edit_accepted(name, icon, path);
            if !main.get_audio_edit_dialog_open()
                && let Some(editor) = audio_edit_weak.upgrade()
            {
                save_and_hide_window(
                    &editor,
                    WindowSlot::AudioEdit,
                    &config_for_accept,
                    &queue_for_accept,
                );
            }
        }
    });

    // 接受回调可能触发主窗口关闭编辑对话框；只有主窗口确认完成后才隐藏窗口。
    let audio_edit_weak = windows.audio_edit.as_weak();
    let main_weak = ui.as_weak();
    let config_for_cancel = state.app_config.clone();
    let queue_for_cancel = state.save_queue.clone();
    windows.audio_edit.on_cancelled(move || {
        if let Some(main) = main_weak.upgrade() {
            main.invoke_audio_edit_cancelled();
        }
        if let Some(editor) = audio_edit_weak.upgrade() {
            save_and_hide_window(
                &editor,
                WindowSlot::AudioEdit,
                &config_for_cancel,
                &queue_for_cancel,
            );
        }
    });

    let audio_edit_weak = windows.audio_edit.as_weak();
    let main_weak = ui.as_weak();
    windows.audio_edit.on_custom_icon_requested(move || {
        if let Some(main) = main_weak.upgrade()
            && let Some(editor) = audio_edit_weak.upgrade()
        {
            main.invoke_audio_edit_custom_icon_requested();
            editor.set_custom_icon_image(main.get_audio_edit_custom_icon_image());
            editor.set_custom_icon_path(main.get_audio_edit_custom_icon_path());
            editor.set_refresh_surface(!editor.get_refresh_surface());
            refresh_window_after_show(&editor);
        }
    });
}

/// 保存指定窗口的物理位置，并通过防抖队列写入配置。
fn save_window_position(
    window: &slint::Window,
    slot: WindowSlot,
    app_config: &Rc<RefCell<config::AppConfig>>,
    save_queue: &Rc<SaveQueue>,
) {
    // WindowSlot 只选择 AppConfig 中的字段；位置使用物理像素，供跨 DPI 会话原样恢复。
    let position = window::saved_position(window);
    let position = config::WindowPosition {
        x: position.x,
        y: position.y,
    };
    let mut app_config = app_config.borrow_mut();
    match slot {
        WindowSlot::Main => app_config.windows.main = Some(position),
        WindowSlot::AddSource => app_config.windows.add_source = Some(position),
        WindowSlot::AudioEdit => app_config.windows.audio_edit = Some(position),
    }
    drop(app_config);
    save_queue.schedule();
}

/// 保存位置后隐藏窗口，保留对象以便下一次打开时复用。
fn save_and_hide_window<T: ComponentHandle>(
    window: &T,
    slot: WindowSlot,
    app_config: &Rc<RefCell<config::AppConfig>>,
    save_queue: &Rc<SaveQueue>,
) {
    save_window_position(window.window(), slot, app_config, save_queue);
    let _ = window.hide();
}

/// 显示添加源窗口，并恢复其位置、Tab 和替换模式。
pub(super) fn open_add_source_window(ui: &MainWindow, state: &AppState) {
    library::update_audio_catalog(ui, &state.cache);
    let source = state.windows.add_source.as_ref();
    // 仅首次从隐藏态打开时恢复保存坐标，重复显示时保留用户刚调整的位置。
    let first_show = !source.window().is_visible();
    let saved_position = state.app_config.borrow().windows.add_source;
    if first_show {
        window::restore_position(source.window(), saved_position);
    }
    sync_add_source_window(ui, source, &state.cache, &state.app_config);
    source.set_selected_tab(ui.get_source_add_tab());
    source.set_single_tab(ui.get_source_replace_index() >= 0);
    if let Err(error) = source.show() {
        tracing::warn!("显示添加源窗口失败：{error}");
    } else {
        if first_show
            && saved_position.is_none()
            && let Err(error) = crate::platform::window::center_window_on_work_area("添加音频源")
        {
            tracing::warn!("首次居中添加源窗口失败：{error}");
        }
        source.set_refresh_surface(!source.get_refresh_surface());
        refresh_window_after_show(source);
        if let Err(error) = crate::platform::window::activate_window("添加音频源") {
            tracing::warn!("前置添加源窗口失败：{error}");
        }
    }
}

/// 显示音频编辑窗口，并在首次显示时恢复或计算其位置。
pub(super) fn show_audio_edit_window(state: &AppState) {
    let editor = state.windows.audio_edit.as_ref();
    // 编辑器对象会复用，因此只在隐藏态到显示态的首次切换恢复持久化坐标。
    let first_show = !editor.window().is_visible();
    let saved_position = state.app_config.borrow().windows.audio_edit;
    if first_show {
        window::restore_position(editor.window(), saved_position);
    }
    if let Err(error) = editor.show() {
        tracing::warn!("显示音频编辑窗口失败：{error}");
    } else {
        if first_show && saved_position.is_none() {
            let title = editor.get_edit_title();
            if let Err(error) = crate::platform::window::center_window_on_work_area(title.as_str())
            {
                tracing::warn!("首次居中音频编辑窗口失败：{error}");
            }
        }
        editor.set_refresh_surface(!editor.get_refresh_surface());
        refresh_window_after_show(editor);
        let title = editor.get_edit_title();
        if let Err(error) = crate::platform::window::activate_window(title.as_str()) {
            tracing::warn!("前置音频编辑窗口失败：{error}");
        }
    }
}

/// 在显示独立窗口后请求当前帧和下一事件循环帧各重绘一次。
pub(super) fn refresh_window_after_show<T>(window: &T)
where
    T: ComponentHandle + 'static,
{
    window.window().request_redraw();
    let weak = window.as_weak();
    // 首次请求在显示调用中立即执行；此处延后一轮，等待原生窗口完成首帧布局。
    slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
        if let Some(window) = weak.upgrade() {
            window.window().request_redraw();
        }
    });
}

/// 将主窗口当前列表和路由设置同步到添加源窗口。
pub(super) fn sync_add_source_window(
    ui: &MainWindow,
    window: &AddSourceWindow,
    cache: &Rc<RefCell<UiCache>>,
    app_config: &Rc<RefCell<config::AppConfig>>,
) {
    // 列表与路由值均以主窗口为准，子窗口只接收快照，避免独立副本产生分歧。
    window.set_processes(ui.get_processes());
    window.set_process_index(ui.get_process_index());
    window.set_process_icon_images(ui.get_process_icon_images());
    window.set_process_pids(ui.get_process_pids());
    window.set_mics(ui.get_mics());
    window.set_mic_index(ui.get_mic_index());
    window.set_audio_entries(ui.get_audio_library_catalog());
    window.set_audio_entry_metadata(ui.get_audio_library_metadata());
    window.set_audio_entry_durations(ui.get_audio_library_durations());
    window.set_audio_entry_sizes(ui.get_audio_library_sizes());
    window.set_audio_entry_categories(ui.get_audio_library_categories());
    window.set_audio_entry_hotkeys(ui.get_audio_library_hotkeys());
    window.set_audio_entry_loops(ui.get_audio_library_loops());
    window.set_output_options(ui.get_devices());
    let output_id = app_config.borrow().output.mic_output_id.clone();
    // 子窗口消费主窗口的派生模型；麦克风输出选项的零号项表示跟随全局设备。
    let cache = cache.borrow();
    let mut mic_output_options = vec!["跟随全局默认设备".into()];
    mic_output_options.extend(cache.dests.iter().map(|device| device.label.clone().into()));
    // 配置保存的是 render_id，Slint 仍使用带“跟随全局”偏移的一-based 下标。
    let mic_output_index = output_id
        .as_deref()
        .and_then(|id| cache.dests.iter().position(|device| device.render_id == id))
        .map(|index| (index + 1) as i32)
        .unwrap_or(0);
    window.set_mic_output_options(ModelRc::new(VecModel::from(mic_output_options)));
    window.set_mic_output_index(mic_output_index);
    window.set_file_output_index(ui.get_file_output_index());
    window.set_file_volume(ui.get_audio_file_volume());
    window.set_mic_volume(ui.get_microphone_input_volume());
    window.set_mix_mic(ui.get_mix_mic());
    window.set_running(ui.get_running());
}
