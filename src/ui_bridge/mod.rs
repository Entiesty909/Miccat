//! Slint 桥：分页界面、列表填充、状态轮询、热键。

use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::config;
use crate::engine::AudioEngine;
use crate::error::Error;

slint::include_modules!();

mod cache;
mod callbacks;
mod dev;
mod diagnostics;
mod dialogs;
mod format;
mod input;
mod library;
mod lists;
mod persist;
mod sources;
mod state;
mod status;
mod window;

use cache::UiCache;
pub(crate) use dev::{SnapshotOptions, snapshot_to_file};
use dialogs::{select_audio_file, select_icon_file};
use format::format_hns_position;
use input::start_hotkey;
use library::{category_leaf, category_paths, update_audio_list, visible_entry};
use lists::{refill, refill_devices, update_output_text};
use persist::{
    SaveQueue, hydrate_audio_durations, persist_config, restore_default_mics,
    restore_saved_selection,
};
use state::AppState;
use status::start_status_timer;
use window::fit_window_to_screen;
/// 创建窗口、填列表、跑事件循环。退出时还原默认麦并停引擎。
pub fn run_ui() -> Result<(), Error> {
    let Some(_instance) = config::acquire_single_instance()? else {
        return Ok(());
    };
    let ui = MainWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?;
    let add_source_window =
        Rc::new(AddSourceWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?);
    let audio_edit_window =
        Rc::new(AudioEditWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?);
    let engine = Rc::new(AudioEngine::new());
    let app_config = Rc::new(RefCell::new(config::load()));
    if hydrate_audio_durations(&app_config) {
        config::save(&app_config.borrow());
    }
    let save_queue = SaveQueue::new(app_config.clone())?;
    let persisted = app_config.borrow().clone();
    let cache = Rc::new(RefCell::new(UiCache {
        all_devices: persisted.output.all_devices,
        selected_device_ids: persisted.output.device_ids.clone(),
        device_selected: Vec::new(),
        selected_pids: Vec::new(),
        process_volumes: persisted.mix.process_volumes.clone(),
        process_output_ids: persisted
            .output
            .process_output_ids
            .iter()
            .cloned()
            .map(Some)
            .collect(),
        audio_volumes: persisted.mix.sfx_volumes.clone(),
        audio_output_ids: Vec::new(),
        set_default_communications: persisted.output.set_default_communications,
        audio_categories: persisted.audio_categories.clone(),
        audio_entries: persisted.audio_entries.clone(),
        audio_category: crate::library::category_paths(
            &persisted.audio_categories,
            &persisted.audio_entries,
        )
        .first()
        .cloned()
        .unwrap_or_default(),
        ..UiCache::default()
    }));
    let list_refresh = Arc::new(state::ListRefresh::default());
    let windows = Rc::new(state::AppWindows {
        add_source: add_source_window,
        audio_edit: audio_edit_window,
    });
    let saved_mics = Rc::new(RefCell::new(Vec::<(u32, String)>::new()));
    let hotkey = Rc::new(RefCell::new(persisted.hotkeys.toggle_route));
    engine.set_volume(persisted.mix.process);
    engine.set_mic_volume(persisted.mix.mic);
    ui.set_master_volume((persisted.mix.master * 100.0).clamp(0.0, 100.0));
    engine.set_master_volume(persisted.mix.master);
    engine.set_sfx_volume(persisted.mix.sfx);
    for (slot, volume) in persisted.mix.process_volumes.iter().copied().enumerate() {
        engine.set_process_volume(slot, volume);
    }
    for (slot, volume) in persisted.mix.sfx_volumes.iter().copied().enumerate() {
        engine.set_sfx_volume_for_slot(slot, volume);
    }
    ui.set_mix_mic(persisted.route.mix_mic);
    ui.set_audio_file_volume((persisted.mix.sfx * 100.0).clamp(0.0, 100.0));
    ui.set_microphone_input_volume((persisted.mix.mic * 100.0).clamp(0.0, 100.0));
    fit_window_to_screen(&ui);
    if let Some(position) = persisted.windows.main {
        window::restore_position(ui.window(), Some(position));
    }
    ui.window()
        .show()
        .map_err(|e| Error::InvalidArgs(e.to_string()))?;
    if persisted.windows.main.is_none()
        && let Err(error) = crate::platform::window::center_window_on_work_area("Miccat")
    {
        tracing::warn!("首次居中主窗口失败：{error}");
        window::center_window_on_screen(ui.window());
    }
    refresh_window_after_show(&ui);
    diagnostics::init(&ui, &persisted);
    refill(&ui, &cache, true);
    restore_saved_selection(&ui, &cache, &persisted);
    if ui.get_mix_mic() && ui.get_mic_index() == 0 {
        ui.set_mix_mic(false);
        let mut config = app_config.borrow_mut();
        config.route.mix_mic = false;
        config.route.mic_id = None;
        save_queue.schedule();
    }
    library::update_audio_categories(&ui, &cache, &app_config.borrow());
    update_audio_list(&ui, &cache);

    let hotkey_server = Rc::new(start_hotkey(
        &ui,
        &hotkey,
        input::collect_bindings(&app_config.borrow()),
    ));
    let state = AppState {
        engine: engine.clone(),
        cache: cache.clone(),
        saved_mics: saved_mics.clone(),
        hotkey: hotkey.clone(),
        hotkey_server: hotkey_server.clone(),
        app_config: app_config.clone(),
        save_queue: save_queue.clone(),
        windows: windows.clone(),
        list_refresh: list_refresh.clone(),
    };
    wire_callbacks(&ui, &state);
    sources::update(&ui, &state);
    let _timer = start_status_timer(&ui, &state);
    // 启动时把三行热键文案灌入 Slint。
    callbacks::refresh_row_labels(&ui, &app_config);
    // 热键在后台线程注册；进事件循环后取一次失败列表并逐条上报。
    let failure_timer = slint::Timer::default();
    {
        let ui_weak = ui.as_weak();
        let server = hotkey_server.clone();
        failure_timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(400),
            move || {
                let Some(ui) = ui_weak.upgrade() else { return };
                for failure in server.take_failures() {
                    diagnostics::report_warning(
                        &ui,
                        &format!("热键 {} 注册失败：{}", failure.id, failure.reason),
                    );
                }
            },
        );
    }
    let _failure_timer = failure_timer;
    let _hotkey_server = hotkey_server;

    ui.run().map_err(|e| Error::InvalidArgs(e.to_string()))?;
    persist_config(&ui, &cache, &app_config);
    save_queue.flush();
    save_queue.shutdown();
    restore_default_mics(&saved_mics);
    engine.stop();
    Ok(())
}

