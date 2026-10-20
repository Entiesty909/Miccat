//! 音频库领域规则：分类路径、递归索引、图标映射与分类变更。

use std::collections::BTreeMap;
use std::path::Path;

use crate::audio::decode::MediaInfo;
use crate::config::AudioEntry;
// 分类路径使用 `/` 作为层级分隔符；条目的 category 是实际归属的权威值，分类列表只保存显式节点。

/// 验证音频文件并读取媒体时长与格式信息。
pub(crate) fn validate_file(path: &Path) -> Result<MediaInfo, crate::error::Error> {
    crate::audio::decode::validate_file(path)
}
/// 判断条目是否属于分类及其所有后代分类。
pub(crate) fn category_matches(entry: &AudioEntry, category: &str) -> bool {
    // “全部”是虚拟根节点；其余分类必须在路径边界上匹配，避免误收同名前缀。
    category == "全部"
        || entry.category == category
        || entry.category.starts_with(&format!("{category}/"))
}

/// 重命名分类路径时同步迁移其图标和全部后代图标。
pub(crate) fn rename_category_icon_paths(
    icons: &mut BTreeMap<String, String>,
    old_path: &str,
    new_path: &str,
) {
    // 先取走旧表再重建，既能原地更新所有键，也不会在遍历时修改同一张 map。
    let old_prefix = format!("{old_path}/");
    let old_icons = std::mem::take(icons);
    for (path, glyph) in old_icons {
        let next_path = if path == old_path {
            new_path.to_owned()
        } else if let Some(suffix) = path.strip_prefix(&old_prefix) {
            // 保留后代相对路径，只替换根分类部分。
            format!("{new_path}/{suffix}")
        } else {
            // 不属于被重命名子树的图标键必须原样保留。
            path
        };
        icons.insert(next_path, glyph);
    }
}

/// 删除分类时移除它及全部后代对应的图标设置。
pub(crate) fn delete_category_icon_paths(icons: &mut BTreeMap<String, String>, path: &str) {
    // 同样以斜杠限定子树边界，避免删除同名前缀的平级分类图标。
    let prefix = format!("{path}/");
    icons.retain(|candidate, _| candidate != path && !candidate.starts_with(&prefix));
}

/// 按分类返回配置条目下标。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn visible_entry_index(
    entries: &[AudioEntry],
    category: &str,
    visible_index: usize,
) -> Option<usize> {
    // 先按分类生成可见序列，再用原数组下标返回，保证 UI 行号不会误当配置下标。
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| category_matches(entry, category))
        .nth(visible_index)
        .map(|(index, _)| index)
}

