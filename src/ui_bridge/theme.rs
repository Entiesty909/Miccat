//! 主题选择桥接：切换统一调色板，并异步获取淡粉主题图片。

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use slint::private_unstable_api::re_exports::ColorScheme;
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode};

mod composite;

use super::state::AppWindows;
use super::{AppState, MainWindow, Palette, Theme};
use crate::platform;

/// 启动窗口显示前应用持久化主题，保证首帧即使用用户上次选择。
pub(super) fn initialize(ui: &MainWindow, windows: &AppWindows, index: i32) {
    apply_theme_index(ui, windows, index);
}

/// 注册主题选择与图片回写；网络请求不在 Slint 线程中执行。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    let original = Rc::new(RefCell::new(None::<SharedPixelBuffer<Rgba8Pixel>>));
    let resize_timer = Rc::new(Timer::default());
    let generation = Arc::new(AtomicU64::new(0));

    let windows = state.windows.clone();
    let ui_for_image = ui.as_weak();
    let source = original.clone();
    let timer = resize_timer.clone();
    let version = generation.clone();
    ui.on_theme_image_ready(move |image| {
        *source.borrow_mut() = image.to_rgba8();
        if let Some(ui) = ui_for_image.upgrade() {
            ui.global::<Theme>()
                .set_pink_background_image(image.clone());
            schedule_pink_composite(&ui, &source, &timer, &version);
        }
        windows.add_source.set_pink_background_image(image.clone());
        windows.audio_edit.set_pink_background_image(image);
    });

    let ui_weak = ui.as_weak();
    ui.on_theme_composite_ready(move |image, width, height| {
        let Some(ui) = ui_weak.upgrade() else { return };
        if ui.global::<Theme>().get_theme_index() != 2
            || logical_window_size(&ui) != (width as u32, height as u32)
        {
            return;
        }
        let theme = ui.global::<Theme>();
        theme.set_pink_composite_image(image);
        theme.set_pink_composite_width(width);
        theme.set_pink_composite_height(height);
    });

    let ui_weak = ui.as_weak();
    let source = original.clone();
    let timer = resize_timer.clone();
    let version = generation.clone();
    ui.on_theme_size_changed(move || {
        if let Some(ui) = ui_weak.upgrade() {
            schedule_pink_composite(&ui, &source, &timer, &version);
        }
    });

    let windows = state.windows.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    let ui_weak = ui.as_weak();
    ui.on_theme_selected(move |index| {
        if !(0..=2).contains(&index) {
            return;
        }
        let Some(ui) = ui_weak.upgrade() else { return };
        resize_timer.stop();
        generation.fetch_add(1, Ordering::Relaxed);
        *original.borrow_mut() = None;
        app_config.borrow_mut().theme_index = index;
        save_queue.schedule();
        apply_theme_index(&ui, &windows, index);
        if index == 2 {
            request_pink_background(&ui);
        }
    });

    if ui.global::<Theme>().get_theme_index() == 2 {
        request_pink_background(ui);
    }
}

/// 更新主窗口和独立窗口的共同主题状态，不改持久化配置。
fn apply_theme_index(ui: &MainWindow, windows: &AppWindows, index: i32) {
    let palette = ui.global::<Palette>();
    palette.set_color_scheme(if index == 1 {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    });
    ui.global::<Theme>().set_theme_index(index);
    windows.add_source.set_theme_index(index);
    windows.audio_edit.set_theme_index(index);
    clear_pink_background(ui, windows);
}

/// 切换预设时立即清除旧图片，避免新主题首帧沿用上一张背景。
fn clear_pink_background(ui: &MainWindow, windows: &AppWindows) {
    let theme = ui.global::<Theme>();
    theme.set_pink_composite_width(0);
    theme.set_pink_composite_height(0);
    theme.set_pink_composite_image(Image::default());
    theme.set_pink_background_image(Image::default());
    windows
        .add_source
        .set_pink_background_image(slint::Image::default());
    windows
        .audio_edit
        .set_pink_background_image(slint::Image::default());
}

/// HTTP 200 无可解码图片时允许额外重试的次数。
const MAX_PINK_BACKGROUND_RETRIES: usize = 3;
/// 主窗口默认尺寸；预裁剪后默认渲染无需逐帧缩放背景图。
const PINK_BACKGROUND_WIDTH: u32 = 1280;
const PINK_BACKGROUND_HEIGHT: u32 = 800;
/// 远程图片的透明像素以淡粉色为底合成，输出始终不透明。
const PINK_BACKGROUND_BASE: [u32; 3] = [255, 240, 246];

