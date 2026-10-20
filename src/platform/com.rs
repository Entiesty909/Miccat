//! COM 公寓初始化，供进程、音频和引擎线程共享；成功初始化的线程必须在退出前对称反初始化。

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

use crate::error::Error;

/// 当前线程的 COM 作用域；析构时仅撤销本作用域实际建立的 MTA 初始化。
pub(crate) struct ComScope {
    /// 仅当本实例成功调用了 `CoInitializeEx` 时为真，避免撤销调用方已有的公寓。
    uninit: bool,
}

/// 在当前线程进入 COM MTA；已有 STA 时保留现有公寓。
pub(crate) fn init_mta() -> Result<ComScope, Error> {
    // SAFETY: CoInitializeEx 只作用于当前线程；返回成功时由 ComScope 在同一线程配对调用 CoUninitialize。
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    // S_OK/S_FALSE 都表示初始化请求已被接受，前者/后者都需要一次对称反初始化。
    if hr.is_ok() {
        Ok(ComScope { uninit: true })
    // 线程已经加入 STA 时不能改成 MTA；保留现有 apartment，不取得新的释放责任。
    } else if hr == RPC_E_CHANGED_MODE {
        Ok(ComScope { uninit: false })
    } else {
        Err(Error::Com(windows::core::Error::from_hresult(hr)))
    }
}

/// 在当前作用域结束时撤销本实例建立的 COM 初始化。
impl Drop for ComScope {
    /// 只对成功建立 MTA 的实例调用配对清理，避免影响已有的 STA/MTA 所有权。
    fn drop(&mut self) {
        if self.uninit {
            // SAFETY: 仅在 init_mta 成功建立本线程 COM 初始化时调用，且 Drop 仍运行于该线程。
            unsafe { CoUninitialize() };
        }
    }
}
