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
    // 输出 ID 优先取该源槽位的独立配置，缺失时回退到路由页当前目标设备。
    let output_id = {
        let cache_state = cache.borrow();
        let config = app_config.borrow();
        // 文件源按混合行之前的文件数量计算独立槽位；进程和麦克风使用全局目标配置。
        let configured = if kind == 1 {
            let source_index = usize::try_from(source_index).ok();
            let kinds = ui.get_source_kinds();
            source_index
                .map(|index| {
                    (0..index)
                        .filter(|row| kinds.row_data(*row) == Some(1))
                        .count()
                })
                .and_then(|slot| cache_state.audio_output_ids.get(slot))
                .and_then(Option::as_deref)
                .map(str::to_owned)
        } else {
            match kind {
                0 => config.output.process_output_id.as_deref(),
                _ => config.output.mic_output_id.as_deref(),
            }
            .map(str::to_owned)
        };
        // 没有该源的独立目标时，沿用路由页当前设备，保证单源启动仍有可用输出。
        configured.or_else(|| {
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
    // 单源启动也要解除隐藏状态，否则路由列表会把正在转发的文件/麦克风卡片隐藏。
    if kind == 1 {
        cache.borrow_mut().file_source_hidden = false;
    }
    if kind == 2 {
        cache.borrow_mut().mic_source_hidden = false;
    }
    // 单源模式先彻底停止旧路由，再用固定的唯一输出映射重建源，避免残留其他槽位。
    engine.stop();
    // 所有源的路由目标都指向输出数组的第 0 项，其他槽位保持空映射以隔离单源模式。
    let routes = crate::engine::SourceOutputRoutes {
        process: vec![0; crate::engine::PROCESS_SLOTS],
        file: vec![0; crate::engine::SFX_SLOTS],
        mic: 0,
    };
    if let Err(error) = engine.open_routed_outputs(&[output_id], routes) {
        tracing::warn!("单独转发打开输出失败：{error}");
        super::super::super::diagnostics::report_error(ui, &error);
        return;
    }
    // 文件单独转发仅准备目标和保留卡片状态，实际播放由卡片按钮显式启动。
    let result = match kind {
        0 => source_process_pid(ui, cache, source_index)
            .ok_or(Error::ProcessNotFound(0))
            .and_then(|pid| engine.add_process_source(pid, 0)),
        1 => source_file_entry(ui, cache, source_index)
            .map(|_| ())
            .ok_or_else(|| Error::InvalidArgs("没有可单独转发的音频文件".into())),
        2 => source_mic_id(ui, cache)
            .ok_or_else(|| Error::InvalidArgs("没有选择麦克风".into()))
            .and_then(|id| engine.set_mic_source(Some(&id))),
        _ => Err(Error::InvalidArgs("未知音频源".into())),
    };
    // 任何源安装失败都回滚整个引擎，避免 UI 显示运行中但没有实际音频。
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

/// 将混合源列表中的 UI 行转换为可用进程槽位，再解析对应 PID。
fn source_process_pid(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>, index: i32) -> Option<u32> {
    let index = usize::try_from(index).ok()?;
    // 混合源行先折算为此前 kind=0 行数，才能与进程槽和独立增益数组对齐。
    let kinds = ui.get_source_kinds();
    let slot = (0..index)
        .filter(|row| kinds.row_data(*row) == Some(0))
        .count();
    let cache = cache.borrow();
    // 进程槽位按 selected_pids 顺序解析，但退出或禁用的 PID 必须先过滤掉。
    cache
        .selected_pids
        .iter()
        .filter(|pid| cache.pids.contains(pid) && !cache.disabled_pids.contains(pid))
        .nth(slot)
        .copied()
}

/// 读取当前 UI 麦克风索引对应的设备 ID；空字符串表示没有有效选择。
fn source_mic_id(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) -> Option<String> {
    // mic_index 只表示 UI 设备列表位置，真正送入引擎的是缓存中的不透明设备 ID。
    cache
        .borrow()
        .mic_ids
        .get(ui.get_mic_index().max(0) as usize)
        .filter(|id| !id.is_empty())
        .cloned()
}

/// 按混合源行映射到路由文件槽，避免单独转发误用音频库当前选择。
fn source_file_entry(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    source_index: i32,
) -> Option<config::AudioEntry> {
    let source_index = usize::try_from(source_index).ok()?;
    let kinds = ui.get_source_kinds();
    // 文件槽位只计数当前行之前的文件源，进程与麦克风行不改变文件下标。
    let file_slot = (0..source_index)
        .filter(|row| kinds.row_data(*row) == Some(1))
        .count();
    let cache = cache.borrow();
    let entry_index = if cache.route_audio_entries.is_empty() {
        (file_slot == 0)
            .then_some(cache.route_audio_entry)
            .flatten()
    } else {
        cache.route_audio_entries.get(file_slot).copied()
    }?;
    cache.audio_entries.get(entry_index).cloned()
}
