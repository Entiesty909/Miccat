//! Win32 剪贴板文本写入。

use std::mem::size_of;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
// CF_UNICODETEXT 是 CLIPBOARD_FORMAT(u16)，定义在 Ole 模块。
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// 打开剪贴板失败时的重试次数。
const OPEN_RETRIES: usize = 5;

/// 每次重试前的等待，避免与占用剪贴板的进程死磕。
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

/// 写入失败时对用户可见的短句。
const WRITE_FAILED: &str = "无法写入剪贴板";

/// 把文本写入系统剪贴板；失败返回中文错误短句。
pub(crate) fn set_text(text: &str) -> Result<(), String> {
    if !open_with_retry() {
        return Err(WRITE_FAILED.into());
    }
    let result = write_locked(text);
    // SAFETY: 剪贴板由上面的 OpenClipboard 成功打开，此处配对关闭；
    // 忽略返回值，失败也无法再补救。
    let _ = unsafe { CloseClipboard() };
    result
}

/// 尝试打开剪贴板；被其他进程占用时短暂重试。
fn open_with_retry() -> bool {
    for attempt in 0..OPEN_RETRIES {
        // SAFETY: 传 None 表示把剪贴板所有权挂到当前任务，无额外前置条件；
        // 成功即持有所有权，须由调用方 CloseClipboard 释放。
        if unsafe { OpenClipboard(None) }.is_ok() {
            return true;
        }
        if attempt + 1 < OPEN_RETRIES {
            std::thread::sleep(OPEN_RETRY_DELAY);
        }
    }
    false
}

/// 在已打开的剪贴板上写入 UTF-16 文本。
fn write_locked(text: &str) -> Result<(), String> {
    // SAFETY: 剪贴板已由调用方打开并持有所有权；清空后即可放置新数据。
    if unsafe { EmptyClipboard() }.is_err() {
        return Err(WRITE_FAILED.into());
    }
    // UTF-16 编码并补结尾 0，剪贴板要求 NUL 终止。
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let bytes = utf16.len() * size_of::<u16>();
    // SAFETY: GMEM_MOVEABLE 请求可移动全局内存，bytes 为总字节数；
    // 返回的 HGLOBAL 归本函数所有，未交给系统前必须自行释放。
    let hmem = match unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) } {
        Ok(handle) => handle,
        Err(_) => return Err(WRITE_FAILED.into()),
    };
    if !copy_into_global(hmem, &utf16) {
        release(hmem);
        return Err(WRITE_FAILED.into());
    }
    // SAFETY: hmem 是刚分配并写入完毕的全局内存；SetClipboardData 成功后
    // 内存所有权移交系统，此后不得再 GlobalFree。
    let set = unsafe { SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hmem.0))) };
    if set.is_err() {
        release(hmem);
        return Err(WRITE_FAILED.into());
    }
    Ok(())
}

/// 把 UTF-16 数据拷进全局内存；失败返回 `false`。
fn copy_into_global(hmem: HGLOBAL, data: &[u16]) -> bool {
    // SAFETY: hmem 由 GlobalAlloc 返回且尚未释放；GlobalLock 返回可写首地址，
    // 须与 GlobalUnlock 成对出现。
    let ptr = unsafe { GlobalLock(hmem) } as *mut u16;
    if ptr.is_null() {
        return false;
    }
    // SAFETY: ptr 指向至少 data.len() 个 u16 的可写区域，源与目标不重叠。
    unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len()) };
    // SAFETY: ptr 来自上面的 GlobalLock，写入已结束，此处配对解锁。
    let _ = unsafe { GlobalUnlock(hmem) };
    true
}

/// 释放尚未交给系统的全局内存。
fn release(hmem: HGLOBAL) {
    // SAFETY: hmem 由 GlobalAlloc 分配且未移交系统；释放后不再使用该句柄。
    // GlobalFree 成功时返回空句柄，windows-rs 会当作错误，故忽略结果。
    let _ = unsafe { GlobalFree(Some(hmem)) };
}
