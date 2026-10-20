//! 音频库行选择、分类修改、分页、排序、搜索和应用内快捷键。

use super::super::library::visible_rank;
use super::super::*;

/// 绑定音频库选择、分类、分页、排序、搜索和条目快捷键事件。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_selection(ui, state);
    register_category(ui, state);
    register_page(ui, state);
    register_sort(ui, state);
    register_search(ui, state);
    register_audio_shortcuts(ui, state);
}
/// 处理可见条目选择，并同步其分类、循环状态和 UI 索引。
fn register_selection(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    // Slint 只提供当前页的可见行号，播放配置必须通过 visible_entry 还原到缓存索引。
    ui.on_audio_entry_selected(move |visible_index| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let Some(actual_index) = visible_entry(&cache, visible_index.max(0) as usize) else {
            return;
        };
        let entry_state = {
            let cache = cache.borrow();
            let Some(entry) = cache.audio_entries.get(actual_index) else {
                return;
            };
            (entry.category.clone(), entry.loop_playback)
        };
        // 条目分类下拉框使用同一份可见分类路径模型，避免显示索引与保存路径错位。
        if let Some(category_index) = category_paths(&cache)
            .iter()
            .position(|path| path == &entry_state.0)
        {
            ui.set_audio_entry_category_index(category_index as i32);
        }
        // 选中条目时同步循环开关，编辑或播放入口读取到的状态才能对应当前行。
        ui.set_player_loop(entry_state.1);
        ui.set_audio_entry_index(visible_index);
    });
}
/// 修改当前条目的分类；可见索引先转换为缓存实际索引。
fn register_category(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let ui_weak = ui.as_weak();
    // 分类变更只修改实际条目的 category 字段，列表刷新后保留原可见行选择。
    ui.on_entry_category_changed(move |category_index| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let visible_index = ui.get_audio_entry_index().max(0) as usize;
        let Some(actual_index) = visible_entry(&cache, visible_index) else {
            return;
        };
        let Some(category) = category_paths(&cache)
            .get(category_index.max(0) as usize)
            .cloned()
        else {
            return;
        };
        // 先改缓存再复制到配置，保存队列看到的始终是完整条目快照。
        {
            let mut cache_state = cache.borrow_mut();
            let Some(entry) = cache_state.audio_entries.get_mut(actual_index) else {
                return;
            };
            entry.category = category;
            app_config.borrow_mut().audio_entries = cache_state.audio_entries.clone();
        }
        save_queue.schedule();
        update_audio_list(&ui, &cache);
        ui.set_audio_entry_index(visible_index as i32);
    });
}

/// 接收分页变化并刷新当前页；负值被钳制为第一页。
fn register_page(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    // 页码是 UI 派生态，负值来自边界输入时统一回到第一页，不写入持久化配置。
    ui.on_audio_page_changed(move |page| {
        let Some(ui) = ui_weak.upgrade() else { return };
        // 刷新由统一列表构建函数完成，分页、搜索和排序因此共享相同索引规则。
        cache.borrow_mut().audio_page = page.max(0) as usize;
        update_audio_list(&ui, &cache);
    });
}

/// 切换排序方向并回到第一页，避免当前页号超出新排序结果。
fn register_sort(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    // 排序方向改变会重排可见索引，必须清零页码避免继续使用旧结果的页号。
    ui.on_audio_sort_toggled(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        {
            let mut cache = cache.borrow_mut();
            cache.audio_sort_ascending = !cache.audio_sort_ascending;
            cache.audio_page = 0;
        }
        update_audio_list(&ui, &cache);
    });
}

/// 更新搜索字符串并回到第一页；搜索只影响可见列表，不改持久化条目。
fn register_search(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    // 搜索词只存于 UiCache；更新后从第一页重新计算筛选结果。
    ui.on_audio_search_edited(move |search| {
        let Some(ui) = ui_weak.upgrade() else { return };
        {
            let mut cache = cache.borrow_mut();
            // 搜索改变可见集合，旧页号可能越界，所以与搜索词原子地重置。
            cache.audio_search = search.to_string();
            cache.audio_page = 0;
        }
        update_audio_list(&ui, &cache);
    });
}

/// 注册音频条目快捷键的录制与分发两条路径。
fn register_audio_shortcuts(ui: &MainWindow, state: &AppState) {
    register_audio_shortcut_capture(ui, state);
    register_audio_shortcut_dispatch(ui, state);
}

