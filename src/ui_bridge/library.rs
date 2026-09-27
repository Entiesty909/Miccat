//! 音频库分类、条目筛选与 Slint 模型填充。

use super::format::format_duration_hns;
use super::*;

/// 分类树模型；内部保存路径，显示层只增加层级缩进。
pub(super) fn audio_categories(cache: &Rc<RefCell<UiCache>>) -> Vec<slint::SharedString> {
    category_paths(cache)
        .into_iter()
        .map(|path| category_display(&path).into())
        .collect()
}

pub(super) fn category_matches(entry: &config::AudioEntry, category: &str) -> bool {
    category == "全部"
        || entry.category == category
        || entry.category.starts_with(&format!("{category}/"))
}

pub(super) fn audio_entry_matches(
    entry: &config::AudioEntry,
    category: &str,
    search: &str,
) -> bool {
    category_matches(entry, category)
        && (search.is_empty() || entry.name.to_ascii_lowercase().contains(search))
}

/// 按当前列表视图下标查找配置条目下标。
pub(super) fn visible_entry(cache: &RefCell<UiCache>, visible_index: usize) -> Option<usize> {
    let cache = cache.borrow();
    let search = cache.audio_search.to_ascii_lowercase();
    visible_entry_in(
        &cache.audio_entries,
        &cache.audio_category,
        &search,
        visible_index,
    )
}

/// 在给定条目源中映射当前分类和搜索下的可见下标。
pub(super) fn visible_entry_in(
    entries: &[config::AudioEntry],
    category: &str,
    search: &str,
    visible_index: usize,
) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| audio_entry_matches(entry, category, search))
        .nth(visible_index)
        .map(|(index, _)| index)
}

pub(super) fn category_paths(cache: &Rc<RefCell<UiCache>>) -> Vec<String> {
    let c = cache.borrow();
    let mut paths = vec!["全部".to_string(), "未分类".to_string()];
    let raw = c
        .audio_categories
        .iter()
        .cloned()
        .chain(c.audio_entries.iter().map(|entry| entry.category.clone()));
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

pub(super) fn category_leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn category_display(path: &str) -> String {
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

/// 按分类与搜索词更新右侧音频列表。
pub(super) fn update_audio_list(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let (category, search) = {
        let c = cache.borrow();
        (
            c.audio_category.clone(),
            c.audio_search.to_ascii_lowercase(),
        )
    };
    let c = cache.borrow();
    let mut names = Vec::new();
    let mut durations = Vec::new();
    for entry in &c.audio_entries {
        if audio_entry_matches(entry, &category, &search) {
            names.push(entry.name.clone().into());
            durations.push(format_duration_hns(entry.duration_hns).into());
        }
    }
    drop(c);
    ui.set_audio_entries(ModelRc::new(VecModel::from(names)));
    ui.set_audio_entry_durations(ModelRc::new(VecModel::from(durations)));
    ui.set_audio_entry_index(-1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, category: &str) -> config::AudioEntry {
        config::AudioEntry {
            name: name.into(),
            path: format!("{name}.wav"),
            category: category.into(),
            duration_hns: 0,
            loop_playback: false,
            hotkey: None,
        }
    }

    #[test]
    fn visible_index_maps_filtered_descendants_to_config_indices() {
        let entries = vec![
            entry("Railgun", "其他"),
            entry("Railgun", "游戏/武器"),
            entry("Rocket", "游戏/武器"),
            entry("Railgun special", "游戏/武器/特殊"),
        ];

        assert_eq!(visible_entry_in(&entries, "游戏", "rail", 0), Some(1));
        assert_eq!(visible_entry_in(&entries, "游戏", "rail", 1), Some(3));
        assert_eq!(visible_entry_in(&entries, "游戏", "rail", 2), None);
    }
}
