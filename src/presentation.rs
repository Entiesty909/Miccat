//! CLI 与桥接层共用的用户可见文本格式化。

use crate::config::Hotkey;
use crate::process::ProcessInfo;

/// 把进程结构格式化为 CLI 和调试列表使用的稳定单行文本。
pub(crate) fn process_label(process: &ProcessInfo) -> String {
    // 标题只作为展示补充，不参与进程匹配；空标题保持紧凑格式。
    // PID 始终保留，避免同名进程在列表和诊断输出中无法区分。
    match &process.title {
        Some(title) if !title.is_empty() => {
            format!("{}  [{}]  {}", process.name, process.pid, title)
        }
        _ => format!("{}  [{}]", process.name, process.pid),
    }
}

/// 把配置中的热键值格式化为界面和诊断中显示的组合键文本。
pub(crate) fn hotkey_label(hotkey: Hotkey) -> String {
    // 修饰键按固定 Ctrl→Shift→Alt 顺序输出，保证配置诊断文本稳定。
    let mut parts = Vec::new();
    // Hotkey.mods 使用 Win32 兼容的位掩码：Alt=1、Ctrl=2、Shift=4。
    if hotkey.mods & 2 != 0 {
        parts.push("Ctrl");
    }
    if hotkey.mods & 4 != 0 {
        parts.push("Shift");
    }
    if hotkey.mods & 1 != 0 {
        parts.push("Alt");
    }
    // 即使键码未知也追加占位名称，确保输出仍表示完整的绑定。
    parts.push(vk_name(hotkey.vk));
    parts.join("+")
}
/// 将常用 Windows 虚拟键码映射为短标签；未知键保留通用占位而不猜测名称。
fn vk_name(vk: u32) -> &'static str {
    // 仅列出界面中常见且无需布局依赖的键；未知码不把数值误显示成错误名称。
    match vk {
        0x20 => "Space",
        0x70 => "F1",
        0x71 => "F2",
        0x72 => "F3",
        0x73 => "F4",
        0x74 => "F5",
        0x75 => "F6",
        0x76 => "F7",
        0x77 => "F8",
        0x78 => "F9",
        0x79 => "F10",
        0x7A => "F11",
        0x7B => "F12",
        _ => "Key",
    }
}

/// 仅验证展示文本的稳定顺序和缺省字段，不依赖窗口或键盘设备。
#[cfg(test)]
mod tests {
    use super::{hotkey_label, process_label};
    use crate::config::Hotkey;
    use crate::process::ProcessInfo;

    /// 有窗口标题时应追加标题，便于用户区分同名进程。
    #[test]
    fn process_label_includes_window_title_when_present() {
        let process = ProcessInfo {
            pid: 42,
            name: "player.exe".into(),
            title: Some("窗口标题".into()),
            sounding: true,
        };
        assert_eq!(process_label(&process), "player.exe  [42]  窗口标题");
    }

    /// 空或缺失标题不应留下多余分隔符。
    #[test]
    fn process_label_omits_empty_window_title() {
        let process = ProcessInfo {
            pid: 42,
            name: "player.exe".into(),
            title: None,
            sounding: false,
        };
        assert_eq!(process_label(&process), "player.exe  [42]");
    }

    /// 修饰键顺序固定，避免相同组合在不同调用点显示不一致。
    #[test]
    fn hotkey_label_orders_modifiers_consistently() {
        assert_eq!(
            hotkey_label(Hotkey {
                mods: 2 | 4,
                vk: 0x70
            }),
            "Ctrl+Shift+F1"
        );
    }
}
