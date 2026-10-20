//! 填充进程、麦克风与输出设备列表。

use super::*;
use crate::audio::device;
use crate::process;
use crate::vbcable;

/// 重新枚举进程、麦克风、输出设备。`initial` 为真时自动挑一只虚拟麦。
pub(super) fn refill(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>, initial: bool) {
    // 三组模型按同一轮枚举顺序更新，避免进程、麦克风和输出设备在刷新期间彼此错位。
    refill_processes(ui, cache);
    refill_mics(ui, cache);
    refill_devices(ui, cache, initial);
}

/// 在 UI 线程同步枚举当前进程，并把失败转换为用户可见诊断。
pub(super) fn refill_processes(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let list = match process::list_processes() {
        Ok(list) => list,
        Err(error) => {
            tracing::warn!("list processes: {error}");
            super::diagnostics::report_error(ui, &error);
            return;
        }
    };
    apply_process_list(ui, cache, list);
}

/// 将后台或同步枚举结果写入缓存与 Slint 模型，同时尽量保持原选中 PID。
pub(super) fn apply_process_list(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    list: Vec<process::ProcessInfo>,
) {
    let selected_pid = {
        // UI 使用列表下标，缓存同时保留 PID；刷新时以 PID 恢复选择而不是依赖旧下标。
        let state = cache.borrow();
        state.pids.get(ui.get_process_index() as usize).copied()
    };
    // Slint 的显示文本与缓存的 PID 分开保存，后续事件用 PID 回查而不是依赖名称唯一。
    let labels: Vec<slint::SharedString> = list
        .iter()
        .map(|process| process.name.clone().into())
        .collect();
    let pids: Vec<u32> = list.iter().map(|process| process.pid).collect();
    let process_pids: Vec<slint::SharedString> = list
        .iter()
        .map(|process| process.pid.to_string().into())
        .collect();
    let process_icon_images = list
        .iter()
        .map(|process| crate::platform::icon::process_icon(process.pid))
        .collect::<Vec<_>>();
    let selected_index = selected_pid
        .and_then(|pid| pids.iter().position(|candidate| *candidate == pid))
        .unwrap_or(0);
    {
        // 进程消失后清理批量选择状态，防止 UI 继续向已不存在的对象发送命令。
        let mut cache = cache.borrow_mut();
        cache.selected_pids.retain(|pid| pids.contains(pid));
        cache.disabled_pids.retain(|pid| pids.contains(pid));
        cache.processes = list;
        cache.pids = pids;
    }
    ui.set_processes(ModelRc::new(VecModel::from(labels)));
    ui.set_process_pids(ModelRc::new(VecModel::from(process_pids)));
    ui.set_process_icon_images(ModelRc::new(VecModel::from(process_icon_images)));
    ui.set_process_index(selected_index as i32);
    ui.set_about_text(about_text(cache).into());
}

/// 枚举物理麦克风；索引零保留给“不混入麦克风”的哨兵项。
fn refill_mics(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    // 设备枚举失败只更新诊断，不清空已有模型；这样暂时性枚举失败不会破坏当前路由。
    match device::list_physical_mics() {
        Ok(list) => apply_mics(ui, cache, list),
        Err(error) => {
            tracing::warn!("list mics: {error}");
            super::diagnostics::report_error(ui, &error);
        }
    }
}

/// 重建麦克风模型，并按不透明设备 ID 恢复原选择。
pub(super) fn apply_mics(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    microphones: Vec<device::RenderDevice>,
) {
    let kept = {
        let cache = cache.borrow();
        cache.mic_ids.get(ui.get_mic_index() as usize).cloned()
    };
    let mut labels: Vec<slint::SharedString> = vec!["不混入麦克风".into()];
    // 下标必须在 labels、ids、names 三个并行数组中保持一致，空首项作为“禁用麦克风”哨兵。
    let mut ids = vec![String::new()];
    let mut names = vec![String::new()];
    for microphone in microphones {
        labels.push(microphone.name.clone().into());
        names.push(microphone.name);
        ids.push(microphone.id);
    }
    let index = kept
        .as_deref()
        .and_then(|id| ids.iter().position(|candidate| candidate == id))
        .unwrap_or(0);
    {
        let mut cache = cache.borrow_mut();
        cache.mic_ids = ids;
        cache.mic_names = names;
    }
    ui.set_mics(ModelRc::new(VecModel::from(labels)));
    ui.set_mic_index(index as i32);
}

