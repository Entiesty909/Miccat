//! 路由启停回调与启动阶段。

use super::super::*;
use crate::audio::device::{self, DestDevice};
use crate::audio::policy;
use crate::engine::PlaybackTarget;
/// 提供源卡片菜单触发的单源路由启动路径。
mod source_only;
pub(super) use source_only::start_source_only;

/// 启动一次路由所需的快照；构建完成后后台/引擎操作不再依赖易变的 UI 列表。
struct RouteSelection {
    /// 主进程源的 PID；它占用进程槽 0。
    pid: Option<u32>,
    /// 其余进程源的 PID，按顺序占用槽 1 起的槽位。
    extra_pids: Vec<u32>,
    /// UI 选中的目标设备，负责默认设备策略和提示文案。
    dest: DestDevice,
    /// 去重后的输出设备 ID 列表，供引擎创建共享输出。
    output_ids: Vec<String>,
    /// 每类源到输出 ID 列表的索引映射；存在时启用按槽路由。
    source_routes: Option<crate::engine::SourceOutputRoutes>,
    /// 路由中混入的物理麦克风 ID；没有混音时为空。
    mic_id: Option<String>,
    /// 是否把虚拟麦克风设为系统默认采集设备。
    set_default_mic: bool,
    /// 主进程名称，用于旧配置字段。
    process_name: Option<String>,
    /// 所有已添加进程卡片的名称（包括禁用卡片），持久化时保持原槽位顺序。
    process_names: Vec<String>,
    /// 是否实际启用麦克风混音；无有效设备时会被强制关闭。
    mix_mic: bool,
}

/// 绑定底部开始/停止回调；使用弱 UI 引用避免窗口销毁后闭包持有界面。
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
/// 保存引擎重建输出图前的路由卡片试听位置；库内播放不在此范围内。
fn route_preview(state: &AppState) -> Option<(usize, config::AudioEntry, u64)> {
    let cache = state.cache.borrow();
    let slot = cache.route_preview_slot?;
    let entry = cache
        .route_audio_entries
        .get(slot)
        .and_then(|index| cache.audio_entries.get(*index))?
        .clone();
    let file = state.engine.file_state();
    (file.playing && file.path == entry.path).then_some((slot, entry, file.position_hns))
}

/// 输出图启停后恢复本机试听，并仅把已启用卡片加入正在运行的路由。
fn resume_route_preview(
    ui: &MainWindow,
    state: &AppState,
    preview: Option<(usize, config::AudioEntry, u64)>,
    allow_route_file: bool,
) {
    let Some((slot, entry, position)) = preview else {
        return;
    };
    state.engine.ensure_monitor_default(true);
    if let Err(error) = state
        .engine
        .play_file(&entry, position, PlaybackTarget::Monitor)
    {
        state.cache.borrow_mut().route_preview_slot = None;
        ui.set_player_playing(false);
        super::super::diagnostics::report_error(ui, &error);
        return;
    }
    ui.set_player_playing(true);
    if allow_route_file
        && state.engine.is_running()
        && state.cache.borrow().route_audio_enabled.get(slot) == Some(&true)
        && let Err(error) = state
            .engine
            .play_file_source(&entry, slot, false, position, false)
    {
        super::super::diagnostics::report_error(ui, &error);
    }
}

/// 根据引擎运行状态选择停止或快照并启动一条路由。
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
    // 三类增益在创建源之前统一设置，保证本次路由的进程、麦克风和主输出使用同一快照。
    state.engine.set_volume(mix.process);
    state.engine.set_mic_volume(mix.mic);
    state.engine.set_master_volume(mix.master);

    let preview = route_preview(state);
    match start_route(&state.engine, &selection) {
        Ok(()) => {
            finish_route_start(ui, state, selection);
            resume_route_preview(ui, state, preview, true);
        }
        Err(error) => {
            fail_route_start(ui, state, error);
            resume_route_preview(ui, state, preview, false);
        }
    }
}

