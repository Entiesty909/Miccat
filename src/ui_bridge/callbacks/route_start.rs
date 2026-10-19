//! 路由启停回调与启动阶段。

use super::super::*;
use crate::audio::device::{self, DestDevice};
use crate::audio::policy;
mod source_only;
pub(super) use source_only::start_source_only;

struct RouteSelection {
    pid: Option<u32>,
    extra_pids: Vec<u32>,
    dest: DestDevice,
    output_ids: Vec<String>,
    source_routes: Option<crate::engine::SourceOutputRoutes>,
    mic_id: Option<String>,
    set_default_mic: bool,
    process_name: Option<String>,
    process_names: Vec<String>,
    mix_mic: bool,
    file_entries: Vec<(usize, config::AudioEntry)>,
}

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    let ui_weak = ui.as_weak();
    let state = state.clone();
    ui.on_start_stop(move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        handle_start_stop(&ui, &state);
    });
}

fn handle_start_stop(ui: &MainWindow, state: &AppState) {
    if state.engine.is_running() {
        stop_route(ui, state);
        return;
    }

    let Some(selection) = selected_route(ui, state) else {
        ui.set_error_text("请选择音频源和输出设备".into());
        return;
    };
    let mix = state.app_config.borrow().mix.clone();
    state.engine.set_volume(mix.process);
    state.engine.set_mic_volume(mix.mic);
    state.engine.set_master_volume(mix.master);

    match start_route(&state.engine, &selection) {
        Ok(()) => finish_route_start(ui, state, selection),
        Err(error) => fail_route_start(ui, state, error),
    }
}

fn stop_route(ui: &MainWindow, state: &AppState) {
    state.engine.stop();
    restore_default_mics(&state.saved_mics);
    ui.set_running(false);
    ui.set_status_text("未运行".into());
    ui.set_status_kind(0);
    ui.set_error_text("".into());
}

fn selected_route(ui: &MainWindow, state: &AppState) -> Option<RouteSelection> {
    let cache = state.cache.borrow();
    let pid = cache
        .pids
        .get(ui.get_process_index() as usize)
        .copied()
        .filter(|pid| *pid != 0 && !cache.disabled_pids.contains(pid));
    let mut selected = cache
        .selected_pids
        .iter()
        .copied()
        .filter(|pid| !cache.disabled_pids.contains(pid))
        .collect::<Vec<_>>();
    if selected.is_empty()
        && let Some(pid) = pid
    {
        selected.push(pid);
    }
    let primary_pid = selected.first().copied().or(pid);
    let extra_pids = selected.iter().skip(1).copied().collect();
    let file_entries = if cache.route_audio_entries.is_empty() {
        cache
            .route_file_enabled
            .then_some(cache.route_audio_entry)
            .flatten()
            .and_then(|index| cache.audio_entries.get(index).cloned())
            .map(|entry| vec![(0, entry)])
            .unwrap_or_default()
    } else {
        cache
            .route_audio_entries
            .iter()
            .enumerate()
            .filter(|(slot, _)| {
                cache
                    .route_audio_enabled
                    .get(*slot)
                    .copied()
                    .unwrap_or(false)
            })
            .filter_map(|(slot, index)| {
                cache
                    .audio_entries
                    .get(*index)
                    .cloned()
                    .map(|entry| (slot, entry))
            })
            .collect::<Vec<_>>()
    };
    let dest = cache.dests.get(ui.get_device_index() as usize).cloned()?;
    let output_config = state.app_config.borrow().output.clone();
    let custom_routes = !cache.process_output_ids.is_empty()
        || !cache.audio_output_ids.is_empty()
        || output_config.process_output_id.is_some()
        || output_config.mic_output_id.is_some();
    let (output_ids, source_routes) = if custom_routes {
        let mut output_ids = Vec::new();
        let mut route_index = |id: String| {
            output_ids
                .iter()
                .position(|existing| existing == &id)
                .unwrap_or_else(|| {
                    output_ids.push(id);
                    output_ids.len() - 1
                })
        };
        let process_count = selected.len();
        let process = (0..process_count)
            .map(|slot| {
                cache
                    .process_output_ids
                    .get(slot)
                    .and_then(|id| id.clone())
                    .or_else(|| output_config.process_output_id.clone())
                    .unwrap_or_else(|| dest.render_id.clone())
            })
            .map(&mut route_index)
            .collect::<Vec<_>>();
        let file_count = cache
            .route_audio_entries
            .len()
            .max(usize::from(cache.route_audio_entry.is_some()))
            .max(crate::engine::SFX_SLOTS);
        let file = (0..file_count)
            .map(|slot| {
                cache
                    .audio_output_ids
                    .get(slot)
                    .and_then(|id| id.clone())
                    .unwrap_or_else(|| dest.render_id.clone())
            })
            .map(&mut route_index)
            .collect::<Vec<_>>();
        let mic_id = output_config
            .mic_output_id
            .clone()
            .unwrap_or_else(|| dest.render_id.clone());
        let mic = route_index(mic_id);
        (
            output_ids,
            Some(crate::engine::SourceOutputRoutes { process, file, mic }),
        )
    } else {
        let selected_outputs = cache
            .dests
            .iter()
            .enumerate()
            .filter(|(index, _)| cache.device_selected.get(*index).copied().unwrap_or(false))
            .map(|(_, device)| device.render_id.clone())
            .collect::<Vec<_>>();
        let output_ids = if selected_outputs.is_empty() {
            vec![dest.render_id.clone()]
        } else {
            selected_outputs
        };
        (output_ids, None)
    };
    let mic_index = ui.get_mic_index() as usize;
    let mic_id = if ui.get_mix_mic() {
        cache
            .mic_ids
            .get(mic_index)
            .filter(|id| !id.is_empty())
            .cloned()
    } else {
        None
    };
    let mix_mic = ui.get_mix_mic() && mic_id.is_some();
    if primary_pid.is_none() && file_entries.is_empty() && !mix_mic {
        return None;
    }
    let process_names = selected
        .iter()
        .filter_map(|pid| {
            cache
                .processes
                .iter()
                .find(|process| process.pid == *pid)
                .map(|process| process.name.clone())
        })
        .collect::<Vec<_>>();
    Some(RouteSelection {
        pid: primary_pid,
        extra_pids,
        dest,
        output_ids,
        source_routes,
        mic_id,
        set_default_mic: cache.set_default_mic,
        process_name: process_names.first().cloned(),
        process_names,
        mix_mic,
        file_entries,
    })
}

