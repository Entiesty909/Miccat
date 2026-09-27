//! Windows 命名互斥体，保证单实例运行。

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

/// 单实例互斥体；句柄存活期间保持应用独占。
pub(crate) struct SingleInstance {
    handle: HANDLE,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// 获取命名互斥体；返回 None 表示已有实例。
pub(crate) fn acquire_single_instance() -> Result<Option<SingleInstance>, windows::core::Error> {
    let handle = unsafe { CreateMutexW(None, true, w!("Local\\ProcessMic.SingleInstance"))? };
    if unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Ok(None);
    }
    Ok(Some(SingleInstance { handle }))
}
