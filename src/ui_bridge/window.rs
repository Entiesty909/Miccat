//! 适配 Slint 主窗口尺寸并读取工作区。

use super::*;

/// 首选窗口尺寸（逻辑像素）。
const WINDOW_W: f32 = 1080.0;
const WINDOW_H: f32 = 560.0;

/// 把窗口尺寸按逻辑像素收窄到显示器工作区内。
///
/// 高 DPI 小屏如果直接使用默认尺寸，底部状态栏会落到屏幕外。
pub(super) fn fit_window_to_screen(ui: &MainWindow) {
    let scale = ui.window().scale_factor().max(1.0);
    let (work_w, work_h) =
        primary_work_area().unwrap_or(((WINDOW_W * scale) as u32, (WINDOW_H * scale) as u32));
    let available_w = work_w as f32 / scale;
    let available_h = work_h as f32 / scale;
    let width = WINDOW_W.min(available_w - 12.0).max(900.0);
    let height = WINDOW_H.min(available_h - 24.0).max(520.0);
    ui.window().set_size(slint::LogicalSize::new(width, height));
}

/// 主显示器工作区（物理像素）。取不到就返回 None。
fn primary_work_area() -> Option<(u32, u32)> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut rc = RECT::default();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rc as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_ok() && rc.right > rc.left && rc.bottom > rc.top {
        Some(((rc.right - rc.left) as u32, (rc.bottom - rc.top) as u32))
    } else {
        None
    }
}
