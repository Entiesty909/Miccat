//! Slint 桥：分页界面、列表填充、状态轮询、热键。

use slint::{ModelRc, VecModel};
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
use dialogs::select_audio_file;
use format::format_hns_position;
use input::start_hotkey;
use library::{
    audio_categories, audio_category_counts, category_leaf, category_paths, update_audio_list,
    visible_entry,
};
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
    let saved_mics = Rc::new(RefCell::new(Vec::<(u32, String)>::new()));
    engine.set_monitor_default(persisted.preview_enabled);
    engine.set_monitor_volume(persisted.preview_volume);
    let hotkey = Rc::new(RefCell::new(persisted.hotkeys.toggle_route));
    engine.set_volume(persisted.mix.process);
    engine.set_mic_volume(persisted.mix.mic);
    ui.set_master_volume((persisted.mix.master * 100.0).clamp(0.0, 100.0));
    engine.set_master_volume(persisted.mix.master);
    engine.set_sfx_volume(persisted.mix.sfx);
    ui.set_mix_mic(persisted.route.mix_mic);
    ui.window()
        .show()
        .map_err(|e| Error::InvalidArgs(e.to_string()))?;
    fit_window_to_screen(&ui);
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
    ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
    ui.set_audio_category_counts(ModelRc::new(VecModel::from(audio_category_counts(&cache))));
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
}