/// 按当前“显示全部设备”开关枚举输出端点。
pub(super) fn refill_devices(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>, initial: bool) {
    let include_all = ui.get_all_devices();
    // 枚举参数取自 UI 开关；真正的设备模型替换统一由 apply_destinations 完成。
    match device::list_destinations(include_all) {
        Ok(destinations) => apply_destinations(ui, cache, destinations, initial),
        Err(error) => {
            tracing::warn!("list devices: {error}");
            super::diagnostics::report_error(ui, &error);
        }
    }
}

/// 写入输出设备模型，维护设备 ID 选择集合和初次启动的虚拟麦默认值。
pub(super) fn apply_destinations(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    destinations: Vec<device::DestDevice>,
    initial: bool,
) {
    let kept_id = {
        // 输出端点的 render_id 是稳定身份，label 只作显示与旧配置的兜底匹配。
        let cache = cache.borrow();
        cache
            .dests
            .get(ui.get_device_index() as usize)
            .map(|device| device.render_id.clone())
    };
    cache.borrow_mut().dests = destinations;
    // 先替换缓存再构造模型，后面的默认选择、设备多选和说明文字都读取同一份权威列表。
    let (labels, index, cable, picked_is_mic) = {
        let cache = cache.borrow();
        let labels = cache
            .dests
            .iter()
            .map(|device| device.label.clone().into())
            .collect::<Vec<slint::SharedString>>();
        let index = kept_id
            .as_deref()
            .and_then(|id| cache.dests.iter().position(|device| device.render_id == id))
            .or_else(|| {
                if initial {
                    cache
                        .dests
                        .iter()
                        .position(|device| device.capture_id.is_some())
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let cable = cache.dests.iter().any(|device| {
            vbcable::device_looks_like_cable(&device.label) || device.capture_id.is_some()
        });
        let picked_is_mic = cache
            .dests
            .get(index)
            .is_some_and(|device| device.capture_id.is_some());
        (labels, index, cable, picked_is_mic)
    };
    // 多选状态按设备 ID 投影到当前排序后的模型，避免设备刷新后错位。
    let selected = {
        let cache = cache.borrow();
        cache
            .dests
            .iter()
            .map(|device| cache.selected_device_ids.contains(&device.render_id))
            .collect::<Vec<_>>()
    };
    cache.borrow_mut().device_selected = selected.clone();
    ui.set_device_selected(ModelRc::new(VecModel::from(selected)));
    ui.set_devices(ModelRc::new(VecModel::from(labels)));
    ui.set_device_index(index as i32);
    if cable {
        ui.set_cable_text("VB-CABLE 已安装".into());
    } else if !ui.get_cable_busy() {
        ui.set_cable_text("未检测到 VB-CABLE".into());
    }
    ui.set_cable_installed(cable);
    if initial && kept_id.is_none() && picked_is_mic {
        ui.set_set_default_mic(true);
    }
    update_output_text(ui, cache);
    ui.set_about_text(about_text(cache).into());
}

/// 输出名称、说明短句、是否显示「设为默认麦」勾选。
pub(super) fn update_output_text(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let c = cache.borrow();
    // 说明文字依赖当前设备的 capture_id：有捕获端点时它同时代表可供游戏选择的虚拟麦。
    match c.dests.get(ui.get_device_index() as usize) {
        Some(d) => {
            ui.set_show_set_default_mic(d.capture_id.is_some());
            let hint = match d.capture_name.as_deref() {
                Some(name) => format!("虚拟麦克风「{name}」。游戏里把麦克风选成它，队友就能听到。"),
                None => "虚拟扬声器。声音播到这只设备。".to_string(),
            };
            ui.set_dest_hint(hint.into());
        }
        None => {
            ui.set_show_set_default_mic(false);
            ui.set_dest_hint("输出列表是虚拟扬声器与虚拟麦克风。".into());
        }
    }
}

/// 生成关于当前枚举数量的版本和诊断页摘要。
pub(super) fn about_text(cache: &Rc<RefCell<UiCache>>) -> String {
    let c = cache.borrow();
    format!(
        "版本 {} · 输出设备 {} 个 · 进程 {} 个",
        env!("CARGO_PKG_VERSION"),
        c.dests.len(),
        c.processes.len()
    )
}
