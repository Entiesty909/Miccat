//! Windows 命名互斥体，保证单实例运行。

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE, SetLastError,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::{PCWSTR, w};

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
    acquire_mutex(w!("Local\\Miccat.SingleInstance"))
}

fn acquire_mutex(name: PCWSTR) -> Result<Option<SingleInstance>, windows::core::Error> {
    // SAFETY: SetLastError 只写入当前线程的Win32错误槽。
    unsafe { SetLastError(ERROR_SUCCESS) };
    // SAFETY: name 在同步调用期间有效；返回句柄由当前进程创建并在Drop时关闭。
    let handle = unsafe { CreateMutexW(None, true, name)? };
    // SAFETY: GetLastError 读取当前线程的Win32错误槽。
    if unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: 句柄由本次 CreateMutexW 调用返回且未转移所有权。
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Ok(None);
    }
    Ok(Some(SingleInstance { handle }))
}

#[cfg(test)]
mod tests {
    use super::acquire_mutex;
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, SetLastError};
    use windows::core::PCWSTR;

    #[test]
    fn stale_already_exists_error_does_not_reject_new_mutex() -> windows::core::Result<()> {
        let name = format!("Local\\Miccat.Test.Instance.{}", std::process::id());
        let wide_name = name.encode_utf16().chain([0]).collect::<Vec<_>>();
        // SAFETY: SetLastError 只写入当前测试线程的Win32错误槽。
        unsafe { SetLastError(ERROR_ALREADY_EXISTS) };

        let instance = acquire_mutex(PCWSTR(wide_name.as_ptr()))?;
        assert!(instance.is_some());
        drop(instance);
        Ok(())
    }
}
