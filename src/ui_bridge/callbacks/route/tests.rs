//! 验证路由页进程行到当前进程选择的索引映射。

use super::{process_source_slot, visible_process_pid};

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
fn process_row_skips_stale_selected_process_ids() {
    let selected = [10, 20, 30];
    let available = [20, 30];

    assert_eq!(visible_process_pid(&selected, &available, 0), Some(20));
    assert_eq!(visible_process_pid(&selected, &available, 1), Some(30));
    assert_eq!(visible_process_pid(&selected, &available, 2), None);
}
