//! 格式化音频库时长与进度文案。
//! 本模块只做无状态的字符串投影，不修改播放位置、缓存或配置。

/// 将 100 纳秒为单位的 Windows 时间值格式化为 `分:秒` 或 `时:分:秒`；零表示未知。
pub(super) fn format_duration_hns(duration_hns: u64) -> String {
    // 库条目可能没有时长元数据；用占位符而不是“0:00”避免把未知误报成零时长。
    if duration_hns == 0 {
        return "—".into();
    }
    // HNS 是 10^-7 秒，先取整秒再拆分，避免在界面上显示不稳定的小数。
    let total_seconds = duration_hns / 10_000_000;
    // 分别取出小时、余下分钟和余下秒，保证无小时时仍显示紧凑的 `分:秒`。
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        // 只有存在小时才加第三段，避免短音频出现多余的 `0:` 前缀。
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// 播放位置文案：「当前 / 总时长」；任一输入为零时沿用未知时长占位符。
pub(super) fn format_hns_position(position_hns: u64, duration_hns: u64) -> String {
    // 复用同一单位转换和未知值规则，避免进度与库列表显示出现两套格式。
    format!(
        "{} / {}",
        format_duration_hns(position_hns),
        format_duration_hns(duration_hns)
    )
}
