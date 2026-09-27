//! 音频库领域规则：分类路径、可见索引与递归分类变更。

use std::path::Path;

use crate::audio::decode::MediaInfo;
use crate::config::AudioEntry;

/// 验证音频文件并读取媒体时长与格式信息。
pub(crate) fn validate_file(path: &Path) -> Result<MediaInfo, crate::error::Error> {
    crate::audio::decode::validate_file(path)
}
/// 判断条目是否属于分类及其所有后代分类。
pub(crate) fn category_matches(entry: &AudioEntry, category: &str) -> bool {
    category == "全部"
        || entry.category == category
        || entry.category.starts_with(&format!("{category}/"))
}

/// 按分类与搜索词返回配置条目下标。
pub(crate) fn visible_entry_index(
    entries: &[AudioEntry],
    category: &str,
    search: &str,
    visible_index: usize,
) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            category_matches(entry, category)
                && (search.is_empty() || entry.name.to_ascii_lowercase().contains(search))
        })
        .nth(visible_index)
        .map(|(index, _)| index)
}

/// 展开配置中的分类路径，并保留固定根项。
pub(crate) fn category_paths(categories: &[String], entries: &[AudioEntry]) -> Vec<String> {
    let mut paths = vec!["全部".to_string(), "未分类".to_string()];
    let raw = categories
        .iter()
        .cloned()
        .chain(entries.iter().map(|entry| entry.category.clone()));
    for path in raw {
        let mut parent = String::new();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(part);
            if !paths.contains(&parent) {
                paths.push(parent.clone());
            }
        }
    }
    paths
}

/// 返回路径最后一级名称。
pub(crate) fn category_leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// 将分类路径转成带层级缩进的显示文本。
pub(crate) fn category_display(path: &str) -> String {
    if path == "全部" || path == "未分类" {
        return path.to_string();
    }
    let depth = path.matches('/').count() + 1;
    format!(
        "{}{}",
        "　　".repeat(depth.saturating_sub(1)),
        category_leaf(path)
    )
}

/// 将分类路径转换为 Slint 显示字符串。
pub(crate) fn category_labels(categories: &[String], entries: &[AudioEntry]) -> Vec<String> {
    category_paths(categories, entries)
        .iter()
        .map(|path| category_display(path))
        .collect()
}

/// 创建不冲突的新子分类路径。
pub(crate) fn new_category(
    categories: &[String],
    entries: &[AudioEntry],
    selected: &str,
) -> String {
    let paths = category_paths(categories, entries);
    let base = (selected != "全部" && selected != "未分类").then_some(selected);
    let prefix = base.map(|path| format!("{path}/"));
    let mut number = 1;
    loop {
        let candidate = format!("{}新分类 {number}", prefix.as_deref().unwrap_or(""));
        if !paths.contains(&candidate) {
            return candidate;
        }
        number += 1;
    }
}

/// 重命名分类及其所有后代，并同步条目分类路径。
pub(crate) fn rename_category(
    categories: &mut [String],
    entries: &mut [AudioEntry],
    old: &str,
    new_path: &str,
) {
    let prefix = format!("{old}/");
    for category in categories {
        if *category == old || category.starts_with(&prefix) {
            *category = format!("{new_path}{}", &category[old.len()..]);
        }
    }
    for entry in entries {
        if entry.category == old || entry.category.starts_with(&prefix) {
            entry.category = format!("{new_path}{}", &entry.category[old.len()..]);
        }
    }
}

/// 删除分类及其所有后代，并将其中条目移到未分类。
pub(crate) fn delete_category(
    categories: &mut Vec<String>,
    entries: &mut [AudioEntry],
    name: &str,
) {
    let prefix = format!("{name}/");
    categories.retain(|category| category != name && !category.starts_with(&prefix));
    for entry in entries {
        if entry.category == name || entry.category.starts_with(&prefix) {
            entry.category = "未分类".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, category: &str) -> AudioEntry {
        AudioEntry {
            name: name.into(),
            path: format!("{name}.wav"),
            category: category.into(),
            duration_hns: 0,
            loop_playback: false,
        }
    }

    #[test]
    fn visible_index_maps_filtered_descendants() {
        let entries = vec![
            entry("Railgun", "其他"),
            entry("Railgun", "游戏/武器"),
            entry("Rocket", "游戏/武器"),
            entry("Railgun special", "游戏/武器/特殊"),
        ];

        assert_eq!(visible_entry_index(&entries, "游戏", "rail", 0), Some(1));
        assert_eq!(visible_entry_index(&entries, "游戏", "rail", 1), Some(3));
        assert_eq!(visible_entry_index(&entries, "游戏", "rail", 2), None);
    }

    #[test]
    fn delete_category_moves_descendants_to_uncategorized() {
        let mut categories = vec!["游戏".into(), "游戏/武器".into(), "其他".into()];
        let mut entries = vec![entry("a", "游戏"), entry("b", "游戏/武器")];

        delete_category(&mut categories, &mut entries, "游戏");

        assert_eq!(categories, vec!["其他"]);
        assert!(entries.iter().all(|entry| entry.category == "未分类"));
    }
}
