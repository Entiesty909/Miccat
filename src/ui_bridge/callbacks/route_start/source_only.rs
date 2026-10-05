//! 路由页单源启动：绕过主路由按钮，只转发一个进程、文件或麦克风。

use super::super::super::*;
use crate::config;
use crate::engine::AudioEngine;
use slint::Model;
use std::cell::RefCell;
use std::rc::Rc;

/// 只启动一个路由源，绕过底部主按钮的多源选择。
pub(crate) fn start_source_only(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    app_config: &Rc<RefCell<config::AppConfig>>,
    engine: &AudioEngine,
    kind: usize,
    source_index: i32,
) {
    let output_id = {
        let cache_state = cache.borrow();
        let config = app_config.borrow();
        let configured = match kind {
            0 => config.output.process_output_id.as_deref(),
            1 => config.output.file_output_id.as_deref(),
            _ => config.output.mic_output_id.as_deref(),
        };
        configured.map(str::to_owned).or_else(|| {
            cache_state
                .dests
                .get(ui.get_device_index().max(0) as usize)
                .map(|device| device.render_id.clone())
        })
    };
    let Some(output_id) = output_id else {
        ui.set_error_text("请选择输出设备".into());
        return;
    };
    if kind == 1 {
        cache.borrow_mut().file_source_hidden = false;
    }
    if kind == 2 {
        cache.borrow_mut().mic_source_hidden = false;
    }
    let file_state = engine.file_state();
    engine.stop();
    let routes = crate::engine::SourceOutputRoutes {
        process: 0,
        file: 0,
        mic: 0,
    };
    if let Err(error) = engine.open_routed_outputs(&[output_id], routes) {
        tracing::warn!("单独转发打开输出失败：{error}");
        super::super::super::diagnostics::report_error(ui, &error);
        return;
    }
    let result = match kind {
        0 => source_process_pid(ui, cache, source_index)
            .ok_or(Error::ProcessNotFound(0))
            .and_then(|pid| engine.add_process_source(pid, 0)),
        1 => source_file_entry(ui, cache, &file_state.path)
            .ok_or_else(|| Error::InvalidArgs("没有可单独转发的音频文件".into()))
            .and_then(|entry| {
                let start = if file_state.path == entry.path {
                    file_state.position_hns
                } else {
                    0
                };
                engine.play_file(&entry, start, crate::engine::PlaybackTarget::Route)
            }),
        2 => source_mic_id(ui, cache)
            .ok_or_else(|| Error::InvalidArgs("没有选择麦克风".into()))
            .and_then(|id| engine.set_mic_source(Some(&id))),
        _ => Err(Error::InvalidArgs("未知音频源".into())),
    };
    if let Err(error) = result {
        engine.stop();
        tracing::warn!("单独转发失败：{error}");
        ui.set_running(false);
        super::super::super::diagnostics::report_error(ui, &error);
        return;
    }
    ui.set_running(true);
    ui.set_status_text("单独转发中".into());
    ui.set_status_kind(1);
    ui.set_error_text("".into());
}

fn source_process_pid(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>, index: i32) -> Option<u32> {
    let index = usize::try_from(index).ok()?;
    let kinds = ui.get_source_kinds();
    let slot = (0..index)
        .filter(|row| kinds.row_data(*row) == Some(0))
        .count();
    let cache = cache.borrow();
    cache
        .selected_pids
        .iter()
        .filter(|pid| cache.pids.contains(pid) && !cache.disabled_pids.contains(pid))
        .nth(slot)
        .copied()
}

fn source_mic_id(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) -> Option<String> {
    cache
        .borrow()
        .mic_ids
        .get(ui.get_mic_index().max(0) as usize)
        .filter(|id| !id.is_empty())
        .cloned()
}

fn source_file_entry(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    file_path: &str,
) -> Option<config::AudioEntry> {
    if !file_path.is_empty() {
        return cache
            .borrow()
            .audio_entries
            .iter()
            .find(|entry| entry.path == file_path)
            .cloned();
    }
    let index = ui.get_audio_entry_index().max(0) as usize;
    let actual = crate::ui_bridge::library::visible_entry(cache, index)?;
    cache.borrow().audio_entries.get(actual).cloned()
}
