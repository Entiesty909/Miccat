//! 用未文档化的 IPolicyConfig 切换系统默认录音设备。失败则忽略。

use std::ffi::c_void;

use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::{GUID, HRESULT, IUnknown, Interface, PCWSTR};

use super::com;
use crate::error::Error;

/// `{870AF99C-171D-4F9E-AF0D-E63DF40C2BC9}`
const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870a_f99c_171d_4f9e_af0d_e63d_f40c_2bc9);

/// 按 COM ABI 排列的 IPolicyConfig vtable 前缀及默认端点设置槽位。
#[repr(C)]
struct PolicyConfigVtbl {
    /// IUnknown 的 QueryInterface、AddRef、Release 三个槽位。
    _iunknown: [*const (); 3],
    /// 未调用的接口方法；占位以保持 SetDefaultEndpoint 的 ABI 偏移正确。
    _unused: [*const (); 10],
    /// 设置设备在指定 role 下的默认端点；role 使用 Windows 的枚举数值。
    set_default_endpoint: unsafe extern "system" fn(*mut c_void, PCWSTR, u32) -> HRESULT,
}

/// 把设备设为三种录音 role 的默认麦。
pub fn set_default_capture(device_id: &str) -> std::result::Result<(), Error> {
    set_default(device_id, &[0, 1, 2])
}

/// 把播放端设为系统默认播放设备（console + multimedia）。
pub fn set_default_render(device_id: &str) -> std::result::Result<(), Error> {
    set_default(device_id, &[0, 1])
}

/// 把播放端设为默认通信设备。
pub fn set_default_communications(device_id: &str) -> std::result::Result<(), Error> {
    set_default(device_id, &[2])
}

/// 恢复之前保存的默认端点（role, id）。
pub fn restore_defaults(saved: &[(u32, String)]) -> std::result::Result<(), Error> {
    for (role, id) in saved {
        set_default(id, &[*role])?;
    }
    Ok(())
}

/// 通过手写的 IPolicyConfig vtable 将设备设为指定系统角色的默认端点。
fn set_default(device_id: &str, roles: &[u32]) -> std::result::Result<(), Error> {
    let _com = com::init_mta()?;
    let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
    // 缓冲区覆盖所有 role 调用的生命周期，传给 COM 的 PCWSTR 因而始终有效。
    // SAFETY: COM 已在当前 MTA 初始化；接口指针来自持有其引用的 IUnknown，调用使用有效的 NUL 结尾 ID。
    unsafe {
        let unk: IUnknown = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)
            .map_err(|_| Error::Cable("无法切换系统默认麦克风。".into()))?;
        // 接口没有 windows-rs 定义，按其 ABI 前缀取 vtable；占位槽位数必须与 IPolicyConfig 顺序一致。
        let this = Interface::as_raw(&unk);
        let vtbl = &**(this as *mut *const PolicyConfigVtbl);
        for role in roles {
            let hr = (vtbl.set_default_endpoint)(this, PCWSTR(wide.as_ptr()), *role);
            if hr.is_err() {
                tracing::warn!("SetDefaultEndpoint role {role} failed: {hr:?}");
                return Err(Error::Cable("无法把虚拟麦设成系统默认麦克风。".into()));
            }
        }
    }
    Ok(())
}
