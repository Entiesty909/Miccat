//! 激活并定位本进程的独立顶层窗口。

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, HWND_TOP,
    SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOSIZE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetForegroundWindow, SetWindowPos, SystemParametersInfoW,
};
use windows::core::BOOL;

/// 按当前进程和窗口标题激活独立窗口。
pub(crate) fn activate_window(title: &str) -> Result<(), String> {
    let hwnd = find_current_window(title).ok_or_else(|| format!("找不到本进程窗口：{title}"))?;
    let activated = unsafe {
        // SAFETY: hwnd 来自当前进程的同步 EnumWindows 枚举。
        // 使用当前进程已经找到的顶层句柄，不创建新窗口或改变标题匹配状态。
        SetForegroundWindow(hwnd).as_bool()
    };
    if activated {
        Ok(())
    } else {
        Err(format!("无法激活窗口：{title}"))
    }
}
/// 把首次显示的独立窗口居中到主显示器工作区。
pub(crate) fn center_window_on_work_area(title: &str) -> Result<(), String> {
    let hwnd = find_current_window(title).ok_or_else(|| format!("找不到本进程窗口：{title}"))?;
    let mut window_rect = RECT::default();
    let mut work_area = RECT::default();
    unsafe {
        // SAFETY: 两个RECT都是可写缓冲区，hwnd来自当前进程的同步窗口枚举。
        GetWindowRect(hwnd, &mut window_rect).map_err(|error| error.to_string())?;
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work_area as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .map_err(|error| error.to_string())?;
        // 以外框尺寸计算中心点，避免客户区偏移导致首次显示不居中。
        let width = window_rect.right - window_rect.left;
        let height = window_rect.bottom - window_rect.top;
        let x = work_area.left + ((work_area.right - work_area.left) - width) / 2;
        let y = work_area.top + ((work_area.bottom - work_area.top) - height) / 2;
        // 只移动位置；SWP_NOSIZE 保持原尺寸，SWP_NOACTIVATE 避免居中过程抢焦点。
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 枚举当前进程的顶层窗口，并按完整标题返回第一个匹配句柄。
fn find_current_window(title: &str) -> Option<HWND> {
    // 搜索状态只在同步 EnumWindows 调用期间借用，回调结束后再读取结果。
    let mut search = WindowSearch {
        // 仅接受属于当前进程的窗口，避免标题相同时误选其他进程窗口。
        process_id: std::process::id(),
        // UTF-16 标题用于与 Win32 返回的窗口文本直接比较。
        title: title.encode_utf16().collect(),
        // 回调找到首个匹配窗口后写入并终止枚举。
        found: None,
    };
    let result = unsafe {
        // SAFETY: LPARAM 指向同步 EnumWindows 调用期间有效的栈对象。
        EnumWindows(
            Some(find_window_callback),
            LPARAM(&mut search as *mut WindowSearch as isize),
        )
    };
    if search.found.is_some() {
        return search.found;
    }
    // 枚举失败与“没有匹配窗口”都返回 None；调用方统一呈现找不到窗口的错误。
    result.ok()?;
    None
}

/// EnumWindows 回调共享的筛选条件与结果槽；生命周期限定在一次同步枚举内。
struct WindowSearch {
    /// 目标进程 ID，避免标题相同时误选其他进程窗口。
    process_id: u32,
    /// 目标窗口标题的 UTF-16 编码，不含额外的 NUL 哨兵。
    title: Vec<u16>,
    /// 找到的窗口句柄；只保留首个完整标题匹配项。
    found: Option<HWND>,
}

/// 检查窗口所属进程和标题；返回 false 终止同步枚举。
unsafe extern "system" fn find_window_callback(hwnd: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: EnumWindows 参数由 find_current_window 指向有效 WindowSearch，且回调同步执行。
    let search = unsafe { &mut *(parameter.0 as *mut WindowSearch) };
    let mut process_id = 0u32;
    unsafe {
        // SAFETY: hwnd 是 EnumWindows 返回的有效窗口，输出槽在当前调用栈有效。
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }
    if process_id != search.process_id {
        return true.into();
    }

    let mut title = [0u16; 256];
    // SAFETY: title 是可写缓冲区，hwnd 是 EnumWindows 返回的有效窗口。
    let length = unsafe { GetWindowTextW(hwnd, &mut title) };
    if length > 0 && title[..length as usize] == search.title {
        // 保存句柄后立即停止枚举，避免后续同名窗口覆盖结果。
        search.found = Some(hwnd);
        return false.into();
    }
    true.into()
}

/// 验证窗口枚举、定位和居中逻辑的 Win32 回归测试。
#[cfg(test)]
mod tests {
    use super::{center_window_on_work_area, find_current_window};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetWindowRect, SPI_GETWORKAREA,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW, WINDOW_EX_STYLE, WINDOW_STYLE,
        WS_POPUP, WS_VISIBLE,
    };
    use windows::core::{PCWSTR, w};

    /// 由测试创建的窗口句柄；离开作用域时负责销毁，避免桌面对象泄漏。
    struct TestWindow(
        /// Win32 顶层窗口句柄；由 Drop 唯一销毁。
        HWND,
    );

    /// 释放测试窗口的唯一拥有者句柄。
    impl Drop for TestWindow {
        /// 测试结束时销毁窗口，确保后续测试不会看到残留标题。
        fn drop(&mut self) {
            unsafe {
                // SAFETY: 测试窗口由本测试创建且只在此处销毁一次。
                let _ = DestroyWindow(self.0);
            }
        }
    }

    /// 确认窗口查找只返回当前进程中标题完全匹配的窗口。
    #[test]
    fn enumerates_current_process_window_by_title() -> windows::core::Result<()> {
        let title = format!("Miccat window enum test {}", std::process::id());
        let wide_title = title.encode_utf16().chain([0]).collect::<Vec<_>>();
        let hwnd = unsafe {
            // SAFETY: STATIC 是系统窗口类，标题缓冲区在同步创建调用期间有效。
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                PCWSTR(wide_title.as_ptr()),
                WINDOW_STYLE(WS_POPUP.0 | WS_VISIBLE.0),
                -5000,
                -5000,
                200,
                120,
                None,
                None,
                None,
                None,
            )?
        };
        let window = TestWindow(hwnd);

        assert_eq!(
            find_current_window(&title).map(|found| found.0),
            Some(window.0.0)
        );
        Ok(())
    }
    /// 确认首次显示的窗口外框会落在主显示器工作区中心。
    #[test]
    fn centers_outer_window_inside_primary_work_area() -> windows::core::Result<()> {
        let title = format!("Miccat window center test {}", std::process::id());
        let wide_title = title.encode_utf16().chain([0]).collect::<Vec<_>>();
        let hwnd = unsafe {
            // SAFETY: STATIC 是系统窗口类，标题缓冲区在同步创建调用期间有效。
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                PCWSTR(wide_title.as_ptr()),
                WINDOW_STYLE(WS_POPUP.0 | WS_VISIBLE.0),
                -5000,
                -5000,
                300,
                200,
                None,
                None,
                None,
                None,
            )?
        };
        let _window = TestWindow(hwnd);
        center_window_on_work_area(&title)
            .unwrap_or_else(|error| panic!("center test window: {error}"));

        let mut window_rect = RECT::default();
        let mut work_area = RECT::default();
        unsafe {
            // SAFETY: hwnd 仍由测试窗口拥有，window_rect 是可写输出缓冲区。
            GetWindowRect(hwnd, &mut window_rect)?;
            // SAFETY: work_area 是可写RECT，系统调用在返回前填充该结构。
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work_area as *mut RECT as *mut core::ffi::c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )?;
        }
        assert!(
            ((window_rect.left + window_rect.right) - (work_area.left + work_area.right)).abs()
                <= 1
        );
        assert!(
            ((window_rect.top + window_rect.bottom) - (work_area.top + work_area.bottom)).abs()
                <= 1
        );
        Ok(())
    }
}
