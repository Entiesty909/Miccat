//! 进程枚举：有窗口或正在出声的进程，去掉系统噪声。

use std::collections::{HashMap, HashSet};
use std::mem::size_of;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, MMDeviceEnumerator, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::Interface;
use windows_core::BOOL;

use crate::error::Error;
use crate::platform::com;

/// 一条可被 UI 选中的进程。
#[derive(Clone, Debug)]
pub struct ProcessInfo {
    /// 进程 ID。
    pub pid: u32,
    /// 可执行文件名，如 `notepad.exe`。
    pub name: String,
    /// 可见顶层窗口标题（没有窗口则为 None）。
    pub title: Option<String>,
    /// 当前是否有 WASAPI 音频会话。
    pub sounding: bool,
}

/// 只保留：有可见窗口，或正在出声。排除自身和系统进程。
pub fn list_processes() -> Result<Vec<ProcessInfo>, Error> {
    // 仅返回用户可辨识且可捕获的来源，避免把系统服务噪声暴露给路由选择器。
    let self_pid = std::process::id();
    let titles = window_titles();
    let sounding = audio_session_pids();
    // 快照句柄只在当前函数内使用；PROCESSENTRY32W 的大小字段是 Win32 枚举前提。
    // SAFETY: 快照由本线程创建，后续仅由同一线程读取并在枚举结束后关闭。
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? };
    if snapshot.is_invalid() {
        return Err(Error::CaptureInit("process snapshot failed"));
    }

    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    // 先按“正在出声”和窗口可见性排序，让当前有音频的来源优先出现；
    // 这只影响列表顺序，不改变进程是否可被选中的判定。
    let mut list = Vec::new();
    // ToolHelp 迭代器的首项和后续项都可能失败；失败时保留已收集的结果。
    unsafe {
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let pid = entry.th32ProcessID;
                // 跳过系统保留 PID 与自身，避免把路由器或内核服务暴露为可选来源。
                if pid > 4 && pid != self_pid {
                    let name = wchar_to_string(&entry.szExeFile);
                    if !name.is_empty() && !is_noise_process(&name) {
                        let title = titles.get(&pid).cloned();
                        let has_audio = sounding.contains(&pid);
                        if title.is_some() || has_audio {
                            list.push(ProcessInfo {
                                pid,
                                name,
                                title,
                                sounding: has_audio,
                            });
                        }
                    }
                }
                // 迭代结束和枚举错误都通过 Err 表示，这里安全地结束当前快照。
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        // 句柄不再使用后立即关闭，避免刷新进程列表时累积内核对象。
        let _ = CloseHandle(snapshot);
    }

    // 排序只读已缓存字段，避免在返回前再次访问进程或窗口句柄。
    list.sort_by(|a, b| {
        b.sounding
            .cmp(&a.sounding)
            .then(b.title.is_some().cmp(&a.title.is_some()))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(list)
}

/// 服务/壳进程，几乎不会作为音频来源。
fn is_noise_process(name: &str) -> bool {
    // 名称匹配故意保持白名单式排除，避免把系统进程误当作用户音频源。
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "svchost.exe"
            | "csrss.exe"
            | "smss.exe"
            | "wininit.exe"
            | "winlogon.exe"
            | "lsass.exe"
            | "services.exe"
            | "fontdrvhost.exe"
            | "dwm.exe"
            | "conhost.exe"
            | "runtimebroker.exe"
            | "searchhost.exe"
            | "searchindexer.exe"
            | "searchfilterhost.exe"
            | "searchprotocolhost.exe"
            | "dllhost.exe"
            | "wmiprvse.exe"
            | "sihost.exe"
            | "taskhostw.exe"
            | "ctfmon.exe"
            | "textinputhost.exe"
            | "applicationframehost.exe"
            | "shellexperiencehost.exe"
            | "startmenuexperiencehost.exe"
            | "securityhealthsystray.exe"
            | "securityhealthservice.exe"
            | "nvcontainer.exe"
            | "nvdisplay.container.exe"
            | "aggregatorhost.exe"
            | "registry"
            | "secure system"
            | "system"
            | "idle"
            | "memory compression"
            | " audiodg.exe"
            | "audiodg.exe"
            | "wudfhost.exe"
            | "spoolsv.exe"
            | "dashost.exe"
            | "lsaiso.exe"
            | "ngciso.exe"
            | "smartscreen.exe"
            | "widgetservice.exe"
            | "widgets.exe"
            | "msedgewebview2.exe"
            | "crashpad_handler.exe"
            | "elevation_service.exe"
    )
}

