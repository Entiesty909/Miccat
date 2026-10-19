//! 读取工作区并恢复独立窗口位置。

use super::*;
use crate::config::WindowPosition;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};

/// 首选主窗口尺寸（逻辑像素）。
const WINDOW_W: f32 = 1080.0;
const WINDOW_H: f32 = 560.0;

/// 把主窗口尺寸收窄到显示器工作区内。
pub(super) fn fit_window_to_screen(ui: &MainWindow) {
    let scale = ui.window().scale_factor().max(1.0);
    let area = primary_work_area().unwrap_or(RECT {
        left: 0,
        top: 0,
        right: (WINDOW_W * scale) as i32,
        bottom: (WINDOW_H * scale) as i32,
    });
    let available_w = (area.right - area.left) as f32 / scale;
    let available_h = (area.bottom - area.top) as f32 / scale;
    let width = WINDOW_W.min(available_w - 12.0).max(900.0);
    let height = WINDOW_H.min(available_h - 24.0).max(520.0);
    ui.window().set_size(slint::LogicalSize::new(width, height));
}

/// 应用上次保存的物理屏幕坐标；无记录时由调用方执行首次居中。
pub(super) fn restore_position(window: &slint::Window, saved: Option<WindowPosition>) {
    if let Some(position) = saved {
        window.set_position(slint::PhysicalPosition::new(position.x, position.y));
    }
}
/// 在窗口尺寸稳定后按外框尺寸居中到主显示器工作区。
pub(super) fn center_window_on_screen(window: &slint::Window) {
    let Some(area) = primary_work_area() else {
        return;
    };
    let size = window.size();
    let width = (area.right - area.left).max(0);
    let height = (area.bottom - area.top).max(0);
    let x = area.left + (width - size.width as i32).max(0) / 2;
    let y = area.top + (height - size.height as i32).max(0) / 2;
    window.set_position(slint::PhysicalPosition::new(x, y));
}

/// 以外框坐标保存窗口左上角位置。
pub(super) fn saved_position(window: &slint::Window) -> WindowPosition {
    let position = window.position();
    WindowPosition {
        x: position.x,
        y: position.y,
    }
}

/// 返回主显示器工作区的物理坐标。
fn primary_work_area() -> Option<RECT> {
    let mut area = RECT::default();
    let result = unsafe {
        // SAFETY: area 是可写RECT，系统调用在返回前填充该结构。
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut area as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    (result.is_ok() && area.right > area.left && area.bottom > area.top).then_some(area)
}