fn start_route(engine: &AudioEngine, selection: &RouteSelection) -> Result<(), Error> {
    if let Some(routes) = selection.source_routes.clone() {
        engine.open_routed_outputs(&selection.output_ids, routes)?;
    } else {
        engine.open_outputs(&selection.output_ids)?;
    }
    if let Some(pid) = selection.pid {
        engine.set_process_source(pid)?;
        for (slot, pid) in selection.extra_pids.iter().enumerate() {
            engine.add_process_source(*pid, slot + 1)?;
        }
    }
    engine.set_mic_source(selection.mic_id.as_deref())?;
    for (slot, entry) in &selection.file_entries {
        engine.play_file_source(entry, *slot)?;
    }
    Ok(())
}
fn finish_route_start(ui: &MainWindow, state: &AppState, selection: RouteSelection) {
    {
        let cache = state.cache.borrow();
        if cache.set_default_render {
            let _ = policy::set_default_render(&selection.dest.render_id);
        }
        if cache.set_default_communications {
            let _ = policy::set_default_communications(&selection.dest.render_id);
        }
    }
    {
        let mut config = state.app_config.borrow_mut();
        config.route.process_name = selection.process_name;
        config.route.process_names = selection.process_names;
        config.route.mix_mic = selection.mix_mic;
        config.output.device_id = Some(selection.dest.render_id.clone());
        config.output.device_ids = selection.output_ids.clone();
        if let Some(routes) = selection.source_routes.as_ref() {
            config.output.process_output_ids = routes
                .process
                .iter()
                .filter_map(|index| selection.output_ids.get(*index).cloned())
                .collect();
            config.output.file_output_ids = routes
                .file
                .iter()
                .filter_map(|index| selection.output_ids.get(*index).cloned())
                .collect();
            config.output.process_output_id = config.output.process_output_ids.first().cloned();
            config.output.file_output_id = config.output.file_output_ids.first().cloned();
            config.output.mic_output_id = selection.output_ids.get(routes.mic).cloned();
        }
        config.output.mic_id = selection.dest.capture_id.clone();
        config.output.mic_name = selection.dest.capture_name.clone();
        config.route.mic_id = selection.mic_id.clone();
        config.output.set_default_mic = selection.set_default_mic;
        state.save_queue.schedule();
    }
    ui.set_mix_mic(selection.mix_mic);
    ui.set_running(true);
    ui.set_status_text("运行中".into());
    ui.set_status_kind(1);
    ui.set_error_text("".into());
    apply_virtual_mic_route(
        ui,
        &selection.dest,
        selection.set_default_mic,
        &state.saved_mics,
    );
}

fn fail_route_start(ui: &MainWindow, state: &AppState, error: Error) {
    state.engine.stop();
    tracing::warn!("start failed: {error}");
    ui.set_running(false);
    ui.set_status_text("错误".into());
    ui.set_status_kind(3);
    super::super::diagnostics::report_error(ui, &error);
}

/// 开始转发时按勾选切换系统默认麦，失败只提示，不影响转发。
fn apply_virtual_mic_route(
    ui: &MainWindow,
    dest: &DestDevice,
    set_default: bool,
    saved_mics: &Rc<RefCell<Vec<(u32, String)>>>,
) {
    let Some(cap_id) = dest.capture_id.as_deref() else {
        ui.set_dest_hint("当前输出是虚拟扬声器，声音播到这只设备。".into());
        return;
    };
    let name = dest.capture_name.as_deref().unwrap_or(dest.label.as_str());
    if !set_default {
        ui.set_dest_hint(format!("已混进「{name}」。").into());
        return;
    }
    *saved_mics.borrow_mut() = device::default_capture_ids();
    if let Err(e) = policy::set_default_capture(cap_id) {
        tracing::warn!("set default mic: {e}");
        ui.set_error_text(format!("已开始转发，但没能改系统默认麦。请手动选择「{name}」。").into());
        ui.set_dest_hint(format!("请把麦克风改成「{name}」，关掉噪音抑制后重启。").into());
        return;
    }
    ui.set_dest_hint(format!("已把系统默认麦改成「{name}」。").into());
}
