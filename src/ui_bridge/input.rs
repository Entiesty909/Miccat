//! 桥接 Slint 热键捕获与系统热键服务。

use super::*;
use crate::hotkey::{ENTRY_ID_BASE, HotkeyServer};
use std::collections::BTreeMap;
use std::sync::Arc;

/// 内置动作：开始/停止转发。
pub(super) const ACTION_TOGGLE_ROUTE: i32 = 1;
/// 内置动作：上一首。
pub(super) const ACTION_PREVIOUS: i32 = 2;
/// 内置动作：下一首。
pub(super) const ACTION_NEXT: i32 = 3;
/// 内置动作：停止播放。
pub(super) const ACTION_STOP_PLAYBACK: i32 = 4;

/// 启动安全系统热键服务；回调按绑定 id 分发到对应动作。
pub(super) fn start_hotkey(
    ui: &MainWindow,
    hotkey: &Rc<RefCell<config::Hotkey>>,
    bindings: BTreeMap<i32, config::Hotkey>,
) -> HotkeyServer {
    let ui_weak = ui.as_weak();
    let _ = hotkey;
    HotkeyServer::start(
        crate::hotkey::HotkeyMode::System,
        bindings,
        Arc::new(move |id| {
            // 热键线程不能直接触碰 Slint；通过事件循环把动作转回 UI 线程，并用弱引用避免关闭后悬挂。
            let ui_weak = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = ui_weak.upgrade() else { return };
                dispatch(&ui, id);
            });
        }),
    )
}

/// 按绑定 id 触发对应动作。
fn dispatch(ui: &MainWindow, id: i32) {
    match id {
        // 内置动作使用固定 ID，音频条目从 ENTRY_ID_BASE 起按库内下标编码。
        ACTION_TOGGLE_ROUTE => ui.invoke_start_stop(),
        ACTION_PREVIOUS => ui.invoke_player_previous(),
        ACTION_NEXT => ui.invoke_player_next(),
        ACTION_STOP_PLAYBACK => ui.invoke_audio_stop(),
        _ if id >= ENTRY_ID_BASE => {
            let index = id - ENTRY_ID_BASE;
            ui.invoke_audio_hotkey_index(index);
        }
        _ => {}
    }
}

/// 重注册绑定并把注册失败回报给 UI。
pub(super) fn resync_bindings(
    ui: &MainWindow,
    server: &HotkeyServer,
    app_config: &Rc<RefCell<config::AppConfig>>,
) {
    server.set_bindings(collect_bindings(&app_config.borrow()));
    let failures = server.take_failures();
    if let Some(failure) = failures.first() {
        ui.set_error_text(format!("热键注册失败：{}", failure.reason).into());
        super::diagnostics::report_warning(ui, &failure.reason);
    }
}

/// 把配置里的热键汇总成「id → 组合」的绑定表。
pub(super) fn collect_bindings(config: &config::AppConfig) -> BTreeMap<i32, config::Hotkey> {
    let mut bindings = BTreeMap::new();
    bindings.insert(ACTION_TOGGLE_ROUTE, config.hotkeys.toggle_route);
    if let Some(hk) = config.hotkeys.previous_track {
        bindings.insert(ACTION_PREVIOUS, hk);
    }
    if let Some(hk) = config.hotkeys.next_track {
        bindings.insert(ACTION_NEXT, hk);
    }
    if let Some(hk) = config.hotkeys.stop_playback {
        bindings.insert(ACTION_STOP_PLAYBACK, hk);
    }
    for (index, entry) in config.audio_entries.iter().enumerate() {
        // 条目热键 ID 的下标必须对应 AppConfig.audio_entries，不能使用当前分页模型下标。
        if let Some(hk) = entry.hotkey_binding {
            bindings.insert(ENTRY_ID_BASE + index as i32, hk);
        }
    }
    bindings
}

/// 将 Slint 捕获到的文本转换为 Windows 虚拟键码。
pub(crate) fn slint_key_to_vk(text: &str) -> Option<u32> {
    // Slint 传入的是显示文本，系统注册需要 ASCII/功能键对应的 Win32 VK 常量。
    if text == " " || text.eq_ignore_ascii_case("space") {
        return Some(0x20);
    }
    if let Some(number) = text.strip_prefix('F').or_else(|| text.strip_prefix('f'))
        && let Ok(number) = number.parse::<u32>()
        && (1..=12).contains(&number)
    {
        return Some(0x70 + number - 1);
    }
    let mut characters = text.chars();
    let key = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let value = key as u32;
    if (0xF001..=0xF00C).contains(&value) {
        return Some(0x70 + value - 0xF001);
    }
    let value = key.to_ascii_uppercase() as u32;
    if (0x30..=0x39).contains(&value) || (0x41..=0x5A).contains(&value) {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::slint_key_to_vk;

    #[test]
    /// 覆盖空格、功能键、字母和数字，拒绝多字符或未支持的按键名称。
    fn maps_supported_key_text_to_virtual_keys() {
        assert_eq!(slint_key_to_vk(" "), Some(0x20));
        assert_eq!(slint_key_to_vk("F12"), Some(0x7B));
        assert_eq!(slint_key_to_vk("a"), Some(0x41));
        assert_eq!(slint_key_to_vk("Escape"), None);
    }
}
