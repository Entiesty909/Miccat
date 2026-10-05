//! 音频库 Slint 模型适配：分类统计、排序分页和本地文件元数据。

use super::format::format_duration_hns;
use super::*;
use std::path::Path;

pub(super) const PAGE_SIZE: usize = 20;

/// 分类树模型；内部保存路径，显示层只增加层级缩进。
pub(super) fn audio_categories(cache: &Rc<RefCell<UiCache>>) -> Vec<slint::SharedString> {
    let cache = cache.borrow();
    crate::library::category_labels(&cache.audio_categories, &cache.audio_entries)
        .into_iter()
        .map(Into::into)
        .collect()
}

/// 返回每个显示分类中实际条目的数量。
pub(super) fn audio_category_counts(cache: &Rc<RefCell<UiCache>>) -> Vec<i32> {
    let cache = cache.borrow();
    crate::library::category_paths(&cache.audio_categories, &cache.audio_entries)
        .iter()
        .map(|category| {
            cache
                .audio_entries
                .iter()
                .filter(|entry| crate::library::category_matches(entry, category))
                .count() as i32
        })
        .collect()
}

/// 条目快捷键文本：优先全局组合键，其次旧的前台单键。
pub(super) fn entry_shortcut(entry: &config::AudioEntry) -> Option<String> {
    if let Some(binding) = entry.hotkey_binding {
        return Some(binding.label());
    }
    entry.hotkey.clone()
}

pub(super) fn audio_hotkey_labels(cache: &Rc<RefCell<UiCache>>) -> Vec<slint::SharedString> {
    cache
        .borrow()
        .audio_entries
        .iter()
        .filter_map(|entry| {
            entry_shortcut(entry).map(|shortcut| format!("{shortcut}  {}", entry.name).into())
        })
        .collect()
}

/// 把当前页的行下标映射回稳定配置条目下标。
pub(super) fn visible_entry(cache: &RefCell<UiCache>, visible_index: usize) -> Option<usize> {
    let cache = cache.borrow();
    filtered_entry_indices(&cache)
        .into_iter()
        .skip(cache.audio_page * PAGE_SIZE)
        .nth(visible_index)
}

fn filtered_entry_indices(cache: &UiCache) -> Vec<usize> {
    let search = cache.audio_search.to_lowercase();
    let mut indices = cache
        .audio_entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| crate::library::category_matches(entry, &cache.audio_category))
        .filter(|(_, entry)| search.is_empty() || entry.name.to_lowercase().contains(&search))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        let ordering = cache.audio_entries[*left]
            .name
            .to_lowercase()
            .cmp(&cache.audio_entries[*right].name.to_lowercase());
        if cache.audio_sort_ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
    indices
}

/// 返回条目在当前分类/搜索/排序序列中的可见位置。
pub(super) fn visible_rank(cache: &RefCell<UiCache>, actual_index: usize) -> Option<usize> {
    filtered_entry_indices(&cache.borrow())
        .iter()
        .position(|index| *index == actual_index)
}

pub(super) fn category_paths(cache: &Rc<RefCell<UiCache>>) -> Vec<String> {
    let cache = cache.borrow();
    crate::library::category_paths(&cache.audio_categories, &cache.audio_entries)
}

pub(super) fn category_leaf(path: &str) -> &str {
    crate::library::category_leaf(path)
}

