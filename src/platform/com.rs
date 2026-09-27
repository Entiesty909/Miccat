//! COM 公寓初始化，供进程、音频和引擎线程共享。

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

use crate::error::Error;

/// 当前线程的 COM 作用域。
pub(crate) struct ComScope {
    uninit: bool,
}

/// 在当前线程进入 COM MTA；已有 STA 时保留现有公寓。
pub(crate) fn init_mta() -> Result<ComScope, Error> {
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if hr.is_ok() {
        Ok(ComScope { uninit: true })
    } else if hr == RPC_E_CHANGED_MODE {
        Ok(ComScope { uninit: false })
    } else {
        Err(Error::Com(windows::core::Error::from_hresult(hr)))
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.uninit {
            unsafe { CoUninitialize() };
        }
    }
}
