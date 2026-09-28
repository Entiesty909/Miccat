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
    fn deleting_all_removes_library_entries_and_only_its_node() {
        let mut categories = vec!["全部".into(), "未分类".into(), "游戏".into()];
        let mut entries = vec![entry("a", "未分类"), entry("b", "游戏")];

        delete_category(&mut categories, &mut entries, "全部");

        assert_eq!(categories, vec!["未分类", "游戏"]);
        assert!(entries.is_empty());
    }

    #[test]
    fn deleting_uncategorized_removes_only_its_entries_and_descendants() {
        let mut categories = vec![
            "全部".into(),
            "未分类".into(),
            "未分类/临时".into(),
            "游戏".into(),
        ];
        let mut entries = vec![
            entry("a", "未分类"),
            entry("b", "未分类/临时"),
            entry("c", "游戏"),
        ];

        delete_category(&mut categories, &mut entries, "未分类");

        assert_eq!(categories, vec!["全部", "游戏"]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "c");
    }

    #[test]
    fn category_paths_do_not_restore_deleted_empty_roots() {
        assert!(category_paths(&[], &[]).is_empty());
        assert_eq!(category_paths(&["游戏".into()], &[]), vec!["游戏"]);
    }

    #[test]
    fn deleting_regular_category_moves_entries_to_uncategorized() {
        let mut categories = vec!["游戏".into(), "游戏/武器".into(), "其他".into()];
        let mut entries = vec![entry("a", "游戏"), entry("b", "游戏/武器")];

        delete_category(&mut categories, &mut entries, "游戏");

        assert_eq!(categories, vec!["其他"]);
        assert!(entries.iter().all(|entry| entry.category == "未分类"));
    }
    #[test]
    fn import_uses_selected_category_or_uncategorized_fallback() {
        assert_eq!(import_category("游戏/武器"), "游戏/武器");
        assert_eq!(import_category(""), "未分类");
    }
    #[test]
    fn new_category_from_empty_tree_is_a_root() {
        assert_eq!(new_category(&[], &[], ""), "未命名分类");
    }
    #[test]
    fn new_categories_preserve_parent_child_and_sibling_paths() {
        let mut categories = vec!["游戏".into(), "游戏/枪械".into()];
        assert_eq!(new_category(&categories, &[], ""), "未命名分类");
        assert_eq!(new_category(&categories, &[], "游戏"), "游戏/未命名分类");
        assert_eq!(
            new_category(&categories, &[], "游戏/枪械"),
            "游戏/枪械/未命名分类"
        );

        categories.push("游戏/未命名分类".into());
        assert_eq!(new_category(&categories, &[], "游戏"), "游戏/未命名分类 2");
    }
    #[test]
    fn category_paths_keep_children_under_their_parent() {
        let categories = vec![
            "分类1".into(),
            "分类2".into(),
            "分类3".into(),
            "分类1/子分类".into(),
        ];

        assert_eq!(
            category_paths(&categories, &[]),
            vec!["分类1", "分类1/子分类", "分类2", "分类3"]
        );
    }
    #[test]
    fn moving_category_into_target_moves_descendants_and_audio_entries() {
        let mut categories = vec!["分类1".into(), "分类2".into(), "分类3".into()];
        let mut entries = vec![entry("音效", "分类1/子项"), entry("目标", "分类2")];

        let destination = move_category(
            &mut categories,
            &mut entries,
            "分类1",
            "分类2",
            CategoryDropPosition::Into,
        );

        assert_eq!(destination.as_deref(), Some("分类2/分类1"));
        assert_eq!(
            category_paths(&categories, &entries),
            vec!["分类2", "分类2/分类1", "分类2/分类1/子项", "分类3"]
        );
        assert_eq!(entries[0].category, "分类2/分类1/子项");
        assert_eq!(entries[1].category, "分类2");
    }

    #[test]
    fn moving_category_before_sibling_reorders_tree_without_losing_parent() {
        let mut categories = vec![
            "父类".into(),
            "父类/第一项".into(),
            "父类/第二项".into(),
            "其他".into(),
        ];

        let destination = move_category(
            &mut categories,
            &mut [],
            "其他",
            "父类/第二项",
            CategoryDropPosition::Before,
        );

        assert_eq!(destination.as_deref(), Some("父类/其他"));
        assert_eq!(
            category_paths(&categories, &[]),
            vec!["父类", "父类/第一项", "父类/其他", "父类/第二项"]
        );
    }

    #[test]
    fn moving_category_rejects_descendant_cycles_and_name_collisions() {
        let mut categories = vec!["甲".into(), "甲/子".into(), "乙".into(), "乙/甲".into()];
        let original = categories.clone();

        assert!(!can_move_category(
            &categories,
            &[],
            "甲",
            "甲/子",
            CategoryDropPosition::Into,
        ));
        assert!(
            move_category(
                &mut categories,
                &mut [],
                "甲",
                "乙",
                CategoryDropPosition::Into,
            )
            .is_none()
        );
        assert_eq!(categories, original);
    }
    #[test]
    fn moving_category_after_sibling_places_it_after_sibling_subtree() {
        let mut categories = vec![
            "父类".into(),
            "父类/第一项".into(),
            "父类/第二项".into(),
            "父类/第二项/后代".into(),
            "父类/第三项".into(),
        ];

        assert_eq!(
            move_category(
                &mut categories,
                &mut [],
                "父类/第一项",
                "父类/第二项",
                CategoryDropPosition::After,
            )
            .as_deref(),
            Some("父类/第一项")
        );

        assert_eq!(
            category_paths(&categories, &[]),
            vec![
                "父类",
                "父类/第二项",
                "父类/第二项/后代",
                "父类/第一项",
                "父类/第三项",
            ]
        );
    }
    #[test]
    fn moved_category_can_be_deleted_at_its_new_path() {
        let mut categories = vec!["分类1".into(), "分类2".into(), "分类3".into()];
        let mut entries = vec![entry("子类音效", "分类1/子类")];
        let moved = move_category(
            &mut categories,
            &mut entries,
            "分类1",
            "分类2",
            CategoryDropPosition::Into,
        )
        .expect("valid drag target");

        delete_category(&mut categories, &mut entries, &moved);

        assert!(!category_paths(&categories, &entries).contains(&moved));
        assert_eq!(entries[0].category, "未分类");
        assert!(categories.contains(&"分类2".into()));
        assert!(categories.contains(&"分类3".into()));
    }

    #[test]
    fn renaming_category_updates_descendants_and_audio_paths() {
        let mut categories = vec!["旧根".into(), "旧根/子项".into(), "其他".into()];
        let mut entries = vec![entry("音效", "旧根/子项")];

        rename_category(&mut categories, &mut entries, "旧根", "新根");

        assert_eq!(categories, vec!["新根", "新根/子项", "其他"]);
        assert_eq!(entries[0].category, "新根/子项");
    }
}
