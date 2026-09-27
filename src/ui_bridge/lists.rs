//! 填充进程、麦克风与输出设备列表。

use super::*;
use crate::audio::device;
use crate::process::{self, ProcessInfo};
use crate::vbcable;

/// 重新枚举进程、麦克风、输出设备。`initial` 为真时自动挑一只虚拟麦。
pub(super) fn refill(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>, initial: bool) {
    refill_processes(ui, cache);
    refill_mics(ui, cache);
    refill_devices(ui, cache, initial);
}

pub(super) fn refill_processes(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    match process::list_processes() {
        Ok(list) => cache.borrow_mut().processes = list,
        Err(e) => {
            tracing::warn!("list processes: {e}");
            ui.set_error_text(e.user_message().into());
            return;
        }
    }
    apply_process_filter(ui, cache);
}

/// 按关键字重建进程下拉，尽量保住当前选中的 PID。
fn apply_process_filter(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let c = cache.borrow();
    let kept = c.pids.get(ui.get_process_index() as usize).copied();
    let needle = c.filter.trim().to_ascii_lowercase();
    let mut labels: Vec<slint::SharedString> = Vec::new();
    let mut pids: Vec<u32> = Vec::new();
    for p in &c.processes {
        if !needle.is_empty() && !process_matches(p, &needle) {
            continue;
        }
        labels.push(p.label().into());
        pids.push(p.pid);
    }
    let idx = kept
        .and_then(|pid| pids.iter().position(|v| *v == pid))
        .unwrap_or(0);
    drop(c);

    {
        let mut c = cache.borrow_mut();
        c.pids = pids;
    }
    ui.set_processes(ModelRc::new(VecModel::from(labels)));
    ui.set_process_index(idx as i32);
    ui.set_about_text(about_text(cache).into());
}

/// 进程名或窗口标题命中关键字。
fn process_matches(p: &ProcessInfo, needle: &str) -> bool {
    p.name.to_ascii_lowercase().contains(needle)
        || p.title
            .as_deref()
            .is_some_and(|t| t.to_ascii_lowercase().contains(needle))
}

fn refill_mics(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let kept = {
        let c = cache.borrow();
        c.mic_ids.get(ui.get_mic_index() as usize).cloned()
    };
    let mut labels: Vec<slint::SharedString> = vec!["不混入麦克风".into()];
    let mut ids: Vec<String> = vec![String::new()];
    let mut names: Vec<String> = vec![String::new()];
    match device::list_physical_mics() {
        Ok(list) => {
            for m in list {
                labels.push(m.name.clone().into());
                names.push(m.name.clone());
                ids.push(m.id);
            }
        }
        Err(e) => tracing::warn!("list mics: {e}"),
    }
    let idx = kept
        .as_deref()
        .and_then(|id| ids.iter().position(|v| v == id))
        .unwrap_or(0);
    {
        let mut c = cache.borrow_mut();
        c.mic_ids = ids;
        c.mic_names = names;
    }

    ui.set_mics(ModelRc::new(VecModel::from(labels)));
    ui.set_mic_index(idx as i32);
    ui.set_has_mic(idx != 0);
    update_mic_text(ui, cache);
}

/// 把当前选中的麦克风名同步给路由页。
pub(super) fn update_mic_text(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let name = {
        let c = cache.borrow();
        c.mic_names
            .get(ui.get_mic_index() as usize)
            .cloned()
            .unwrap_or_default()
    };
    ui.set_mic_name(name.into());
}

pub(super) fn refill_devices(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>, initial: bool) {
    let include_all = cache.borrow().all_devices;
    let kept_id = {
        let c = cache.borrow();
        c.dests
            .get(ui.get_device_index() as usize)
            .map(|d| d.render_id.clone())
    };
    match device::list_destinations(include_all) {
        Ok(list) => cache.borrow_mut().dests = list,
        Err(e) => {
            tracing::warn!("list devices: {e}");
            ui.set_error_text(e.user_message().into());
            return;
        }
    }

    let (labels, idx, cable, picked_is_mic) = {
        let c = cache.borrow();
        let labels: Vec<slint::SharedString> =
            c.dests.iter().map(|d| d.label.clone().into()).collect();
        let idx = kept_id
            .as_deref()
            .and_then(|id| c.dests.iter().position(|d| d.render_id == id))
            .or_else(|| {
                if initial {
                    c.dests.iter().position(|d| d.capture_id.is_some())
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let cable = c
            .dests
            .iter()
            .any(|d| vbcable::device_looks_like_cable(&d.label) || d.capture_id.is_some());
        let picked_is_mic = c.dests.get(idx).is_some_and(|d| d.capture_id.is_some());
        (labels, idx, cable, picked_is_mic)
    };

    let selected = {
        let c = cache.borrow();
        if c.device_selected.len() == c.dests.len() {
            c.device_selected.clone()
        } else {
            vec![false; c.dests.len()]
        }
    };
    let selected = if selected.iter().any(|v| *v) {
        selected
    } else {
        vec![false; cache.borrow().dests.len()]
    };
    cache.borrow_mut().device_selected = selected.clone();
    ui.set_device_selected(ModelRc::new(VecModel::from(selected)));
    ui.set_devices(ModelRc::new(VecModel::from(labels)));
    ui.set_device_index(idx as i32);
    if cable {
        ui.set_cable_text("VB-CABLE 已安装".into());
    } else if !ui.get_cable_busy() {
        ui.set_cable_text("未检测到 VB-CABLE".into());
    }
    if initial && kept_id.is_none() && picked_is_mic {
        cache.borrow_mut().set_default_mic = true;
        ui.set_set_default_mic(true);
    }
    update_output_text(ui, cache);
    ui.set_about_text(about_text(cache).into());
}

/// 输出名称、说明短句、是否显示「设为默认麦」勾选。
pub(super) fn update_output_text(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let c = cache.borrow();
    match c.dests.get(ui.get_device_index() as usize) {
        Some(d) => {
            ui.set_output_name(d.label.clone().into());
            ui.set_show_set_default_mic(d.capture_id.is_some());
            let hint = match d.capture_name.as_deref() {
                Some(name) => format!("虚拟麦克风「{name}」。游戏里把麦克风选成它，队友就能听到。"),
                None => "虚拟扬声器。声音播到这只设备。".to_string(),
            };
            ui.set_dest_hint(hint.into());
        }
        None => {
            ui.set_output_name("".into());
            ui.set_show_set_default_mic(false);
            ui.set_dest_hint("输出列表是虚拟扬声器与虚拟麦克风。".into());
        }
    }
}

pub(super) fn about_text(cache: &Rc<RefCell<UiCache>>) -> String {
    let c = cache.borrow();
    format!(
        "版本 {} · 输出设备 {} 个 · 进程 {} 个",
        env!("CARGO_PKG_VERSION"),
        c.dests.len(),
        c.processes.len()
    )
}