/// 按分类、搜索词和分页刷新真实条目模型。
pub(super) fn update_audio_list(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let (page, page_count, total, entries) = {
        let mut cache = cache.borrow_mut();
        let indices = filtered_entry_indices(&cache);
        let page_count = indices.len().div_ceil(PAGE_SIZE).max(1);
        cache.audio_page = cache.audio_page.min(page_count - 1);
        let page = cache.audio_page;
        let start = page * PAGE_SIZE;
        let entries = indices
            .iter()
            .skip(start)
            .take(PAGE_SIZE)
            .map(|index| cache.audio_entries[*index].clone())
            .collect::<Vec<_>>();
        (page, page_count, indices.len(), entries)
    };
    let mut names = Vec::with_capacity(entries.len());
    let mut durations = Vec::with_capacity(entries.len());
    let mut metadata = Vec::with_capacity(entries.len());
    let mut categories = Vec::with_capacity(entries.len());
    let mut statuses = Vec::with_capacity(entries.len());
    let mut paths = Vec::with_capacity(entries.len());
    let mut sizes = Vec::with_capacity(entries.len());
    let mut hotkeys = Vec::with_capacity(entries.len());

    for entry in entries {
        // 必须先于 name/path 的部分移动取值，否则无法整借用 entry。
        hotkeys.push(entry_shortcut(&entry).unwrap_or_default().into());
        names.push(entry.name.into());
        durations.push(format_duration_hns(entry.duration_hns).into());
        let format = Path::new(&entry.path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("音频")
            .to_ascii_uppercase();
        let sample_rate = if entry.sample_rate >= 1000 {
            format!("{:.1} kHz", entry.sample_rate as f32 / 1000.0)
        } else {
            format!("{} Hz", entry.sample_rate)
        };
        let bitrate = estimated_bitrate(entry.file_size_bytes, entry.duration_hns);
        metadata.push(format!("{format} · {sample_rate} · {bitrate}").into());
        categories.push(crate::library::category_leaf(&entry.category).into());
        statuses.push(if entry.duration_hns > 0 {
            "已导入".into()
        } else {
            "待扫描".into()
        });
        paths.push(entry.path.into());
        sizes.push(format_file_size(entry.file_size_bytes).into());
    }

    ui.set_audio_entries(ModelRc::new(VecModel::from(names)));
    ui.set_audio_entry_durations(ModelRc::new(VecModel::from(durations)));
    ui.set_audio_entry_metadata(ModelRc::new(VecModel::from(metadata)));
    ui.set_audio_entry_categories(ModelRc::new(VecModel::from(categories)));
    ui.set_audio_entry_statuses(ModelRc::new(VecModel::from(statuses)));
    ui.set_audio_entry_paths(ModelRc::new(VecModel::from(paths)));
    ui.set_audio_entry_sizes(ModelRc::new(VecModel::from(sizes)));
    ui.set_audio_entry_hotkeys(ModelRc::new(VecModel::from(hotkeys)));
    ui.set_audio_hotkey_labels(ModelRc::new(VecModel::from(audio_hotkey_labels(cache))));
    ui.set_audio_page(page as i32);
    ui.set_audio_page_count(page_count as i32);
    ui.set_audio_total_count(total as i32);
    ui.set_audio_category_counts(ModelRc::new(VecModel::from(audio_category_counts(cache))));
    ui.set_audio_sort_ascending(cache.borrow().audio_sort_ascending);
    if total > 0 {
        ui.set_audio_entry_index(0);
        if let Some(actual) = visible_entry(cache, 0) {
            let state = cache.borrow();
            if let Some(entry) = state.audio_entries.get(actual) {
                if let Some(category_index) = state
                    .audio_categories
                    .iter()
                    .position(|category| category == &entry.category)
                {
                    ui.set_audio_entry_category_index(category_index as i32);
                }
                ui.set_player_loop(entry.loop_playback);
            }
        }
    } else {
        ui.set_audio_entry_index(-1);
    }
}

fn estimated_bitrate(file_size_bytes: u64, duration_hns: u64) -> String {
    if file_size_bytes == 0 || duration_hns == 0 {
        return "码率未知".to_string();
    }
    let seconds = duration_hns as f64 / 10_000_000.0;
    let kilobits = (file_size_bytes as f64 * 8.0 / seconds / 1000.0).round() as u64;
    format!("≈{kilobits} kbps")
}

fn format_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "大小未知".to_string();
    }
    if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