/// Slint 长度是逻辑像素，壁纸缓冲尺寸必须按窗口 DPI 逆换算。
fn logical_window_size(ui: &MainWindow) -> (u32, u32) {
    let physical = ui.window().size();
    let scale = ui.window().scale_factor().max(0.1);
    (
        (physical.width as f32 / scale).round() as u32,
        (physical.height as f32 / scale).round() as u32,
    )
}

/// 连续拉伸时只重启短计时器；落稳后在 worker 中生成当前尺寸的单张背景图。
fn schedule_pink_composite(
    ui: &MainWindow,
    source: &Rc<RefCell<Option<SharedPixelBuffer<Rgba8Pixel>>>>,
    timer: &Rc<Timer>,
    generation: &Arc<AtomicU64>,
) {
    if ui.global::<Theme>().get_theme_index() != 2 || source.borrow().is_none() {
        return;
    }
    let ticket = generation.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let source = source.clone();
    let generation = generation.clone();
    let ui_weak = ui.as_weak();
    timer.start(
        TimerMode::SingleShot,
        Duration::from_millis(180),
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let (width, height) = logical_window_size(&ui);
            if width == 0 || height == 0 {
                return;
            }
            let Some(pixels) = source.borrow().as_ref().cloned() else {
                return;
            };
            let ui_weak = ui.as_weak();
            let generation = generation.clone();
            if let Err(error) = thread::Builder::new()
                .name("pink-theme-composite".into())
                .spawn(move || {
                    let composite = composite::render(&pixels, width, height);
                    let _ = slint::invoke_from_event_loop(move || {
                        if generation.load(Ordering::Relaxed) != ticket {
                            return;
                        }
                        let Some(ui) = ui_weak.upgrade() else { return };
                        if ui.global::<Theme>().get_theme_index() == 2
                            && logical_window_size(&ui) == (width, height)
                        {
                            ui.invoke_theme_composite_ready(
                                Image::from_rgba8(composite),
                                width as i32,
                                height as i32,
                            );
                        }
                    });
                })
            {
                tracing::warn!("淡粉背景合成线程启动失败：{error}");
            }
        },
    );
}

/// 后台下载、解码和预裁剪淡粉图片；回写前检查主题仍为粉色。
fn request_pink_background(ui: &MainWindow) {
    let ui_weak = ui.as_weak();
    let spawn_result = thread::Builder::new()
        .name("pink-theme-image".into())
        .spawn(move || {
            let pixels = match load_pink_background_with_retries(
                platform::theme::download_pink_background,
            ) {
                Ok(pixels) => pixels,
                Err(error) => {
                    tracing::warn!("淡粉主题图片获取失败：{error}");
                    return;
                }
            };
            // 解码在后台完成，UI 线程只交换共享像素句柄。
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = ui_weak.upgrade() else { return };
                if ui.global::<Theme>().get_theme_index() != 2 {
                    return;
                }
                ui.invoke_theme_image_ready(Image::from_rgba8(pixels));
            });
        });
    if let Err(error) = spawn_result {
        tracing::warn!("淡粉主题图片线程创建失败：{error}");
    }
}

/// HTTP 非 200、空正文或不可解码正文最多触发三次额外请求。
fn load_pink_background_with_retries(
    mut download: impl FnMut() -> Result<crate::platform::theme::ThemeResponse, String>,
) -> Result<SharedPixelBuffer<Rgba8Pixel>, String> {
    let mut last_error = String::new();
    for attempt in 0..=MAX_PINK_BACKGROUND_RETRIES {
        let response = download()?;
        if response.status_code != 200 {
            last_error = format!("HTTP 状态码 {}", response.status_code);
        } else if response.body.is_empty() {
            last_error = "HTTP 200 响应正文为空".into();
        } else {
            match Image::load_from_data(&response.body, None) {
                Ok(image) => match downsample_image(image) {
                    Ok(pixels) => return Ok(pixels),
                    Err(error) => last_error = error,
                },
                Err(error) => last_error = format!("HTTP 200 正文不是可解码图片：{error}"),
            }
        }
        if attempt < MAX_PINK_BACKGROUND_RETRIES {
            tracing::warn!(
                "淡粉主题图片无效，进行第 {} 次重试：{last_error}",
                attempt + 1
            );
        }
    }
    Err(format!(
        "主题图片连续 {} 次未取得有效图像：{last_error}",
        MAX_PINK_BACKGROUND_RETRIES + 1
    ))
}