/// 停止所有路由源，并恢复启动前保存的系统默认麦克风。
fn stop_route(ui: &MainWindow, state: &AppState) {
    // 停止路由会释放本机监听端，先保存当前文件位置，停完后恢复独立试听。
    let preview = route_preview(state);
    state.engine.stop();
    resume_route_preview(ui, state, preview, false);
    restore_default_mics(&state.saved_mics);
    ui.set_running(false);
    ui.set_status_text("未运行".into());
    ui.set_status_kind(0);
    ui.set_error_text("".into());
}

/// 从缓存、UI 选择和配置中组装稳定的启动快照；返回空表示没有可转发源或输出。
fn selected_route(ui: &MainWindow, state: &AppState) -> Option<RouteSelection> {
    let output_preferences = state.app_config.borrow().output.clone();
    let cache = state.cache.borrow();
    // selected_pids 是多进程选择结果；没有显式选择时才回退到主进程卡片。
    let pid = cache
        .pids
        .get(ui.get_process_index() as usize)
        .copied()
        .filter(|pid| *pid != 0 && !cache.disabled_pids.contains(pid));
    let mut selected = cache
        .selected_pids
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, pid)| !cache.disabled_pids.contains(pid))
        .collect::<Vec<_>>();
    if cache.selected_pids.is_empty()
        && let Some(pid) = pid
    {
        selected.push((0, pid));
    }
    let primary_pid = selected.first().map(|(_, pid)| *pid);
    // 保留原卡片槽号；过滤禁用源只影响本次引擎选择，不挪动其它卡片的输出设置。
    let extra_pids = selected.iter().skip(1).map(|(_, pid)| *pid).collect();
    // 文件源只参与启用判断和输出选择，不在启动时创建播放流。
    let file_count = cache
        .route_audio_entries
        .len()
        .max(usize::from(cache.route_audio_entry.is_some()))
        .max(crate::engine::SFX_SLOTS);
    let enabled_files = (0..file_count)
        .map(|slot| file_source_enabled(&cache, slot))
        .collect::<Vec<_>>();
    let has_enabled_file_source = enabled_files.iter().any(|enabled| *enabled);
    // 目标设备是启动快照的必要条件，索引失效时不应启动半成品路由。
    let dest = cache.dests.get(ui.get_device_index() as usize).cloned()?;
    let output_config = state.app_config.borrow().output.clone();
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
    let mix_mic = mic_id.is_some();
    if primary_pid.is_none() && !has_enabled_file_source && !mix_mic {
        return None;
    }
    // 独立设备只由已启用卡片参与图构建；否则延用全局设备列表广播。
    let custom_routes = !output_config.process_output_ids.is_empty()
        || !output_config.file_output_ids.is_empty()
        || !output_config.mic_output_ids.is_empty();
    let (output_ids, source_routes) = if custom_routes {
        let selected_slots = selected.iter().map(|(slot, _)| *slot).collect::<Vec<_>>();
        let resolved =
            crate::ui_bridge::sources::render_output_config(&cache.dests, &output_config);
        let (output_ids, routes) = routed_outputs(
            &selected_slots,
            &enabled_files,
            &resolved,
            &dest.render_id,
            mix_mic,
        );
        (output_ids, Some(routes))
    } else {
        // 没有自定义槽路由时，设备勾选列表决定同一音频副本广播到哪些输出。
        let selected_outputs = cache
            .dests
            .iter()
            .enumerate()
            .filter(|(index, _)| cache.device_selected.get(*index).copied().unwrap_or(false))
            .map(|(_, device)| device.selection_id().to_owned())
            .collect::<Vec<_>>();
        let selected_outputs =
            crate::ui_bridge::sources::render_target_ids(&cache.dests, &selected_outputs);
        let output_ids = if selected_outputs.is_empty() {
            vec![dest.render_id.clone()]
        } else {
            selected_outputs
        };
        (output_ids, None)
    };
    let process_names = if cache.selected_pids.is_empty() {
        selected.iter().map(|(_, pid)| *pid).collect::<Vec<_>>()
    } else {
        cache.selected_pids.clone()
    }
    .iter()
    .filter_map(|pid| {
        cache
            .processes
            .iter()
            .find(|process| process.pid == *pid)
            .map(|process| process.name.clone())
    })
    .collect::<Vec<_>>();
    // 主进程名称用于旧配置字段；进程列表保留所有卡片，以便禁用卡片下次仍可恢复。
    let active_process_name = primary_pid.and_then(|pid| {
        cache
            .processes
            .iter()
            .find(|process| process.pid == pid)
            .map(|process| process.name.clone())
    });
    // 进程名称只用于持久化和 UI 展示，真正启动依据始终是 PID。
    Some(RouteSelection {
        pid: primary_pid,
        extra_pids,
        dest,
        output_ids,
        source_routes,
        mic_id,
        set_default_mic: output_preferences.set_default_mic,
        process_name: active_process_name,
        process_names,
        mix_mic,
    })
}

