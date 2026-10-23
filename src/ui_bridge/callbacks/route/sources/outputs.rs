//! 处理路由卡片多目标勾选与新文件源的默认输出选择。

use super::*;

/// 只修改当前源槽的设备 ID 集合；至少保留一只输出以维持旧单选行为。
fn toggle_output_selection(
    output: &mut config::OutputConfig,
    kind: usize,
    slot: usize,
    target_id: &str,
    checked: bool,
    fallback: &[String],
) -> bool {
    let saved = match kind {
        0 => output.process_output_ids.get(slot),
        1 => output.file_output_ids.get(slot),
        _ => None,
    };
    let current = if kind == 2 && !output.mic_output_ids.is_empty() {
        output.mic_output_ids.as_slice()
    } else {
        saved.map(Vec::as_slice).unwrap_or(fallback)
    };
    let mut updated = current.to_vec();
    if checked {
        if updated.iter().any(|id| id == target_id) {
            return false;
        }
        updated.push(target_id.to_owned());
    } else {
        if updated.len() <= 1 || !updated.iter().any(|id| id == target_id) {
            return false;
        }
        updated.retain(|id| id != target_id);
    }
    match kind {
        0 => {
            if output.process_output_ids.len() <= slot {
                output
                    .process_output_ids
                    .resize(slot + 1, fallback.to_vec());
            }
            output.process_output_ids[slot] = updated;
        }
        1 => {
            if output.file_output_ids.len() <= slot {
                output.file_output_ids.resize(slot + 1, fallback.to_vec());
            }
            output.file_output_ids[slot] = updated;
        }
        _ => output.mic_output_ids = updated,
    }
    true
}

/// 绑定路由源目标多选与新文件源默认设备选择。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    {
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        // 设备下标仅用于枚举命中；按不透明 ID 修改该卡片的目标集合。
        let state_for_update = state.clone();
        ui.on_source_output_toggled(move |row, target_index, checked| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if ui.get_running() {
                return;
            }
            let Some(kind) = source_kind(&ui, row) else {
                return;
            };
            let Some(slot) = source_slot(&ui, row, kind) else {
                return;
            };
            let target_id = cache
                .borrow()
                .dests
                .get(target_index as usize)
                .map(|device| device.selection_id().to_owned());
            let Some(target_id) = target_id else { return };
            let fallback = {
                let config = app_config.borrow();
                if config.output.device_ids.is_empty() {
                    cache
                        .borrow()
                        .dests
                        .get(ui.get_device_index().max(0) as usize)
                        .map(|device| vec![device.selection_id().to_owned()])
                        .unwrap_or_default()
                } else {
                    config.output.device_ids.clone()
                }
            };
            let changed = toggle_output_selection(
                &mut app_config.borrow_mut().output,
                kind,
                slot,
                &target_id,
                checked,
                &fallback,
            );
            if changed {
                save_queue.schedule();
                crate::ui_bridge::sources::update(&ui, &state_for_update);
            }
        });
    }
    {
        let cache = state.cache.clone();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        ui.on_file_output_changed(move |target_index| {
            let target_id = cache
                .borrow()
                .dests
                .get(target_index as usize)
                .map(|device| device.selection_id().to_owned());
            let Some(target_id) = target_id else { return };
            // 播放页的默认设备只作用于新建文件卡片，不能覆盖首张卡片自己的目标列表。
            app_config.borrow_mut().output.file_default_output_id = Some(target_id);
            save_queue.schedule();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::toggle_output_selection;
    use crate::config::OutputConfig;

    /// 两张卡片与播放页默认设备互不覆盖，且不能取消最后一只目标。
    #[test]
    fn multiple_targets_stay_with_their_source_slot() {
        let mut output = OutputConfig {
            file_default_output_id: Some("player".into()),
            ..Default::default()
        };
        let fallback = vec!["main".into()];
        assert!(toggle_output_selection(
            &mut output,
            0,
            1,
            "second",
            true,
            &fallback
        ));
        assert!(toggle_output_selection(
            &mut output,
            1,
            0,
            "file",
            true,
            &fallback
        ));
        assert_eq!(output.process_output_ids[1], ["main", "second"]);
        assert_eq!(output.file_output_ids[0], ["main", "file"]);
        assert_eq!(output.file_default_output_id.as_deref(), Some("player"));
        assert!(toggle_output_selection(
            &mut output,
            0,
            1,
            "main",
            false,
            &fallback
        ));
        assert!(!toggle_output_selection(
            &mut output,
            0,
            1,
            "second",
            false,
            &fallback
        ));
        assert_eq!(output.process_output_ids[1], ["second"]);
        assert_eq!(output.file_output_ids[0], ["main", "file"]);
    }
    /// 同一驱动的两个列表项指向同一 render，但只改变被点击的端点身份。
    #[test]
    fn paired_endpoints_toggle_independently() {
        let mut output = OutputConfig::default();
        let initial = ["render".to_owned()];
        assert!(toggle_output_selection(
            &mut output,
            0,
            0,
            "capture",
            true,
            &initial
        ));
        assert_eq!(output.process_output_ids[0], ["render", "capture"]);
        assert!(toggle_output_selection(
            &mut output,
            0,
            0,
            "render",
            false,
            &initial
        ));
        assert_eq!(output.process_output_ids[0], ["capture"]);
    }
}
