//! 路由启停回调与启动阶段。

use super::super::*;
use crate::audio::device::{self, DestDevice};
use crate::audio::policy;

struct RouteSelection {
    pid: u32,
    extra_pids: Vec<u32>,
    dest: DestDevice,
    output_ids: Vec<String>,
    mic_id: Option<String>,
    set_default_mic: bool,
    process_name: Option<String>,
    process_names: Vec<String>,
    mix_mic: bool,
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
        ui.set_error_text("请选择进程和输出设备".into());
        return;
    };
    state.engine.set_volume(ui.get_volume() / 100.0);
    state.engine.set_mic_volume(ui.get_mic_volume() / 100.0);
    state
        .engine
        .set_master_volume(ui.get_master_volume() / 100.0);

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
        .filter(|pid| *pid != 0);
    let mut selected = cache.selected_pids.clone();
    if selected.is_empty() {
        if let Some(pid) = pid {
            selected.push(pid);
        }
    }
    let primary_pid = selected.first().copied().or(pid)?;
    let extra_pids = selected.iter().skip(1).copied().collect();
    let dest_index = ui.get_device_index() as usize;
    let dest = cache.dests.get(dest_index).cloned()?;
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
        mic_id,
        set_default_mic: cache.set_default_mic,
        process_name: process_names.first().cloned(),
        process_names,
        mix_mic: ui.get_mix_mic(),
    })
}

fn start_route(engine: &AudioEngine, selection: &RouteSelection) -> Result<(), Error> {
    engine.open_outputs(&selection.output_ids)?;
    engine.set_process_source(selection.pid)?;
    for (slot, pid) in selection.extra_pids.iter().enumerate() {
        engine.add_process_source(*pid, slot + 1)?;
    }
    engine.set_mic_source(selection.mic_id.as_deref())
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
        config.output.device_name = Some(selection.dest.label.clone());
        config.output.mic_id = selection.dest.capture_id.clone();
        config.output.mic_name = selection.dest.capture_name.clone();
        config.output.set_default_mic = selection.set_default_mic;
        state.save_queue.schedule();
    }
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
    ui.set_error_text(error.user_message().into());
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
