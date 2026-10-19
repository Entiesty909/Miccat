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
        let width = window_rect.right - window_rect.left;
        let height = window_rect.bottom - window_rect.top;
        let x = work_area.left + ((work_area.right - work_area.left) - width) / 2;
        let y = work_area.top + ((work_area.bottom - work_area.top) - height) / 2;
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

fn find_current_window(title: &str) -> Option<HWND> {
    let mut search = WindowSearch {
        process_id: std::process::id(),
        title: title.encode_utf16().collect(),
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
    result.ok()?;
    None
}

struct WindowSearch {
    process_id: u32,
    title: Vec<u16>,
    found: Option<HWND>,
}

unsafe extern "system" fn find_window_callback(hwnd: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: EnumWindows 参数由 find_current_window 指向有效 WindowSearch。
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
        search.found = Some(hwnd);
        return false.into();
    }
    true.into()
}

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

    struct TestWindow(HWND);

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                // SAFETY: 测试窗口由本测试创建且只在此处销毁一次。
                let _ = DestroyWindow(self.0);
            }
        }
    }

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