/// 发起全局快捷键录制，并把 Slint 修饰键转换为配置中的位掩码。
fn register_audio_shortcut_capture(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let hotkey_server = state.hotkey_server.clone();
    let ui_weak = ui.as_weak();
    // 录制入口只切换监听状态并提示用户，实际绑定在 captured 回调中提交。
    ui.on_audio_entry_hotkey_bind(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_audio_hotkey_listening(ui.get_audio_entry_index() >= 0);
            ui.set_error_text("按下新的全局快捷键（可带 Ctrl/Alt/Shift）".into());
            super::super::diagnostics::report_info(&ui, "按下新的全局快捷键");
        }
    });

    let cache = cache.clone();
    let app_config = app_config.clone();
    let save_queue = save_queue.clone();
    let hotkey_server = hotkey_server.clone();
    let ui_weak = ui.as_weak();
    // Esc 取消录制；无修饰单键会拦截正常输入，因此拒绝保存。
    // Esc 仅取消监听；其余文本先转换成 Win32 虚拟键再参与冲突校验。
    ui.on_audio_hotkey_binding_captured(move |text, ctrl, shift, alt| {
        let Some(ui) = ui_weak.upgrade() else { return };
        ui.set_audio_hotkey_listening(false);
        if text == "\u{1b}" {
            ui.set_error_text("".into());
            return;
        }
        let Some(vk) = super::super::input::slint_key_to_vk(text.as_str()) else {
            ui.set_error_text("不支持该按键".into());
            return;
        };
        // 修饰键位掩码约定为 Alt=1、Ctrl=2、Shift=4，与全局注册表保持一致。
        let mut mods = 0u32;
        if alt {
            mods |= 1;
        }
        if ctrl {
            mods |= 2;
        }
        if shift {
            mods |= 4;
        }
        // 不带修饰键的按键不进入全局注册表，避免吞掉列表外的正常文字输入。
        let binding = config::Hotkey { mods, vk };
        // 无修饰键的单键做成全局会吞掉正常打字，先给出明确警告。
        if mods == 0 {
            ui.set_error_text("单键全局热键会拦截正常输入，建议加 Ctrl 或 Alt".into());
            return;
        }
        let visible_index = ui.get_audio_entry_index().max(0) as usize;
        let Some(actual_index) = visible_entry(&cache, visible_index) else {
            return;
        };
        // 热键必须全等比较 mods 与虚拟键，避免不同修饰符误判或漏判冲突。
        if let Some(reason) = binding_conflict(&app_config, actual_index, binding) {
            ui.set_error_text(reason.into());
            return;
        }
        // 写入热键后立即同步系统注册，失败状态由统一输入层反馈给 UI。
        {
            let mut cache_state = cache.borrow_mut();
            let Some(entry) = cache_state.audio_entries.get_mut(actual_index) else {
                return;
            };
            entry.hotkey_binding = Some(binding);
            app_config.borrow_mut().audio_entries = cache_state.audio_entries.clone();
        }
        save_queue.schedule();
        // 立即重注册，并检查是否被外部程序占用。
        super::super::input::resync_bindings(&ui, &hotkey_server, &app_config);
        update_audio_list(&ui, &cache);
        ui.set_audio_entry_index(visible_index as i32);
    });
}

/// 检查新绑定是否与已有全局热键重复；重复时返回用户可见原因。
///
/// 按 `(mods, vk)` 全等比较：只比 vk 会把 `Ctrl+A` 与 `A` 误判成冲突，
/// 也会漏掉「同键不同修饰符」的真重复。
fn binding_conflict(
    app_config: &Rc<RefCell<config::AppConfig>>,
    actual_index: usize,
    binding: config::Hotkey,
) -> Option<String> {
    // 同一条目在重绑时由 actual_index 排除自身，否则原绑定会把合法替换误报为冲突。
    let config = app_config.borrow();
    // 冲突比较必须同时包含修饰位和虚拟键值，二者共同定义一个全局组合键。
    let same = |other: &config::Hotkey| other.mods == binding.mods && other.vk == binding.vk;
    // 先检查路由控制热键，避免音频条目绑定覆盖全局播放控制。
    if same(&config.hotkeys.toggle_route) {
        return Some("与「开始/停止转发」热键重复".into());
    }
    if config.hotkeys.previous_track.as_ref().is_some_and(same) {
        return Some("与「上一首」热键重复".into());
    }
    if config.hotkeys.next_track.as_ref().is_some_and(same) {
        return Some("与「下一首」热键重复".into());
    }
    if config.hotkeys.stop_playback.as_ref().is_some_and(same) {
        return Some("与「停止播放」热键重复".into());
    }
    // 条目热键按持久化数组下标比较，并排除正在编辑的条目自身。
    let clash = config
        .audio_entries
        .iter()
        .enumerate()
        .any(|(index, entry)| {
            index != actual_index && entry.hotkey_binding.as_ref().is_some_and(&same)
        });
    if clash {
        return Some("该快捷键已分配给其他音频".into());
    }
    None
}

