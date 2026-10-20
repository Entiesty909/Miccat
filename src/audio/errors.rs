//! WASAPI HRESULT 到领域错误的统一分类。

use crate::error::Error;

/// 将 Process Loopback 的设备失效映射为进程退出。
pub(super) fn classify_capture_error(error: windows::core::Error) -> Error {
    // 取 unsigned 值便于与 Windows SDK 的 HRESULT 常量逐字比较；不改动原始错误对象。
    let code = error.code().0 as u32;
    // 这两个 HRESULT 表示进程音频会话或其设备已经失效，捕获线程应结束而非重试同一会话。
    if code == 0x8889_0004 || code == 0x8889_0013 {
        Error::ProcessNotFound(0)
    } else {
        // 未知 HRESULT 保留原始 COM 细节，避免把格式/权限错误误报成进程退出。
        error.into()
    }
}

/// 将 render 端点失效映射为设备断开。
pub(super) fn classify_render_error(error: windows::core::Error) -> Error {
    // render 端复用相同的失效码，但领域含义是输出设备消失，而不是进程退出。
    let code = error.code().0 as u32;
    // 设备拔出或音频资源失效时必须让上层进入设备断开状态。
    if code == 0x8889_0004 || code == 0x8889_0013 {
        Error::DeviceNotFound
    } else {
        // 未命中的错误继续向上层传播，调用方才能决定重试或展示具体原因。
        error.into()
    }
}