/// 所有活动渲染设备上的音频会话 PID。失败则空集，不挡窗口进程。
fn audio_session_pids() -> HashSet<u32> {
    // 任一 COM/设备枚举环节失败都只影响“正在出声”标记，窗口进程仍可用。
    let mut pids = HashSet::new();
    // 保留 COM guard 到枚举结束，确保接口释放前当前线程仍处于 MTA apartment。
    let _com = match com::init_mta() {
        Ok(c) => c,
        Err(_) => return pids,
    };
    // SAFETY: COM 对象仅在当前已初始化的 MTA 线程访问，接口引用由 windows-rs 管理。
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(_) => return pids,
            };
        let collection = match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
            Ok(c) => c,
            Err(_) => return pids,
        };
        // 每个活动 render endpoint 都可能有独立会话，必须合并所有 PID。
        // 设备数量读取失败按零处理；窗口来源仍由外层快照正常返回。
        let count = collection.GetCount().unwrap_or(0);
        for i in 0..count {
            let Ok(device) = collection.Item(i) else {
                continue;
            };
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let Ok(sessions) = manager.GetSessionEnumerator() else {
                continue;
            };
            let Ok(n) = sessions.GetCount() else {
                continue;
            };
            // 会话 PID 可能重复，HashSet 同时去重并过滤系统保留 PID。
            for j in 0..n {
                let Ok(ctl) = sessions.GetSession(j) else {
                    continue;
                };
                let ctl: IAudioSessionControl = ctl;
                let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else {
                    continue;
                };
                if let Ok(pid) = ctl2.GetProcessId()
                    && pid > 4
                {
                    pids.insert(pid);
                }
            }
        }
    }
    pids
}

/// 枚举顶层窗口并按 PID 缓存第一个有标题的窗口，供列表展示而非音频判定。
/// 可见、无 owner 的顶层窗口 → PID 标题。每个 PID 只留第一个。
fn window_titles() -> HashMap<u32, String> {
    let mut titles = HashMap::new();
    unsafe {
        // SAFETY: 回调只在同步 EnumWindows 期间使用栈上 titles，lparam 不跨调用保存。
        // EnumWindows 同步调用回调；因此指针只在这次调用期间有效，不跨线程保存。
        let _ = EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut titles as *mut HashMap<u32, String> as isize),
        );
    }
    titles
}

/// EnumWindows 回调。过滤工具窗口和不可见窗口。
// SAFETY: lparam 来自本次同步枚举的 HashMap 可变借用，回调期间未发生别名访问。
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: window_titles 传入的 lparam 指向仍存活的 HashMap，EnumWindows 未返回前不会释放。
    let titles = unsafe { &mut *(lparam.0 as *mut HashMap<u32, String>) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        // owner 窗口通常是对话框或子窗口，不作为独立的进程展示项。
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
        if owner != HWND::default() {
            return true.into();
        }
        // 工具窗口多为托盘或辅助窗口，排除后可避免标题列表出现内部 UI。
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return true.into();
        }
        // 一个 PID 只保留首个窗口，避免多窗口应用在 UI 中重复出现。
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || titles.contains_key(&pid) {
            return true.into();
        }
        let mut buf = [0u16; MAX_PATH as usize];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n > 0 {
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if !title.is_empty() {
                titles.insert(pid, title);
            }
        }
    }
    true.into()
}

/// 截到第一个 NUL 的 UTF-16 → String；Win32 固定数组可能未填满整段缓冲区。
fn wchar_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
