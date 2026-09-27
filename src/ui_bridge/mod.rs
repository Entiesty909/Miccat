//! Slint 桥：分页界面、列表填充、状态轮询、热键。

use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

use crate::config;
use crate::engine::AudioEngine;
use crate::error::Error;

slint::include_modules!();

mod cache;
mod callbacks;
mod dev;
mod dialogs;
mod format;
mod input;
mod library;
mod lists;
mod persist;
mod state;
mod status;
mod window;

use cache::UiCache;
pub(crate) use dev::snapshot_to_file;
use dialogs::select_audio_file;
use format::format_hns_position;
use input::start_hotkey;
use library::{audio_categories, category_leaf, category_paths, update_audio_list, visible_entry};
use lists::{refill, refill_devices, refill_processes, update_mic_text, update_output_text};
use persist::{
    SaveQueue, hydrate_audio_durations, persist_config, restore_default_mics,
    restore_saved_selection,
};
use state::AppState;
use status::{set_wave, start_status_timer};
use window::fit_window_to_screen;

/// 声纹柱数量。
const WAVE_BARS: usize = 32;
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
        device_selected: Vec::new(),
        selected_pids: Vec::new(),
        set_default_communications: persisted.output.set_default_communications,
        audio_categories: persisted.audio_categories.clone(),
        audio_entries: persisted.audio_entries.clone(),
        audio_category: "全部".into(),
        ..UiCache::default()
    }));
    let saved_mics = Rc::new(RefCell::new(Vec::<(u32, String)>::new()));
    let capture_hist = Rc::new(RefCell::new(vec![0.0f32; WAVE_BARS]));
    let mic_hist = Rc::new(RefCell::new(vec![0.0f32; WAVE_BARS]));
    let render_hist = Rc::new(RefCell::new(vec![0.0f32; WAVE_BARS]));
    engine.set_monitor_default(persisted.preview_enabled);
    engine.set_monitor_volume(persisted.preview_volume);
    let hotkey = Rc::new(RefCell::new(persisted.hotkeys.toggle_route));
    ui.set_volume((persisted.mix.process * 100.0).clamp(0.0, 100.0));
    ui.set_mic_volume((persisted.mix.mic * 100.0).clamp(0.0, 100.0));
    ui.set_master_volume((persisted.mix.master * 100.0).clamp(0.0, 100.0));
    ui.set_preview_enabled(persisted.preview_enabled);
    ui.set_preview_volume((persisted.preview_volume * 100.0).clamp(0.0, 100.0));
    ui.set_mix_mic(persisted.route.mix_mic);
    ui.window()
        .show()
        .map_err(|e| Error::InvalidArgs(e.to_string()))?;
    fit_window_to_screen(&ui);
    refill(&ui, &cache, true);
    restore_saved_selection(&ui, &cache, &persisted);
    ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
    update_audio_list(&ui, &cache);
    set_wave(
        &ui,
        &capture_hist.borrow(),
        &mic_hist.borrow(),
        &render_hist.borrow(),
    );

    let hotkey_server = Rc::new(start_hotkey(&ui, &hotkey));
    let state = AppState {
        engine: engine.clone(),
        cache: cache.clone(),
        saved_mics: saved_mics.clone(),
        hotkey: hotkey.clone(),
        hotkey_server: hotkey_server.clone(),
        app_config: app_config.clone(),
        save_queue: save_queue.clone(),
        capture_hist: capture_hist.clone(),
        mic_hist: mic_hist.clone(),
        render_hist: render_hist.clone(),
    };
    wire_callbacks(&ui, &state);
    let _timer = start_status_timer(&ui, &state);
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
}