/// 按文件卡片当前启用状态与库条目有效性决定是否需要为该槽打开输出。
fn file_source_enabled(cache: &UiCache, slot: usize) -> bool {
    if cache.route_audio_entries.is_empty() {
        slot == 0
            && cache.route_file_enabled
            && cache
                .route_audio_entry
                .and_then(|index| cache.audio_entries.get(index))
                .is_some()
    } else {
        cache
            .route_audio_enabled
            .get(slot)
            .copied()
            .unwrap_or(false)
            && cache
                .route_audio_entries
                .get(slot)
                .and_then(|index| cache.audio_entries.get(*index))
                .is_some()
    }
}

/// 为实际启用的槽去重输出 ID；禁用槽保持空掩码，不触发设备冲突检查。
fn routed_outputs(
    selected_slots: &[usize],
    enabled_files: &[bool],
    output_config: &config::OutputConfig,
    dest_id: &str,
    mix_mic: bool,
) -> (Vec<String>, crate::engine::SourceOutputRoutes) {
    let mut output_ids = Vec::new();
    let mut route_index = |ids: &[String]| {
        let mut indices = Vec::with_capacity(ids.len());
        for id in ids {
            let index = output_ids
                .iter()
                .position(|existing| existing == id)
                .unwrap_or_else(|| {
                    output_ids.push(id.clone());
                    output_ids.len() - 1
                });
            if !indices.contains(&index) {
                indices.push(index);
            }
        }
        indices
    };
    // 旧配置缺少某个槽时使用全局输出；已有槽的空选择不隐式广播。
    let fallback = if output_config.device_ids.is_empty() {
        vec![dest_id.to_owned()]
    } else {
        output_config.device_ids.clone()
    };
    let process = selected_slots
        .iter()
        .map(|slot| {
            let ids = output_config
                .process_output_ids
                .get(*slot)
                .unwrap_or(&fallback);
            route_index(ids)
        })
        .collect::<Vec<_>>();
    let file = enabled_files
        .iter()
        .enumerate()
        .map(|(slot, enabled)| {
            if !enabled {
                return Vec::new();
            }
            let ids = output_config.file_output_ids.get(slot).unwrap_or(&fallback);
            route_index(ids)
        })
        .collect::<Vec<_>>();
    let mic = if mix_mic {
        let ids = if output_config.mic_output_ids.is_empty() {
            &fallback
        } else {
            &output_config.mic_output_ids
        };
        route_index(ids)
    } else {
        Vec::new()
    };
    (
        output_ids,
        crate::engine::SourceOutputRoutes { process, file, mic },
    )
}

