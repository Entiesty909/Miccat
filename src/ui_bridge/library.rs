//! 音频库 Slint 模型适配；分类语义由根领域模块提供。

use super::format::format_duration_hns;
use super::*;

/// 分类树模型；内部保存路径，显示层只增加层级缩进。
pub(super) fn audio_categories(cache: &Rc<RefCell<UiCache>>) -> Vec<slint::SharedString> {
    let cache = cache.borrow();
    crate::library::category_labels(&cache.audio_categories, &cache.audio_entries)
        .into_iter()
        .map(Into::into)
        .collect()
}

pub(super) fn audio_entry_matches(
    entry: &config::AudioEntry,
    category: &str,
    search: &str,
) -> bool {
    crate::library::category_matches(entry, category)
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
    crate::library::visible_entry_index(entries, category, search, visible_index)
}

pub(super) fn category_paths(cache: &Rc<RefCell<UiCache>>) -> Vec<String> {
    let cache = cache.borrow();
    crate::library::category_paths(&cache.audio_categories, &cache.audio_entries)
}

pub(super) fn category_leaf(path: &str) -> &str {
    crate::library::category_leaf(path)
}

/// 按分类与搜索词更新右侧音频列表。
pub(super) fn update_audio_list(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let (category, search) = {
        let cache = cache.borrow();
        (
            cache.audio_category.clone(),
            cache.audio_search.to_ascii_lowercase(),
        )
    };
    let cache = cache.borrow();
    let mut names = Vec::new();
    let mut durations = Vec::new();
    for entry in &cache.audio_entries {
        if audio_entry_matches(entry, &category, &search) {
            names.push(entry.name.clone().into());
            durations.push(format_duration_hns(entry.duration_hns).into());
        }
    }
    drop(cache);
    ui.set_audio_entries(ModelRc::new(VecModel::from(names)));
    ui.set_audio_entry_durations(ModelRc::new(VecModel::from(durations)));
    ui.set_audio_entry_index(-1);
}