/// 接收全局热键事件：新组合键按实际条目索引播放，旧单键按标签查找条目。
fn register_audio_shortcut_dispatch(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let engine = state.engine.clone();
    let ui_weak = ui.as_weak();
    // 组合键事件携带持久化条目索引，不受当前搜索、排序和分页模型影响。
    // 全局分发不依赖当前分页；先切换到条目所属分类，再交给播放器统一启动。
    // 全局热键按条目下标直接触发播放。
    ui.on_audio_hotkey_index(move |index| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let actual = index.max(0) as usize;
        let category = {
            let cache_state = cache.borrow();
            let Some(entry) = cache_state.audio_entries.get(actual) else {
                return;
            };
            entry.category.clone()
        };
        let categories = category_paths(&cache);
        // 收到全局组合键后切换到条目分类，随后播放逻辑不依赖原先可见页。
        if let Some(category_index) = categories.iter().position(|path| path == &category) {
            ui.set_audio_category_index(category_index as i32);
            cache.borrow_mut().audio_category = category;
        }
        let target = cache.borrow().playback_target;
        cache.borrow_mut().paused_audio = None;
        engine.stop_file();
        if let Some(entry) = cache.borrow().audio_entries.get(actual).cloned() {
            super::player::play_entry(&ui, &engine, &entry, target);
        }
    });

    let cache = state.cache.clone();
    let ui_weak = ui.as_weak();
    // 旧版单键事件只有标签文本，因此先规范化，再在库条目中反查实际索引。
    ui.on_audio_hotkey_pressed(move |key| {
        let Some(hotkey) = shortcut_label(key.as_str()) else {
            return;
        };
        let Some(actual_index) = cache.borrow().audio_entries.iter().position(|entry| {
            entry
                .hotkey
                .as_deref()
                .is_some_and(|bound| bound.eq_ignore_ascii_case(&hotkey))
        }) else {
            return;
        };
        let Some(ui) = ui_weak.upgrade() else { return };
        let category = cache.borrow().audio_entries[actual_index].category.clone();
        let categories = category_paths(&cache);
        let Some(category_index) = categories.iter().position(|path| path == &category) else {
            return;
        };
        {
            let mut cache_state = cache.borrow_mut();
            // 单键路径清除搜索并回到条目所属分类，保证随后设置的可见行确实存在。
            cache_state.audio_category = category;
            cache_state.audio_search.clear();
            cache_state.audio_page = 0;
            cache_state.force_audio_play = true;
            cache_state.playback_target = crate::engine::PlaybackTarget::Both;
        }
        // visible_rank 计算排序/筛选后的页号；先刷新列表，再设置页内行索引。
        let rank = visible_rank(&cache, actual_index).unwrap_or(0);
        cache.borrow_mut().audio_page = rank / super::super::library::PAGE_SIZE;
        ui.set_page(1);
        ui.set_audio_category_index(category_index as i32);
        update_audio_list(&ui, &cache);
        let visible_index =
            visible_rank(&cache, actual_index).unwrap_or(0) % super::super::library::PAGE_SIZE;
        ui.set_audio_entry_index(visible_index as i32);
        ui.invoke_audio_entry_play(0);
    });
}

/// 将 Slint 按键文本规范化为可保存的 Space、功能键或大写字母标签。
fn shortcut_label(key: &str) -> Option<String> {
    // Slint 文本先经过统一虚拟键映射，再限制为可持久化的 Space、F 键和字母/数字。
    let virtual_key = super::super::input::slint_key_to_vk(key)?;
    match virtual_key {
        // Space 使用稳定的用户标签，避免把控制字符直接保存到配置。
        0x20 => Some("Space".to_string()),
        // Win32 功能键虚拟键 0x70..=0x7b 对应 F1..F12。
        0x70..=0x7b => Some(format!("F{}", virtual_key - 0x70 + 1)),
        // 字母和数字按虚拟键值转成大写/原样 ASCII 标签，其他键不参与旧单键路径。
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(virtual_key).map(|key| key.to_string()),
        _ => None,
    }
}

/// 快捷键录制应稳定规范化大小写、空格和功能键文本。
#[cfg(test)]
mod tests {
    use super::shortcut_label;

    /// 验证功能键、字母和空格输入都转换为稳定的快捷键标签。
    #[test]
    fn shortcut_capture_normalizes_function_and_character_keys() {
        assert_eq!(shortcut_label("\u{F001}"), Some("F1".into()));
        assert_eq!(shortcut_label("a"), Some("A".into()));
        assert_eq!(shortcut_label(" "), Some("Space".into()));
        assert_eq!(shortcut_label("Escape"), None);
    }
}
