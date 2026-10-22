//! 在后台把淡粉主题遮罩预合成到主窗口壁纸，保持顶栏、侧栏和底栏原有颜色。

use slint::{Rgba8Pixel, SharedPixelBuffer};

use super::PINK_BACKGROUND_BASE;

/// 将不透明原图裁剪到窗口尺寸，随后只对页面矩形混入现有 72% 浅粉遮罩。
pub(super) fn render(
    source: &SharedPixelBuffer<Rgba8Pixel>,
    width: u32,
    height: u32,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut target = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let bytes = target.make_mut_bytes();
    if source.width() == width && source.height() == height {
        bytes.copy_from_slice(source.as_bytes());
    } else {
        // cover 的中心裁剪及缩放只执行一次，滚动时直接绘制不透明像素。
        let scale =
            (width as f32 / source.width() as f32).max(height as f32 / source.height() as f32);
        let left = (source.width() as f32 - width as f32 / scale) * 0.5;
        let top = (source.height() as f32 - height as f32 / scale) * 0.5;
        let original = source.as_bytes();
        for y in 0..height {
            let fy =
                ((y as f32 + 0.5) / scale + top - 0.5).clamp(0.0, (source.height() - 1) as f32);
            let y0 = fy as u32;
            let y1 = (y0 + 1).min(source.height() - 1);
            let wy = fy - y0 as f32;
            for x in 0..width {
                let fx =
                    ((x as f32 + 0.5) / scale + left - 0.5).clamp(0.0, (source.width() - 1) as f32);
                let x0 = fx as u32;
                let x1 = (x0 + 1).min(source.width() - 1);
                let wx = fx - x0 as f32;
                let a = ((y0 * source.width() + x0) * 4) as usize;
                let b = ((y0 * source.width() + x1) * 4) as usize;
                let c = ((y1 * source.width() + x0) * 4) as usize;
                let d = ((y1 * source.width() + x1) * 4) as usize;
                let dest = ((y * width + x) * 4) as usize;
                for channel in 0..3 {
                    let upper = original[a + channel] as f32
                        + (original[b + channel] as f32 - original[a + channel] as f32) * wx;
                    let lower = original[c + channel] as f32
                        + (original[d + channel] as f32 - original[c + channel] as f32) * wx;
                    bytes[dest + channel] = (upper + (lower - upper) * wy).round() as u8;
                }
                bytes[dest + 3] = 255;
            }
        }
    }

    // 45/58/36px 分别为 SidebarNav/AppHeader/StatusBar 的既有边界。
    // 与 pink.slint 的 rgba(255, 240, 246, 0.72) 对齐，允许 8-bit 混色相差 1。
    const ALPHA: u32 = 184;
    for y in 58..height.saturating_sub(36) {
        for x in 45..width {
            let dest = ((y * width + x) * 4) as usize;
            for (channel, base) in PINK_BACKGROUND_BASE.iter().enumerate() {
                bytes[dest + channel] =
                    ((bytes[dest + channel] as u32 * (255 - ALPHA) + base * ALPHA + 127) / 255)
                        as u8;
            }
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 页面外的三栏保持原图；页面区与现有遮罩逐通道合成。
    #[test]
    fn composite_preserves_bars_and_tints_only_page() {
        let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        source.make_mut_bytes().copy_from_slice(&[20, 40, 60, 255]);
        let image = render(&source, 80, 120);
        let pixel = |x: usize, y: usize| &image.as_bytes()[(y * 80 + x) * 4..][..4];
        assert_eq!(pixel(44, 58), &[20, 40, 60, 255]);
        assert_eq!(pixel(45, 57), &[20, 40, 60, 255]);
        assert_eq!(pixel(45, 58), &[190, 184, 194, 255]);
        assert_eq!(pixel(79, 83), &[190, 184, 194, 255]);
        assert_eq!(pixel(45, 84), &[20, 40, 60, 255]);
    }
}
