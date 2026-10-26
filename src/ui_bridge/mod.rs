//! Slint 桥接组合根：初始化窗口、装配回调、运行事件循环并执行退出收尾。

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
mod theme;
mod window;
mod window_lifecycle;

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
    // 子窗口先创建并长期复用；关闭时只隐藏，避免重复注册 Slint 回调。
    let ui = MainWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?;
    let add_source_window =
        Rc::new(AddSourceWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?);
    let audio_edit_window =
        Rc::new(AudioEditWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?);
    // AudioEngine 持有真实音频运行态，UI 只保存其显示投影。
    let engine = Rc::new(AudioEngine::new());
    // AppConfig 是唯一持久化权威；后面的 clone 只作为启动快照。
    let app_config = Rc::new(RefCell::new(config::load()));
    if hydrate_audio_durations(&app_config) {
        config::save(&app_config.borrow());
    }
    // 所有 UI 写盘都进入防抖队列，避免滑块事件同步阻塞事件循环。
    let save_queue = SaveQueue::new(app_config.clone())?;
    let persisted = app_config.borrow().clone();
    // 顶栏版本直接来自 Cargo package，避免 UI 文案与实际构建版本漂移。
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    ui.set_all_devices(persisted.output.all_devices);
    ui.set_set_default_render(persisted.output.set_default_render);
    ui.set_set_default_communications(persisted.output.set_default_communications);
    let cache = Rc::new(RefCell::new(UiCache {
        selected_device_ids: persisted.output.device_ids.clone(),
        device_selected: Vec::new(),
        selected_pids: Vec::new(),
        process_volumes: persisted.mix.process_volumes.clone(),
        audio_volumes: persisted.mix.sfx_volumes.clone(),
        audio_categories: persisted.audio_categories.clone(),
        audio_entries: persisted.audio_entries.clone(),
        audio_sort_ascending: true,
        audio_category: crate::library::category_paths(
            &persisted.audio_categories,
            &persisted.audio_entries,
        )
        .first()
        .cloned()
        .unwrap_or_default(),
        ..UiCache::default()
    }));
    // ListRefresh 负责把设备/进程枚举结果从 worker 安全交回 UI 线程。
    let list_refresh = Arc::new(state::ListRefresh::default());
    let windows = Rc::new(state::AppWindows {
        add_source: add_source_window,
        audio_edit: audio_edit_window,
    });
    // 首帧使用经过范围校验的持久化主题；非法旧值修正后进入防抖写盘。
    let theme_index = persisted.theme_index.clamp(0, 2);
    if theme_index != persisted.theme_index {
        app_config.borrow_mut().theme_index = theme_index;
        save_queue.schedule();
    }
    theme::initialize(&ui, &windows, theme_index);
    let saved_mics = Rc::new(RefCell::new(Vec::<(u32, String)>::new()));
    let hotkey = Rc::new(RefCell::new(persisted.hotkeys.toggle_route));
    engine.set_volume(persisted.mix.process);
    engine.set_mic_volume(persisted.mix.mic);
    engine.set_mic_preview_volume(persisted.mix.mic_preview_volume);
    ui.set_master_volume((persisted.mix.master * 100.0).clamp(0.0, 100.0));
    engine.set_master_volume(persisted.mix.master);
    engine.set_sfx_volume(persisted.mix.sfx);
    engine.set_monitor_volume(persisted.preview_volume);
    ui.set_preview_volume((persisted.preview_volume * 100.0).clamp(0.0, 100.0));
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
    window_lifecycle::refresh_window_after_show(&ui);
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
    // 麦克风本机试听可以在未启动转发时工作；设备 ID 来自已恢复的真实录音端。
    let preview_mic = cache
        .borrow()
        .mic_ids
        .get(ui.get_mic_index().max(0) as usize)
        .filter(|id| !id.is_empty())
        .cloned();
    if let Err(error) = engine.set_mic_preview_source(preview_mic.as_deref()) {
        tracing::warn!("恢复麦克风本机试听失败：{error}");
    }
    library::update_audio_categories(&ui, &cache, &app_config.borrow());
    update_audio_list(&ui, &cache);

    let hotkey_server = Rc::new(start_hotkey(
        &ui,
        &hotkey,
        input::collect_bindings(&app_config.borrow()),
    ));
    // AppState 把同一组权威对象交给所有回调，避免回调各自复制状态。
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
    theme::register(&ui, &state);
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

    // 事件循环返回后按固定顺序 flush 配置、恢复默认麦并回收音频线程。
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
    window_lifecycle::register(ui, state);
}
