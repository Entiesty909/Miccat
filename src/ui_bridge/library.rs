//! 音频库 Slint 模型适配：分类统计、排序分页和本地文件元数据。

use super::format::format_duration_hns;
use super::*;
use std::path::Path;

pub(super) const PAGE_SIZE: usize = 20;
/// 音频库编辑对话框保留默认音符和首字图标两种字体图标。
pub(super) const AUDIO_ICON_GLYPHS: [&str; 1] = ["\u{EC4F}"];

/// 构造分类行使用的 glyph 与自定义图标模型。
pub(super) fn category_icon_models(
    cache: &Rc<RefCell<UiCache>>,
    app_config: &config::AppConfig,
) -> (Vec<slint::SharedString>, Vec<slint::Image>) {
    let paths = category_paths(cache);
    let mut glyphs = Vec::with_capacity(paths.len());
    let mut images = Vec::with_capacity(paths.len());
    for path in paths {
        match app_config.audio_category_icons.get(&path) {
            Some(icon) if icon.starts_with("file:") => {
                let icon_path = &icon[5..];
                glyphs.push("".into());
                images.push(crate::platform::icon::file_icon(icon_path));
            }
            Some(icon) => {
                glyphs.push(icon.clone().into());
                images.push(slint::Image::default());
            }
            None => {
                glyphs.push(if path == "未分类" {
                    "\u{E8F4}".into()
                } else {
                    "\u{E8B7}".into()
                });
                images.push(slint::Image::default());
            }
        }
    }
    (glyphs, images)
}

/// 解析持久化图标到编辑对话框中的预设索引。
pub(super) fn audio_icon_index(glyph: &str) -> i32 {
    if glyph == AUDIO_ICON_GLYPHS[0] { 0 } else { 5 }
}

/// 根据编辑对话框选项生成持久化 glyph。
pub(super) fn audio_icon_value(index: i32, name: &str) -> String {
    if index == 5 {
        return name.chars().next().unwrap_or('音').to_string();
    }
    AUDIO_ICON_GLYPHS[0].to_owned()
}

/// 判断分类目标路径是否与同级路径冲突；原路径不视为冲突，以允许只编辑图标。
pub(super) fn category_path_conflicts(old_path: &str, new_path: &str, paths: &[String]) -> bool {
    new_path != old_path
        && paths
            .iter()
            .any(|path| path != old_path && path == new_path)
}

/// 分类树模型；内部保存路径，显示层只增加层级缩进。
pub(super) fn audio_categories(cache: &Rc<RefCell<UiCache>>) -> Vec<slint::SharedString> {
    let cache = cache.borrow();
    crate::library::category_labels(&cache.audio_categories, &cache.audio_entries)
        .into_iter()
        .map(Into::into)
        .collect()
}

/// 同步分类名称、计数、图标模型到 Slint。
pub(super) fn update_audio_categories(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    app_config: &config::AppConfig,
) {
    ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(cache))));
    ui.set_audio_category_counts(ModelRc::new(VecModel::from(audio_category_counts(cache))));
    let (glyphs, images) = category_icon_models(cache, app_config);
    ui.set_audio_category_icon_images(ModelRc::new(VecModel::from(images)));
    ui.set_audio_category_depths(ModelRc::new(VecModel::from(category_depths(cache))));
    ui.set_audio_category_icon_glyphs(ModelRc::new(VecModel::from(glyphs)));
}

