//! 在可见端点选择 ID 与路由运行时 render ID 之间转换，保留虚拟麦克风独立勾选身份。
/// 列表项以独立端点 ID 保存；创建 WASAPI render 图时再解析到真实输出端点。
pub(in crate::ui_bridge) fn render_target_ids(
    devices: &[crate::audio::device::DestDevice],
    ids: &[String],
) -> Vec<String> {
    let mut resolved = Vec::with_capacity(ids.len());
    for id in ids {
        let render_id = devices
            .iter()
            .find(|device| device.selection_id() == id)
            .map(|device| device.render_id.as_str())
            .unwrap_or(id);
        if !resolved.iter().any(|existing| existing == render_id) {
            resolved.push(render_id.to_owned());
        }
    }
    resolved
}

/// 启动快照只使用 render ID，配置本身保留虚拟麦与扬声器各自的选择身份。
pub(in crate::ui_bridge) fn render_output_config(
    devices: &[crate::audio::device::DestDevice],
    output: &crate::config::OutputConfig,
) -> crate::config::OutputConfig {
    let mut resolved = output.clone();
    resolved.device_ids = render_target_ids(devices, &output.device_ids);
    resolved.process_output_ids = output
        .process_output_ids
        .iter()
        .map(|ids| render_target_ids(devices, ids))
        .collect();
    resolved.file_output_ids = output
        .file_output_ids
        .iter()
        .map(|ids| render_target_ids(devices, ids))
        .collect();
    resolved.mic_output_ids = render_target_ids(devices, &output.mic_output_ids);
    resolved
}

/// 从稳定 ID 投影出当前设备列表的勾选状态；未知 ID 不会误选其他设备。
pub(in crate::ui_bridge) fn device_selection(
    devices: &[crate::audio::device::DestDevice],
    ids: &[String],
) -> (Vec<bool>, String) {
    let selected = devices
        .iter()
        .map(|device| ids.iter().any(|id| id == device.selection_id()))
        .collect::<Vec<_>>();
    let label = devices
        .iter()
        .zip(&selected)
        .filter_map(|(device, checked)| checked.then_some(device.label.as_str()))
        .collect::<Vec<_>>()
        .join("、");
    (selected, label)
}