/// 把 Slint 回调接到引擎与列表缓存。
fn wire_callbacks(ui: &MainWindow, state: &AppState) {
    callbacks::register(ui, state);
    diagnostics::register(ui, state);
    register_window_callbacks(ui, state);
}

fn register_window_callbacks(ui: &MainWindow, state: &AppState) {
    let windows = state.windows.clone();
    let main_weak = ui.as_weak();
    let add_source_weak = windows.add_source.as_weak();
    let audio_edit_weak = windows.audio_edit.as_weak();
    let config_for_main_close = state.app_config.clone();
    let queue_for_main_close = state.save_queue.clone();
    ui.window().on_close_requested(move || {
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
    windows.add_source.on_refresh(move || {
        if let Some(main) = main_weak.upgrade() {
            main.invoke_refresh();
            if let Some(source) = add_source_weak.upgrade() {
                sync_add_source_window(&main, &source);
            }
        }
    });

    let add_source_weak = windows.add_source.as_weak();
    let main_weak = ui.as_weak();
    windows.add_source.on_add_files(move || {
        if let Some(main) = main_weak.upgrade() {
            main.invoke_add_audio_files();
            if let Some(source) = add_source_weak.upgrade() {
                sync_add_source_window(&main, &source);
                source.set_refresh_surface(!source.get_refresh_surface());
                refresh_window_after_show(&source);
            }
        }
    });

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

#[derive(Clone, Copy)]
enum WindowSlot {
    Main,
    AddSource,
    AudioEdit,
}

fn save_window_position(
    window: &slint::Window,
    slot: WindowSlot,
    app_config: &Rc<RefCell<config::AppConfig>>,
    save_queue: &Rc<SaveQueue>,
) {
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

fn save_and_hide_window<T: ComponentHandle>(
    window: &T,
    slot: WindowSlot,
    app_config: &Rc<RefCell<config::AppConfig>>,
    save_queue: &Rc<SaveQueue>,
) {
    save_window_position(window.window(), slot, app_config, save_queue);
    let _ = window.hide();
}

pub(in crate::ui_bridge) fn open_add_source_window(ui: &MainWindow, state: &AppState) {
    library::update_audio_catalog(ui, &state.cache);
    let source = state.windows.add_source.as_ref();
    let first_show = !source.window().is_visible();
    let saved_position = state.app_config.borrow().windows.add_source;
    if first_show {
        window::restore_position(source.window(), saved_position);
    }
    sync_add_source_window(ui, source);
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

pub(in crate::ui_bridge) fn show_audio_edit_window(state: &AppState) {
    let editor = state.windows.audio_edit.as_ref();
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

fn refresh_window_after_show<T>(window: &T)
where
    T: ComponentHandle + 'static,
{
    window.window().request_redraw();
    let weak = window.as_weak();
    slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
        if let Some(window) = weak.upgrade() {
            window.window().request_redraw();
        }
    });
}

fn sync_add_source_window(ui: &MainWindow, window: &AddSourceWindow) {
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
    window.set_file_output_index(ui.get_file_output_index());
    window.set_file_volume(ui.get_audio_file_volume());
    window.set_mic_volume(ui.get_microphone_input_volume());
    window.set_mix_mic(ui.get_mix_mic());
    window.set_running(ui.get_running());
}
