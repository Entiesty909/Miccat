//! 格式化音频库时长与进度文案。

pub(super) fn format_duration_hns(duration_hns: u64) -> String {
    if duration_hns == 0 {
        return "—".into();
    }
    let total_seconds = duration_hns / 10_000_000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// 播放位置文案：「当前 / 总时长」。
pub(super) fn format_hns_position(position_hns: u64, duration_hns: u64) -> String {
    format!(
        "{} / {}",
        format_duration_hns(position_hns),
        format_duration_hns(duration_hns)
    )
}
