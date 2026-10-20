//! 验证路由页进程行到当前进程选择的索引映射。

use super::{duplicate_audio, duplicate_process, process_source_slot, visible_process_pid};

/// 验证同类源行下标能映射到对应的进程槽位。
#[test]
fn process_rows_map_to_their_selected_pid_slots() {
    let kinds = [0, 0, 1, 2, 0];

    assert_eq!(process_source_slot(&kinds, 0), Some(0));
    assert_eq!(process_source_slot(&kinds, 1), Some(1));
    assert_eq!(process_source_slot(&kinds, 2), None);
    assert_eq!(process_source_slot(&kinds, 3), None);
    assert_eq!(process_source_slot(&kinds, 4), Some(2));
    assert_eq!(process_source_slot(&kinds, 5), None);
}

#[test]
/// 已退出的 PID 被跳过，剩余可用 PID 仍按可见槽位映射。
fn process_row_skips_stale_selected_process_ids() {
    let selected = [10, 20, 30];
    let available = [20, 30];

    assert_eq!(visible_process_pid(&selected, &available, 0), Some(20));
    assert_eq!(visible_process_pid(&selected, &available, 1), Some(30));
    assert_eq!(visible_process_pid(&selected, &available, 2), None);
}

#[test]
/// 添加进程时拒绝重复 PID，替换自身槽位除外。
fn duplicate_process_is_rejected_except_for_the_replaced_slot() {
    assert!(duplicate_process(&[10, 20], 10, None));
    assert!(duplicate_process(&[10, 20], 10, Some(1)));
    assert!(!duplicate_process(&[10, 20], 10, Some(0)));
}

#[test]
/// 添加音频时拒绝重复条目，替换自身槽位除外。
fn duplicate_audio_is_rejected_except_for_the_replaced_slot() {
    assert!(duplicate_audio(&[3, 8], 3, None));
    assert!(duplicate_audio(&[3, 8], 3, Some(1)));
    assert!(!duplicate_audio(&[3, 8], 3, Some(0)));
}