/// 按快照打开输出，再按固定槽位安装进程、麦克风和文件源。
fn start_route(engine: &AudioEngine, selection: &RouteSelection) -> Result<(), Error> {
    // 先打开全部输出，再安装源；这样每个源创建时都能拿到已准备好的目标槽位。
    if let Some(routes) = selection.source_routes.clone() {
        engine.open_routed_outputs(&selection.output_ids, routes)?;
    } else {
        engine.open_outputs(&selection.output_ids)?;
    }
    if let Some(pid) = selection.pid {
        // 进程槽 0 由主源占用，附加进程从槽 1 开始，避免覆盖主源状态。
        engine.set_process_source(pid)?;
        for (slot, pid) in selection.extra_pids.iter().enumerate() {
            engine.add_process_source(*pid, slot + 1)?;
        }
    }
    engine.set_mic_source(selection.mic_id.as_deref())?;
    // 启动只建立路由和输入源；启用的音频文件仍由用户通过卡片播放按钮启动。
    Ok(())
}
/// 启动成功后回写配置/UI，并按用户选项应用系统默认设备策略。
fn finish_route_start(ui: &MainWindow, state: &AppState, selection: RouteSelection) {
    {
        // 系统默认设备策略属于启动后的副作用，失败不回滚已经建立的转发流。
        let output_preferences = state.app_config.borrow().output.clone();
        if output_preferences.set_default_render {
            let _ = policy::set_default_render(&selection.dest.render_id);
        }
        if output_preferences.set_default_communications {
            let _ = policy::set_default_communications(&selection.dest.render_id);
        }
    }
    {
        // 启动仅保存主目标和源启用状态；卡片目标在用户选择时已按原槽位写入配置。
        // 运行图里禁用槽指向占位目标，不能把这个目标写回禁用卡片。
        let mut config = state.app_config.borrow_mut();
        config.route.process_name = selection.process_name;
        config.route.process_names = selection.process_names;
        config.route.mix_mic = selection.mix_mic;
        config.output.device_id = Some(selection.dest.render_id.clone());
        // 用户勾选的列表端点可与另一项共用 render；不能用去重后的运行图覆盖选择。
        config.output.device_ids = state.cache.borrow().selected_device_ids.clone();
        config.output.mic_id = selection.dest.capture_id.clone();
        config.output.mic_name = selection.dest.capture_name.clone();
        config.route.mic_id = selection.mic_id.clone();
        config.output.set_default_mic = selection.set_default_mic;
        state.save_queue.schedule();
        // 配置写入由 SaveQueue 合并落盘，避免每个 UI 字段更新都同步阻塞磁盘。
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

/// 启动任一阶段失败时停止引擎并把底层错误转换为 UI 可见诊断。
fn fail_route_start(ui: &MainWindow, state: &AppState, error: Error) {
    // 任一阶段失败都要停掉已打开的部分输出，防止 UI 显示错误但后台仍在转发。
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
    // 虚拟输出可能没有可设为默认麦克风的 capture ID，但仍可正常播放转发音频。
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
    // 记录原默认采集设备，停止路由时才能恢复用户启动前的系统状态。
    if let Err(e) = policy::set_default_capture(cap_id) {
        tracing::warn!("set default mic: {e}");
        ui.set_error_text(format!("已开始转发，但没能改系统默认麦。请手动选择「{name}」。").into());
        ui.set_dest_hint(format!("请把麦克风改成「{name}」，关掉噪音抑制后重启。").into());
        return;
    }
    ui.set_dest_hint(format!("已把系统默认麦改成「{name}」。").into());
}

#[cfg(test)]
mod tests {
    use super::routed_outputs;
    use crate::config::OutputConfig;

    /// 关闭的进程、文件和麦克风目标不能占用输出；启用进程仍按原卡片槽读取设备。
    #[test]
    fn inactive_cards_do_not_open_their_paired_endpoint() {
        let output_config = OutputConfig {
            process_output_ids: vec![
                vec!["paired-b".into()],
                vec!["paired-a".into(), "paired-c".into()],
            ],
            file_output_ids: vec![vec!["paired-b".into()]],
            mic_output_ids: vec!["paired-b".into()],
            ..Default::default()
        };

        let (ids, routes) = routed_outputs(&[1], &[false], &output_config, "main", false);
        assert_eq!(ids, ["paired-a", "paired-c"]);
        assert_eq!(routes.process, [vec![0, 1]]);
        assert_eq!(routes.file, [Vec::<usize>::new()]);
        assert!(routes.mic.is_empty());
    }

    /// 两张启用卡片选同一虚拟设备的两端时，两个端点都交给引擎原有冲突校验。
    #[test]
    fn active_paired_endpoints_are_both_submitted() {
        let output_config = OutputConfig {
            process_output_ids: vec![vec!["paired-a".into(), "paired-c".into()]],
            file_output_ids: vec![vec!["paired-b".into(), "paired-c".into()]],
            ..Default::default()
        };

        let (ids, routes) = routed_outputs(&[0], &[true], &output_config, "main", false);
        assert_eq!(ids, ["paired-a", "paired-c", "paired-b"]);
        assert_eq!(routes.process, [vec![0, 1]]);
        assert_eq!(routes.file, [vec![2, 1]]);
    }
}