/// 返回分类树每行的父子层级。
pub(super) fn category_depths(cache: &Rc<RefCell<UiCache>>) -> Vec<i32> {
    category_paths(cache)
        .into_iter()
        .map(|path| path.matches('/').count() as i32)
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

/// 为路由添加源对话框提供全库真实条目和编辑元数据。
pub(super) fn update_audio_catalog(ui: &MainWindow, cache: &Rc<RefCell<UiCache>>) {
    let state = cache.borrow();
    let mut names = Vec::with_capacity(state.audio_entries.len());
    let mut metadata = Vec::with_capacity(state.audio_entries.len());
    let mut durations = Vec::with_capacity(state.audio_entries.len());
    let mut sizes = Vec::with_capacity(state.audio_entries.len());
    let mut categories = Vec::with_capacity(state.audio_entries.len());
    let mut hotkeys = Vec::with_capacity(state.audio_entries.len());
    let mut loops = Vec::with_capacity(state.audio_entries.len());
    for entry in &state.audio_entries {
        let format = Path::new(&entry.path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        names.push(entry.name.clone().into());
        metadata.push(
            format!(
                "{format} · {} · {}",
                estimated_bitrate(entry.file_size_bytes, entry.duration_hns),
                format_file_size(entry.file_size_bytes)
            )
            .into(),
        );
        durations.push(format_duration_hns(entry.duration_hns).into());
        sizes.push(format_file_size(entry.file_size_bytes).into());
        categories.push(
            crate::library::category_leaf(&entry.category)
                .to_string()
                .into(),
        );
        hotkeys.push(entry_shortcut(entry).unwrap_or_default().into());
        loops.push(entry.loop_playback);
    }
    drop(state);
    ui.set_audio_library_catalog(ModelRc::new(VecModel::from(names)));
    ui.set_audio_library_metadata(ModelRc::new(VecModel::from(metadata)));
    ui.set_audio_library_durations(ModelRc::new(VecModel::from(durations)));
    ui.set_audio_library_sizes(ModelRc::new(VecModel::from(sizes)));
    ui.set_audio_library_categories(ModelRc::new(VecModel::from(categories)));
    ui.set_audio_library_hotkeys(ModelRc::new(VecModel::from(hotkeys)));
    ui.set_audio_library_loops(ModelRc::new(VecModel::from(loops)));
}

/// 条目快捷键文本：优先全局组合键，其次旧的前台单键。
pub(super) fn entry_shortcut(entry: &config::AudioEntry) -> Option<String> {
    if let Some(binding) = entry.hotkey_binding {
        return Some(crate::presentation::hotkey_label(binding));
    }
    entry.hotkey.clone()
}

/// 构造带快捷键的音频条目显示文本，顺序与配置条目保持一致。
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

/// 按分类、搜索词和排序规则返回配置条目下标，不改变配置数组顺序。
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

/// 返回当前缓存中展开后的分类路径列表，供下拉和树视图共用。
pub(super) fn category_paths(cache: &Rc<RefCell<UiCache>>) -> Vec<String> {
    let cache = cache.borrow();
    crate::library::category_paths(&cache.audio_categories, &cache.audio_entries)
}

/// 返回分类路径最后一级名称，空路径由领域层规则处理。
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
    let mut icon_images = Vec::with_capacity(entries.len());
    let mut icon_glyphs = Vec::with_capacity(entries.len());
    let mut categories = Vec::with_capacity(entries.len());
    let mut statuses = Vec::with_capacity(entries.len());
    let mut paths = Vec::with_capacity(entries.len());
    let mut sizes = Vec::with_capacity(entries.len());
    let mut hotkeys = Vec::with_capacity(entries.len());

    for entry in entries {
        // 必须先于 name/path 的部分移动取值，否则无法整借用 entry。
        let icon_path = entry.icon_path.as_deref().filter(|path| !path.is_empty());
        icon_images
            .push(icon_path.map_or_else(slint::Image::default, crate::platform::icon::file_icon));
        icon_glyphs.push(if icon_path.is_some() {
            String::new().into()
        } else {
            entry.icon_glyph.clone().into()
        });
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

    update_audio_catalog(ui, cache);
    ui.set_audio_entry_icon_images(ModelRc::new(VecModel::from(icon_images)));
    ui.set_audio_entry_icon_glyphs(ModelRc::new(VecModel::from(icon_glyphs)));
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

/// 依据字节数和 100 ns 时长估算码率；任一元数据缺失时返回未知。
fn estimated_bitrate(file_size_bytes: u64, duration_hns: u64) -> String {
    if file_size_bytes == 0 || duration_hns == 0 {
        return "码率未知".to_string();
    }
    let seconds = duration_hns as f64 / 10_000_000.0;
    let kilobits = (file_size_bytes as f64 * 8.0 / seconds / 1000.0).round() as u64;
    format!("≈{kilobits} kbps")
}

/// 将字节数转换为用户可读的 KB/MB 文案，零值表示元数据未知。
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

#[cfg(test)]
mod tests {
    use super::{AUDIO_ICON_GLYPHS, audio_icon_index, category_path_conflicts};

    #[test]
    /// 默认音符 glyph 必须映射到编辑器的默认图标选项。
    fn default_music_note_maps_to_default_icon_option() {
        assert_eq!(audio_icon_index(AUDIO_ICON_GLYPHS[0]), 0);
    }

    #[test]
    /// 非默认 glyph 使用首字图标选项。
    fn non_default_glyph_maps_to_first_letter_option() {
        assert_eq!(audio_icon_index("A"), 5);
    }

    #[test]
    /// 原路径不冲突，允许只修改图标而不重命名分类。
    fn unchanged_category_name_allows_icon_edit() {
        let paths = vec!["效果音".to_string(), "系统/提示".to_string()];

        assert!(!category_path_conflicts("效果音", "效果音", &paths));
    }

    #[test]
    /// 已存在的同级目标路径必须拒绝重命名。
    fn category_rename_still_rejects_existing_sibling_path() {
        let paths = vec!["效果音".to_string(), "提示音".to_string()];

        assert!(category_path_conflicts("效果音", "提示音", &paths));
    }
}
