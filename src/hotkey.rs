//! 多种全局热键模式：系统热键、低级钩子、关闭。回调只投递消息，不做重活。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, HWND_MESSAGE, MSG, PostMessageW, RegisterClassExW, SetTimer, TranslateMessage,
    UnregisterClassW, WINDOW_EX_STYLE, WM_HOTKEY, WNDCLASSEXW, WS_OVERLAPPED,
};
use windows::core::w;

use crate::config::Hotkey;

const WM_STOP: u32 = 0x0400 + 33;
/// 音频条目绑定的 id 起点，实际 id 为 `ENTRY_ID_BASE + 条目下标`。
pub const ENTRY_ID_BASE: i32 = 1000;

/// 热键捕获方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyMode {
    /// RegisterHotKey：不监听普通输入，支持单次/循环/切换触发。
    System,
}

/// 一条注册失败的记录，供 UI 提示「这个组合被别的程序占用」。
#[derive(Clone, Debug)]
pub struct BindingFailure {
    /// 失败的绑定 id。
    pub id: i32,
    /// 用户可见的原因。
    pub reason: String,
}

/// 后台系统热键服务：支持多条绑定，按 id 分发。
pub struct HotkeyServer {
    /// 期望的绑定表；热键线程按它做增删同步。
    desired: Arc<Mutex<BTreeMap<i32, Hotkey>>>,
    /// 唤醒热键线程重新同步。
    wake: Arc<AtomicU64>,
    /// 窗口句柄位；0 表示线程尚未就绪。
    hwnd_bits: Arc<AtomicU64>,
    /// 最近一次注册失败，供 UI 查询。
    failures: Arc<Mutex<Vec<BindingFailure>>>,
}

impl HotkeyServer {
    /// 启动热键线程；`on_hotkey(id)` 在热键线程回调，调用方自行 marshal 到 UI 线程。
    pub fn start(
        mode: HotkeyMode,
        initial: BTreeMap<i32, Hotkey>,
        on_hotkey: Arc<dyn Fn(i32) + Send + Sync>,
    ) -> Self {
        let desired = Arc::new(Mutex::new(initial));
        let wake = Arc::new(AtomicU64::new(0));
        let hwnd_bits = Arc::new(AtomicU64::new(0));
        let failures = Arc::new(Mutex::new(Vec::new()));
        if mode == HotkeyMode::System {
            let desired_t = desired.clone();
            let wake_t = wake.clone();
            let hwnd_t = hwnd_bits.clone();
            let failures_t = failures.clone();
            thread::Builder::new()
                .name("hotkey".into())
                .spawn(move || hotkey_thread(desired_t, wake_t, hwnd_t, failures_t, on_hotkey))
                .ok();
        }
        Self {
            desired,
            wake,
            hwnd_bits,
            failures,
        }
    }

    /// 替换整张绑定表并唤醒同步。
    pub fn set_bindings(&self, bindings: BTreeMap<i32, Hotkey>) {
        if let Ok(mut desired) = self.desired.lock() {
            *desired = bindings;
        }
        self.notify();
    }

    /// 唤醒热键线程立即重新同步。
    fn notify(&self) {
        self.wake.fetch_add(1, Ordering::SeqCst);
        let bits = self.hwnd_bits.load(Ordering::SeqCst);
        if bits != 0 {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(bits as *mut _)),
                    WM_STOP + 1,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }

    /// 取最近一次同步的注册失败列表；调用后可清空。
    pub fn take_failures(&self) -> Vec<BindingFailure> {
        self.failures
            .lock()
            .map(|mut failures| std::mem::take(&mut *failures))
            .unwrap_or_default()
    }
}

impl Drop for HotkeyServer {
    fn drop(&mut self) {
        let bits = self.hwnd_bits.load(Ordering::SeqCst);
        if bits != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(bits as *mut _)), WM_STOP, WPARAM(0), LPARAM(0));
            }
        }
    }
}

fn pack(hk: Hotkey) -> u64 {
    (u64::from(hk.mods) << 32) | u64::from(hk.vk)
}

fn hotkey_thread(
    desired: Arc<Mutex<BTreeMap<i32, Hotkey>>>,
    wake: Arc<AtomicU64>,
    hwnd_bits: Arc<AtomicU64>,
    failures: Arc<Mutex<Vec<BindingFailure>>>,
    on_hotkey: Arc<dyn Fn(i32) + Send + Sync>,
) {
    unsafe {
        let class = w!("ProcessMicHotkey");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: class,
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            tracing::warn!("RegisterClassEx hotkey failed");
            return;
        }
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("ProcessMicHotkey"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(_) => {
                tracing::warn!("CreateWindowEx hotkey failed");
                let _ = UnregisterClassW(class, None);
                return;
            }
        };
        hwnd_bits.store(hwnd.0 as u64, Ordering::SeqCst);
        // 200 ms 定时器兜底：即使没有消息也能发现绑定表变化。
        let _ = SetTimer(Some(hwnd), 1, 200, None);

        let mut live: BTreeMap<i32, u64> = BTreeMap::new();
        sync(hwnd, &desired, &mut live, &failures);
        let mut seen_wake = wake.load(Ordering::SeqCst);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_STOP {
                break;
            }
            if msg.message == WM_HOTKEY {
                on_hotkey(msg.wParam.0 as i32);
            }
            let now_wake = wake.load(Ordering::SeqCst);
            if now_wake != seen_wake {
                seen_wake = now_wake;
                sync(hwnd, &desired, &mut live, &failures);
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for id in live.keys() {
            let _ = UnregisterHotKey(Some(hwnd), *id);
        }
        let _ = DestroyWindow(hwnd);
        let _ = UnregisterClassW(class, None);
        hwnd_bits.store(0, Ordering::SeqCst);
    }
}

/// 把实际注册表同步成期望绑定表：先删后加，并记录失败原因。
fn sync(
    hwnd: HWND,
    desired: &Arc<Mutex<BTreeMap<i32, Hotkey>>>,
    live: &mut BTreeMap<i32, u64>,
    failures: &Arc<Mutex<Vec<BindingFailure>>>,
) {
    let wanted: BTreeMap<i32, u64> = desired
        .lock()
        .map(|bindings| bindings.iter().map(|(id, hk)| (*id, pack(*hk))).collect())
        .unwrap_or_default();
    let mut problems = Vec::new();

    // 撤掉不再需要或已改键的。
    let stale: Vec<i32> = live
        .iter()
        .filter(|(id, packed)| wanted.get(id) != Some(*packed))
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
        live.remove(&id);
    }

    // 注册新增或变更的。
    for (id, packed) in &wanted {
        if live.get(id) == Some(packed) {
            continue;
        }
        let hk = Hotkey {
            mods: (packed >> 32) as u32,
            vk: *packed as u32,
        };
        match unsafe {
            RegisterHotKey(
                Some(hwnd),
                *id,
                HOT_KEY_MODIFIERS(hk.mods | MOD_NOREPEAT.0),
                hk.vk,
            )
        } {
            Ok(()) => {
                live.insert(*id, *packed);
            }
            Err(_) => {
                tracing::warn!(
                    "RegisterHotKey failed id={id} mods={} vk={}",
                    hk.mods,
                    hk.vk
                );
                problems.push(BindingFailure {
                    id: *id,
                    reason: if hk.mods == 0 {
                        "该按键被其它程序占用，或不允许单独使用（会拦截正常打字）".into()
                    } else {
                        "该组合已被其它程序占用".into()
                    },
                });
            }
        }
    }

    if let Ok(mut slot) = failures.lock() {
        *slot = problems;
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
