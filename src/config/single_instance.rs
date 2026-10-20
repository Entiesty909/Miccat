//! Windows 命名互斥体，保证单实例运行。

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE, SetLastError,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::{PCWSTR, w};

/// 单实例互斥体；句柄存活期间保持应用独占。
pub(crate) struct SingleInstance {
    // 结构体是唯一的句柄所有者；不实现 Clone，避免多个值重复关闭同一内核对象。
    /// CreateMutexW 返回的内核句柄；实例存活期间不得复制或提前关闭。
    handle: HANDLE,
}

/// 释放互斥体句柄，使进程退出或对象销毁后可再次启动。
impl Drop for SingleInstance {
    /// 关闭 Win32 句柄；句柄所有权始终由本结构持有。
    fn drop(&mut self) {
        // SAFETY: self.handle 由 CreateMutexW 返回且只由本结构持有，关闭后不再访问。
        // Drop 是释放互斥体占用的唯一出口；失败也不能继续持有已失效句柄。
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// 获取命名互斥体；返回 None 表示已有实例。
pub(crate) fn acquire_single_instance() -> Result<Option<SingleInstance>, windows::core::Error> {
    // 使用 Local 命名空间只限制当前用户会话，避免不同会话之间误相互阻塞。
    acquire_mutex(w!("Local\\Miccat.SingleInstance"))
}

/// 通过命名互斥体执行原子占用检查，返回的句柄维持占用生命周期。
fn acquire_mutex(name: PCWSTR) -> Result<Option<SingleInstance>, windows::core::Error> {
    // 先清空线程错误槽，再创建互斥体，才能把本次调用的“已存在”结果与旧错误区分开。
    // SAFETY: SetLastError 只写入当前线程的Win32错误槽。
    unsafe { SetLastError(ERROR_SUCCESS) };
    // SAFETY: name 在同步调用期间有效；返回句柄由当前进程创建并在Drop时关闭。
    let handle = unsafe { CreateMutexW(None, true, name)? };
    // CreateMutexW 的句柄从这里起归当前 SingleInstance 所有，直到 Drop 关闭。
    // SAFETY: GetLastError 读取当前线程的Win32错误槽。
    if unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
        // ERROR_ALREADY_EXISTS 说明本进程未取得占用；临时句柄必须立即关闭，避免泄漏。
        // SAFETY: 句柄由本次 CreateMutexW 调用返回且未转移所有权。
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Ok(None);
    }
    // 成功路径把句柄交给 RAII 对象，调用方只需保留对象即可维持单实例锁。
    Ok(Some(SingleInstance { handle }))
}

#[cfg(test)]
mod tests {
    use super::acquire_mutex;
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, SetLastError};
    use windows::core::PCWSTR;

    /// 预置过期的错误码后仍应成功创建首个互斥体。
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
