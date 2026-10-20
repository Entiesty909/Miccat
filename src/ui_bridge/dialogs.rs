//! 使用原生 Windows 对话框选择音频与图标文件。

use std::path::PathBuf;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::core::{PCWSTR, PWSTR};

/// 使用原生文件选择器选择音频文件。
#[allow(clippy::field_reassign_with_default)]
pub(super) fn select_audio_file() -> Option<PathBuf> {
    // Win32 文件对话框把完整路径写回调用方提供的 UTF-16 缓冲区，固定容量避免返回悬空指针。
    let mut file = [0u16; 32_768];
    // OPENFILENAMEW 要求筛选器使用“描述、模式”成对字符串，并以额外的 NUL 结束整个列表。
    let filter: Vec<u16> = "支持的音频\0*.wav;*.mp3;*.mp4;*.m4a;*.flac;*.wma\0\0"
        .encode_utf16()
        .collect();
    // 结构体先用默认值初始化，再只填 Win32 必需字段，避免未初始化的保留成员传入系统。
    let mut dialog = OPENFILENAMEW::default();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = HWND::default();
    // 不指定父窗口时由系统拥有对话框；lpstrFile 与 lpstrFilter 在同步调用结束前始终有效。
    dialog.lpstrFilter = PCWSTR(filter.as_ptr());
    dialog.lpstrFile = PWSTR(file.as_mut_ptr());
    dialog.nMaxFile = file.len() as u32;
    // 这些标志要求路径和文件必须存在，避免把无效路径交给后续解码器。
    dialog.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
    // SAFETY: `dialog` 的缓冲区和筛选字符串在调用期间保持有效，Win32 仅写入给定 UTF-16 缓冲区。
    if !unsafe { GetOpenFileNameW(&mut dialog).as_bool() } {
        return None;
    }
    let len = file.iter().position(|v| *v == 0).unwrap_or(file.len());
    // API 返回的缓冲区是以 NUL 结尾的 UTF-16 字符串，转换前截断到首个 NUL。
    Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
}

/// 使用原生文件选择器选择图标文件。
#[allow(clippy::field_reassign_with_default)]
pub(super) fn select_icon_file() -> Option<PathBuf> {
    // 图标选择器沿用同一缓冲区协议，但额外提供“所有文件”作为用户找不到常见扩展名时的兜底。
    let mut file = [0u16; 32_768];
    // 图标筛选器同样以双 NUL 结尾；Vec 的所有权持续到同步 Win32 调用返回。
    let filter: Vec<u16> = "图标文件\0*.ico;*.png;*.jpg;*.jpeg\0所有文件\0*.*\0\0"
        .encode_utf16()
        .collect();
    let mut dialog = OPENFILENAMEW::default();
    // 与音频选择器相同，默认结构体只在这里补齐调用所需的指针、长度和标志。
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = HWND::default();
    dialog.lpstrFilter = PCWSTR(filter.as_ptr());
    dialog.lpstrFile = PWSTR(file.as_mut_ptr());
    dialog.nMaxFile = file.len() as u32;
    // 与音频选择器保持相同的存在性约束，返回值可直接交给后续资源加载逻辑。
    dialog.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
    // SAFETY: 同上；对话框同步返回，`file` 与 `filter` 在调用结束前不会释放。
    if !unsafe { GetOpenFileNameW(&mut dialog).as_bool() } {
        return None;
    }
    let len = file.iter().position(|v| *v == 0).unwrap_or(file.len());
    // 只读取第一个 NUL 之前的路径，避免把缓冲区剩余的零填充误当成文件名。
    Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
}
