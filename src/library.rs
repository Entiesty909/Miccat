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

/// 按分类返回配置条目下标。
pub(crate) fn visible_entry_index(
    entries: &[AudioEntry],
    category: &str,
    visible_index: usize,
) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| category_matches(entry, category))
        .nth(visible_index)
        .map(|(index, _)| index)
}

/// 展开分类路径，并按父节点分组保留同级插入顺序。
pub(crate) fn category_paths(categories: &[String], entries: &[AudioEntry]) -> Vec<String> {
    let mut paths = Vec::new();
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

    let mut ordered = Vec::with_capacity(paths.len());
    let mut pending = Vec::with_capacity(paths.len());
    for index in (0..paths.len()).rev() {
        if !paths[index].contains('/') {
            pending.push(index);
        }
    }
    while let Some(index) = pending.pop() {
        let parent = &paths[index];
        ordered.push(parent.clone());
        let prefix = format!("{parent}/");
        for child in (0..paths.len()).rev() {
            let Some(relative) = paths[child].strip_prefix(&prefix) else {
                continue;
            };
            if !relative.is_empty() && !relative.contains('/') {
                pending.push(child);
            }
        }
    }
    ordered
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

/// 返回新导入条目的分类；没有选择时归入“未分类”。
pub(crate) fn import_category(selected: &str) -> String {
    if selected.is_empty() {
        "未分类".into()
    } else {
        selected.into()
    }
}

/// 在所选分类下创建不冲突的子分类；空选择创建顶层分类。
pub(crate) fn new_category(
    categories: &[String],
    entries: &[AudioEntry],
    selected: &str,
) -> String {
    let paths = category_paths(categories, entries);
    let base =
        (!selected.is_empty() && selected != "全部" && selected != "未分类").then_some(selected);
    let prefix = base.map(|path| format!("{path}/"));
    let name = "未命名分类";
    let first = format!("{}{name}", prefix.as_deref().unwrap_or(""));
    if !paths.contains(&first) {
        return first;
    }
    let mut number = 2;
    loop {
        let candidate = format!("{}{name} {number}", prefix.as_deref().unwrap_or(""));
        if !paths.contains(&candidate) {
            return candidate;
        }
        number += 1;
    }
}

/// 分类拖放到目标行时的层级位置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CategoryDropPosition {
    /// 插入为目标的子分类。
    Into,
    /// 插入为目标同级的前一项。
    Before,
    /// 插入为目标同级的后一项。
    After,
}

/// 判断分类路径能否移动到目标位置。
pub(crate) fn can_move_category(
    categories: &[String],
    entries: &[AudioEntry],
    source: &str,
    target: &str,
    position: CategoryDropPosition,
) -> bool {
    category_move_destination(categories, entries, source, target, position).is_some()
}

/// 移动分类及其后代，并同步条目路径与树顺序。
pub(crate) fn move_category(
    categories: &mut Vec<String>,
    entries: &mut [AudioEntry],
    source: &str,
    target: &str,
    position: CategoryDropPosition,
) -> Option<String> {
    let destination = category_move_destination(categories, entries, source, target, position)?;
    let paths = category_paths(categories, entries);
    let source_prefix = format!("{source}/");
    let moving = paths
        .iter()
        .filter(|path| *path == source || path.starts_with(&source_prefix))
        .map(|path| format!("{destination}{}", &path[source.len()..]))
        .collect::<Vec<_>>();
    let mut remaining = paths
        .into_iter()
        .filter(|path| *path != source && !path.starts_with(&source_prefix))
        .collect::<Vec<_>>();
    let target_prefix = format!("{target}/");
    let insertion = match position {
        CategoryDropPosition::Into => {
            remaining
                .iter()
                .rposition(|path| path == target || path.starts_with(&target_prefix))?
                + 1
        }
        CategoryDropPosition::Before => remaining.iter().position(|path| path == target)?,
        CategoryDropPosition::After => {
            let mut index = remaining.iter().position(|path| path == target)? + 1;
            while index < remaining.len() && remaining[index].starts_with(&target_prefix) {
                index += 1;
            }
            index
        }
    };
    remaining.splice(insertion..insertion, moving);
    for entry in entries {
        if entry.category == source || entry.category.starts_with(&source_prefix) {
            entry.category = format!("{destination}{}", &entry.category[source.len()..]);
        }
    }
    *categories = remaining;
    Some(destination)
}

fn category_move_destination(
    categories: &[String],
    entries: &[AudioEntry],
    source: &str,
    target: &str,
    position: CategoryDropPosition,
) -> Option<String> {
    let paths = category_paths(categories, entries);
    if !paths.iter().any(|path| path == source)
        || !paths.iter().any(|path| path == target)
        || source == target
        || source == "全部"
        || source == "未分类"
        || target == "全部"
        || target == "未分类"
    {
        return None;
    }

    let source_prefix = format!("{source}/");
    if target.starts_with(&source_prefix) {
        return None;
    }
    let target_parent = match position {
        CategoryDropPosition::Into => target,
        CategoryDropPosition::Before | CategoryDropPosition::After => {
            target.rsplit_once('/').map_or("", |(parent, _)| parent)
        }
    };
    if target_parent == source || target_parent.starts_with(&source_prefix) {
        return None;
    }

    let destination = if target_parent.is_empty() {
        category_leaf(source).to_string()
    } else {
        format!("{target_parent}/{}", category_leaf(source))
    };
    if destination != source && paths.iter().any(|path| path == &destination) {
        return None;
    }
    Some(destination)
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

/// 删除分类及其后代；根节点删除相应音频库条目，但不删除磁盘文件。
pub(crate) fn delete_category(
    categories: &mut Vec<String>,
    entries: &mut Vec<AudioEntry>,
    name: &str,
) {
    let prefix = format!("{name}/");
    categories.retain(|category| category != name && !category.starts_with(&prefix));
    match name {
        "全部" => entries.clear(),
        "未分类" => {
            entries.retain(|entry| entry.category != name && !entry.category.starts_with(&prefix))
        }
        _ => {
            for entry in entries {
                if entry.category == name || entry.category.starts_with(&prefix) {
                    entry.category = "未分类".into();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
