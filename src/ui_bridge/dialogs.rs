//! 使用原生 Windows 对话框选择音频文件。

use std::path::PathBuf;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::core::{PCWSTR, PWSTR};

/// 使用 Windows 原生文件选择器选择音频文件。
#[allow(clippy::field_reassign_with_default)]
pub(super) fn select_audio_file() -> Option<PathBuf> {
    let mut file = [0u16; 32_768];
    let filter: Vec<u16> = "支持的音频\0*.wav;*.mp3;*.mp4;*.m4a;*.flac;*.wma\0\0"
        .encode_utf16()
        .collect();
    let mut dialog = OPENFILENAMEW::default();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = HWND::default();
    dialog.lpstrFilter = PCWSTR(filter.as_ptr());
    dialog.lpstrFile = PWSTR(file.as_mut_ptr());
    dialog.nMaxFile = file.len() as u32;
    dialog.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
    if !unsafe { GetOpenFileNameW(&mut dialog).as_bool() } {
        return None;
    }
    let len = file.iter().position(|v| *v == 0).unwrap_or(file.len());
    Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
}