/// 展开分类路径，并按父节点分组保留同级插入顺序。
pub(crate) fn category_paths(categories: &[String], entries: &[AudioEntry]) -> Vec<String> {
    // 分类列表保存显式节点，条目可能携带未写入列表的深层路径；两者都要参与补全。
    let mut paths = Vec::new();
    let raw = categories
        .iter()
        .cloned()
        .chain(entries.iter().map(|entry| entry.category.clone()));
    for path in raw {
        // 逐段累积父路径，既补齐隐含父节点，也去重保留首次出现顺序。
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

    // 收集结果按首次见到的顺序排列；下面改成父节点优先的前序序列，供树形 UI 和拖放索引共用。

    // 栈按逆序压入同级节点，出栈后即可保持原有同级顺序并先处理父节点。
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
    // 两个虚拟根节点不参与缩进，否则它们会被误显示成普通分类。
    if path == "全部" || path == "未分类" {
        return path.to_string();
    }
    // 层级由路径分隔符决定，只缩进叶名称，避免 UI 重复显示完整路径。
    let depth = path.matches('/').count() + 1;
    format!(
        "{}{}",
        "　　".repeat(depth.saturating_sub(1)),
        category_leaf(path)
    )
}

/// 将分类路径转换为 Slint 显示字符串。
pub(crate) fn category_labels(categories: &[String], entries: &[AudioEntry]) -> Vec<String> {
    // 标签与路径使用同一份前序结果，保证显示行号和索引映射保持一致。
    category_paths(categories, entries)
        .iter()
        .map(|path| category_display(path))
        .collect()
}

/// 返回新导入条目的分类；没有选择时归入“未分类”。
pub(crate) fn import_category(selected: &str) -> String {
    // 空选择表示调用方没有指定分类，统一落到可持久化的兜底节点。
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
    // “全部”和“未分类”是虚拟入口，不能作为新分类的父节点。
    let paths = category_paths(categories, entries);
    let base =
        (!selected.is_empty() && selected != "全部" && selected != "未分类").then_some(selected);
    let prefix = base.map(|path| format!("{path}/"));
    let name = "未命名分类";
    let first = format!("{}{name}", prefix.as_deref().unwrap_or(""));
    if !paths.contains(&first) {
        return first;
    }
    // 首选默认名称；冲突时递增后缀，直到整个路径在当前树中唯一。
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
    // 预览和拖放提交共用同一纯计算函数，避免可移动判断与实际落点分叉。
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
    // 目标计算先完成全部合法性检查，失败时保持分类和条目原状。
    let destination = category_move_destination(categories, entries, source, target, position)?;
    let paths = category_paths(categories, entries);
    let source_prefix = format!("{source}/");
    // 源节点与后代使用同一相对后缀改写，确保树节点和条目路径同步迁移。
    let moving = paths
        .iter()
        .filter(|path| *path == source || path.starts_with(&source_prefix))
        .map(|path| format!("{destination}{}", &path[source.len()..]))
        .collect::<Vec<_>>();
    // 先从旧序列剔除源子树，再按目标位置找到不破坏子树边界的插入槽位。

    // 删除源子树后再计算插入点，避免源节点自身影响目标的相对位置。
    let mut remaining = paths
        .into_iter()
        .filter(|path| *path != source && !path.starts_with(&source_prefix))
        .collect::<Vec<_>>();
    let target_prefix = format!("{target}/");
    // Into 放到目标子树末尾；Before/After 则以目标兄弟为基准。
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
    // 条目路径与分类树必须同步更新，否则过滤列表会出现分类节点和音频不一致。
    remaining.splice(insertion..insertion, moving);
    for entry in entries {
        if entry.category == source || entry.category.starts_with(&source_prefix) {
            entry.category = format!("{destination}{}", &entry.category[source.len()..]);
        }
    }
    *categories = remaining;
    Some(destination)
}

/// 计算分类移动后的完整路径；非法目标、循环和重名都会返回 `None`。
/// 该函数只做纯校验和路径推导，不修改输入，供预览和实际移动共同调用。
fn category_move_destination(
    categories: &[String],
    entries: &[AudioEntry],
    source: &str,
    target: &str,
    position: CategoryDropPosition,
) -> Option<String> {
    // 先确认两端都是实际节点，并拒绝虚拟根、自己移动到自己等无意义操作。
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

    // 目标落在源子树内会形成循环；同级移动还需检查目标父节点是否属于源子树。
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

    // 目的路径沿用源叶名称，只替换父路径；重名检查保证不会覆盖既有节点。
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
    // 斜杠边界把整个后代子树作为迁移单位，避免误改同名前缀的平级分类。
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
    // 先移除分类树节点，再按虚拟根的不同语义处理条目；磁盘文件不在此删除。
    let prefix = format!("{name}/");
    categories.retain(|category| category != name && !category.starts_with(&prefix));
    // 删除“全部”代表清空库，删除“未分类”才只清理该子树中的条目。
    match name {
        "全部" => entries.clear(),
        "未分类" => {
            entries.retain(|entry| entry.category != name && !entry.category.starts_with(&prefix))
        }
        _ => {
            // 普通分类删除后保留音频，因此把受影响条目回收到未分类。
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
