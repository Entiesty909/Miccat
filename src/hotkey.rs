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

/// 私有窗口消息，要求热键线程退出消息泵并清理注册。
///
/// `WM_STOP + 1` 复用同一私有消息区间，仅用于唤醒线程重新读取绑定表。
const WM_STOP: u32 = 0x0400 + 33;
/// 音频条目绑定的 id 起点，实际 id 为 `ENTRY_ID_BASE + 条目下标`。
pub const ENTRY_ID_BASE: i32 = 1000;

/// 热键捕获方式；模式决定是否创建拥有消息泵的专用线程。
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

/// 管理期望绑定、Win32 隐藏窗口和线程间唤醒状态。
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
            // 只有系统模式需要消息窗口；其它模式保留同一服务接口但不启动线程。
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
        // 复制表而不是逐项修改，保证同步线程看到的是一次完整替换。
        if let Ok(mut desired) = self.desired.lock() {
            *desired = bindings;
        }
        self.notify();
    }

    /// 唤醒热键线程立即重新同步。
    fn notify(&self) {
        self.wake.fetch_add(1, Ordering::SeqCst);
        // 原子计数兼作版本号；消息负责低延迟唤醒，定时器负责兜底。
        let bits = self.hwnd_bits.load(Ordering::SeqCst);
        if bits != 0 {
            // WM_STOP + 1 不退出线程，只让 GetMessageW 尽快返回并触发 sync。
            // SAFETY: 句柄来自本线程创建的消息窗口；失效句柄只会让投递失败，不解引用用户内存。
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

/// 丢弃服务时通知隐藏窗口退出；线程随后撤销所有仍存活的注册。
impl Drop for HotkeyServer {
    /// 发送停止消息，不直接 join，避免调用方被 Win32 消息泵生命周期卡住。
    fn drop(&mut self) {
        let bits = self.hwnd_bits.load(Ordering::SeqCst);
        if bits != 0 {
            // SAFETY: 仅向原子槽中观察到的消息窗口投递退出消息，不跨线程访问窗口状态。
            unsafe {
                let _ = PostMessageW(Some(HWND(bits as *mut _)), WM_STOP, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 将修饰键与虚拟键压成可比较值，用于检测绑定是否真正变化。
fn pack(hk: Hotkey) -> u64 {
    (u64::from(hk.mods) << 32) | u64::from(hk.vk)
}

/// 在专用线程创建消息窗口、同步 RegisterHotKey，并把 WM_HOTKEY 转交给调用方。
fn hotkey_thread(
    desired: Arc<Mutex<BTreeMap<i32, Hotkey>>>,
    wake: Arc<AtomicU64>,
    hwnd_bits: Arc<AtomicU64>,
    failures: Arc<Mutex<Vec<BindingFailure>>>,
    on_hotkey: Arc<dyn Fn(i32) + Send + Sync>,
) {
    // 热键线程拥有隐藏窗口，所有 RegisterHotKey/UnregisterHotKey 调用保持在线程内。
    // SAFETY: 以下 Win32 调用均在线程内串行执行，窗口句柄和消息参数由系统维护。
    unsafe {
        let class = w!("MiccatHotkey");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: class,
            ..Default::default()
        };
        // 窗口类注册失败时无法建立消息泵，线程直接退出而不伪造已绑定状态。
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            tracing::warn!("RegisterClassEx hotkey failed");
            return;
        }
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("MiccatHotkey"),
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
        // 句柄写入原子槽后，其他线程才允许通过 PostMessageW 唤醒本线程。
        hwnd_bits.store(hwnd.0 as u64, Ordering::SeqCst);
        // 200 ms 定时器投递 WM_TIMER，作为绑定变更消息丢失时的同步兜底。
        let _ = SetTimer(Some(hwnd), 1, 200, None);

        // live 保存已成功注册的 packed 值，避免重复调用 Win32 注册接口。
        let mut live: BTreeMap<i32, u64> = BTreeMap::new();
        sync(hwnd, &desired, &mut live, &failures);
        let mut seen_wake = wake.load(Ordering::SeqCst);

        let mut msg = MSG::default();
        // WM_HOTKEY 只携带整数 id；真正业务动作由上层回调决定。
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_STOP {
                // 停止消息只由 Drop 发送，退出前仍会撤销 live 中的所有注册。
                break;
            }
            if msg.message == WM_HOTKEY {
                // wParam 是 RegisterHotKey 时传入的 id，不是虚拟键码。
                on_hotkey(msg.wParam.0 as i32);
            }
            let now_wake = wake.load(Ordering::SeqCst);
            if now_wake != seen_wake {
                // 普通唤醒消息和 WM_TIMER 都在这里汇合，按版本号避免重复同步。
                seen_wake = now_wake;
                sync(hwnd, &desired, &mut live, &failures);
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // 消息泵退出前逐个注销，避免窗口销毁后遗留全局热键。
        for id in live.keys() {
            let _ = UnregisterHotKey(Some(hwnd), *id);
        }
        // 销毁隐藏窗口同时终止其定时器；随后注销窗口类，避免下次启动复用旧回调。
        let _ = DestroyWindow(hwnd);
        let _ = UnregisterClassW(class, None);
        hwnd_bits.store(0, Ordering::SeqCst);
    }
}

/// 按 id 对比修饰键/虚拟键，先撤销旧绑定再注册新绑定，保持状态可重试。
/// 把实际注册表同步成期望绑定表：先删后加，并记录失败原因。
fn sync(
    hwnd: HWND,
    desired: &Arc<Mutex<BTreeMap<i32, Hotkey>>>,
    live: &mut BTreeMap<i32, u64>,
    failures: &Arc<Mutex<Vec<BindingFailure>>>,
) {
    // 克隆并压缩期望表后释放锁，注册 Win32 热键期间不阻塞配置写入。
    let wanted: BTreeMap<i32, u64> = desired
        .lock()
        .map(|bindings| bindings.iter().map(|(id, hk)| (*id, pack(*hk))).collect())
        .unwrap_or_default();
    let mut problems = Vec::new();

    // 先撤销删除或改键项，避免同一 id 在系统中短暂指向两个组合。
    let stale: Vec<i32> = live
        .iter()
        .filter(|(id, packed)| wanted.get(id) != Some(*packed))
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        // SAFETY: hwnd 仍由热键线程拥有，注销调用与消息泵在同一线程串行执行。
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
        live.remove(&id);
    }

    // 只向 Win32 注册尚未确认成功的项；失败项不进入 live，下一次同步仍可重试。
    for (id, packed) in &wanted {
        if live.get(id) == Some(packed) {
            continue;
        }
        let hk = Hotkey {
            mods: (packed >> 32) as u32,
            vk: *packed as u32,
        };
        // SAFETY: hwnd 和 id 来自同一线程维护的 live/desired 表，调用不保存 Rust 借用。
        // MOD_NOREPEAT 让一次按住只产生一次 WM_HOTKEY，重复触发由上层策略决定。
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

    // 用一次替换发布本轮结果，读取方不会看到逐项更新的中间状态。
    if let Ok(mut slot) = failures.lock() {
        *slot = problems;
    }
}

/// 隐藏消息窗口的默认过程；业务消息由热键线程显式处理。
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // WM_HOTKEY 和私有唤醒消息已由消息泵处理；其余消息交给系统默认过程。
    // SAFETY: hwnd、消息参数均由系统消息泵提供，默认过程不保存借用数据。
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
