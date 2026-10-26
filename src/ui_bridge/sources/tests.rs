//! 验证源行并行模型、设备身份和文件上下文页码映射。
use super::{
    MicrophoneRowInput, ProcessRowInput, SourceRows, device_selection, render_output_config,
    render_target_ids,
};

/// 三类行都必须同步扩展并行数组，否则 UI 属性会错位。
#[test]
fn source_row_builders_keep_parallel_models_aligned() {
    let mut rows = SourceRows::default();
    rows.push_process(ProcessRowInput {
        name: "进程".into(),
        pid: 1,
        status: "".into(),
        icon_image: slint::Image::default(),
        active: true,
        volume: 1.0,
        peak: 0.0,
        output_selection: vec![true],
        output_label: "默认设备".into(),
    });
    rows.push_microphone(MicrophoneRowInput {
        name: "麦克风设备".into(),
        status: "".into(),
        active: false,
        volume: 1.0,
        preview_volume: 0.5,
        peak: 0.0,
        output_selection: vec![true],
        output_label: "默认设备".into(),
    });
    assert!(rows.is_aligned());
}

/// 同驱动两条可见选项独立勾选；运行图合并真实 render，不能回写覆盖选择。
#[test]
fn paired_capture_and_render_have_independent_selection() {
    use crate::audio::device::DestDevice;
    let devices = vec![
        DestDevice {
            label: "虚拟扬声器".into(),
            render_id: "render".into(),
            capture_id: None,
            capture_name: None,
            driver_family: "cable".into(),
            virtual_driver: true,
        },
        DestDevice {
            label: "虚拟麦克风".into(),
            render_id: "render".into(),
            capture_id: Some("capture".into()),
            capture_name: Some("虚拟麦克风".into()),
            driver_family: "cable".into(),
            virtual_driver: true,
        },
    ];
    let mic = vec!["capture".to_owned()];
    let (selected, label) = device_selection(&devices, &mic);
    assert_eq!(selected, [false, true]);
    assert_eq!(label, "虚拟麦克风");
    let both = vec!["render".to_owned(), "capture".to_owned()];
    assert_eq!(device_selection(&devices, &both).0, [true, true]);
    assert_eq!(render_target_ids(&devices, &both), ["render"]);
    let config = crate::config::OutputConfig {
        process_output_ids: vec![mic.clone()],
        file_output_ids: vec![both.clone()],
        mic_output_ids: mic.clone(),
        ..Default::default()
    };
    let resolved = render_output_config(&devices, &config);
    assert_eq!(resolved.process_output_ids[0], ["render"]);
    assert_eq!(resolved.file_output_ids[0], ["render"]);
    assert_eq!(resolved.mic_output_ids, ["render"]);
    assert_eq!(config.process_output_ids[0], mic);
}