/// 裁剪并重采样为默认窗口大小，保留原图颜色并输出不透明像素。
fn downsample_image(image: Image) -> Result<SharedPixelBuffer<Rgba8Pixel>, String> {
    let Some(source) = image.to_rgba8() else {
        return Err("主题图片无法转换为 RGBA 图像".into());
    };
    let source_width = source.width();
    let source_height = source.height();
    if source_width == 0 || source_height == 0 {
        return Err("主题图片尺寸为空".into());
    }
    // 先按窗口宽高比裁剪，保证默认窗口显示时图片不再逐帧 cover 缩放。
    let source_is_wider = source_width as u64 * PINK_BACKGROUND_HEIGHT as u64
        > source_height as u64 * PINK_BACKGROUND_WIDTH as u64;
    let (crop_x, crop_y, crop_width, crop_height) = if source_is_wider {
        let crop_width = ((source_height as u64 * PINK_BACKGROUND_WIDTH as u64
            / PINK_BACKGROUND_HEIGHT as u64) as u32)
            .max(1);
        (
            (source_width - crop_width) / 2,
            0,
            crop_width,
            source_height,
        )
    } else {
        let crop_height = ((source_width as u64 * PINK_BACKGROUND_HEIGHT as u64
            / PINK_BACKGROUND_WIDTH as u64) as u32)
            .max(1);
        (
            0,
            (source_height - crop_height) / 2,
            source_width,
            crop_height,
        )
    };

    let source_bytes = source.as_bytes();
    let mut target =
        SharedPixelBuffer::<Rgba8Pixel>::new(PINK_BACKGROUND_WIDTH, PINK_BACKGROUND_HEIGHT);
    let target_bytes = target.make_mut_bytes();
    for y in 0..PINK_BACKGROUND_HEIGHT {
        let source_y =
            crop_y + (y as u64 * crop_height as u64 / PINK_BACKGROUND_HEIGHT as u64) as u32;
        for x in 0..PINK_BACKGROUND_WIDTH {
            let source_x =
                crop_x + (x as u64 * crop_width as u64 / PINK_BACKGROUND_WIDTH as u64) as u32;
            let source_offset =
                ((source_y as u64 * source_width as u64 + source_x as u64) * 4) as usize;
            let target_offset = ((y * PINK_BACKGROUND_WIDTH + x) * 4) as usize;
            let alpha = source_bytes[source_offset + 3] as u32;
            for channel in 0..3 {
                let source_value = source_bytes[source_offset + channel] as u32;
                target_bytes[target_offset + channel] = if alpha == 255 {
                    source_bytes[source_offset + channel]
                } else {
                    let base_value = PINK_BACKGROUND_BASE[channel];
                    ((source_value * alpha + base_value * (255 - alpha) + 127) / 255) as u8
                };
            }
            target_bytes[target_offset + 3] = 255;
        }
    }
    Ok(target)
}

/// 主题图片接口的异常响应边界测试。
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// HTTP 200 空正文最多请求四次，即初次请求加三次重试。
    #[test]
    fn empty_ok_response_retries_three_times_at_most() {
        let requests = Cell::new(0);
        let result = load_pink_background_with_retries(|| {
            requests.set(requests.get() + 1);
            Ok(crate::platform::theme::ThemeResponse {
                status_code: 200,
                body: Vec::new(),
            })
        });

        assert!(result.is_err());
        assert_eq!(requests.get(), MAX_PINK_BACKGROUND_RETRIES + 1);
    }
    /// 预裁剪输出覆盖默认窗口尺寸且不再把照片淡化到 18%。
    #[test]
    fn background_crop_preserves_opaque_source_color() {
        let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        source.make_mut_bytes().copy_from_slice(&[12, 34, 56, 255]);

        let target = downsample_image(Image::from_rgba8(source))
            .expect("opaque source pixel should be converted");

        assert_eq!(target.width(), PINK_BACKGROUND_WIDTH);
        assert_eq!(target.height(), PINK_BACKGROUND_HEIGHT);
        assert_eq!(&target.as_bytes()[..4], &[12, 34, 56, 255]);
    }
}
