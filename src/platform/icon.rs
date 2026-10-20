//! 从 Windows 进程或文件提取小尺寸图标，转换为 Slint 图片。

use std::ffi::c_void;
use std::ptr::null_mut;

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, HBITMAP, SelectObject,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{
    ExtractIconExW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW,
};
use windows::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};
use windows::core::{PCWSTR, PWSTR};

/// Windows 图标栅格边长，render_icon 与 Slint 缓冲区必须保持一致。
const ICON_SIZE: i32 = 32;

/// 提取进程 exe 图标；进程路径或图标不可读时返回空图片。
#[allow(unsafe_code)]
#[allow(clippy::too_many_lines)]
pub(crate) fn process_icon(pid: u32) -> Image {
    // 进程路径查询失败时返回空图，调用方仍可渲染没有图标的进程卡片。
    let Some(path) = process_path(pid) else {
        return Image::default();
    };
    file_icon(&path)
}

/// 提取本地音频文件的 Windows 文件图标；失败时返回空图片。
pub(crate) fn file_icon(path: &str) -> Image {
    let path = std::path::Path::new(path);
    let extension = path.extension().and_then(std::ffi::OsStr::to_str);
    if extension.is_some_and(|extension| {
        [
            "avif", "bmp", "exr", "gif", "jpeg", "jpg", "png", "qoi", "tif", "tiff", "webp",
        ]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    }) && let Ok(image) = Image::load_from_path(path)
    {
        // 常见图片直接交给 Slint 解码，避免无意义的 Shell 图标往返。
        return image;
    }
    let path = path.to_string_lossy();
    let wide = path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if let Some(icon) = extract_icon(PCWSTR(wide.as_ptr())) {
        // ExtractIconExW 返回的句柄由 take_icon 统一销毁，避免成功路径泄漏。
        return take_icon(icon);
    }

    let mut info = SHFILEINFOW::default();
    let result = unsafe {
        // SAFETY: wide 以 NUL 结尾，info 是可写的 Shell 文件信息结构。
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if result == 0 || info.hIcon.is_invalid() {
        return Image::default();
    }
    // SHGetFileInfoW 的 hIcon 同样是本次取得的资源，转换后必须调用 DestroyIcon。
    take_icon(info.hIcon)
}

/// 从文件路径请求大图标句柄；调用方取得句柄后必须负责 DestroyIcon。
fn extract_icon(path: PCWSTR) -> Option<HICON> {
    let mut large = HICON::default();
    let count = unsafe {
        // SAFETY: path 以 NUL 结尾，large 是可写的图标句柄槽。
        ExtractIconExW(path, 0, Some(&mut large as *mut HICON), None, 1)
    };
    (count != 0 && !large.is_invalid()).then_some(large)
}

/// 将拥有的图标句柄转换为 Slint 图片，并在转换后释放句柄。
fn take_icon(icon: HICON) -> Image {
    let image = icon_to_image(icon);
    unsafe {
        // SAFETY: 图标句柄由 Shell API 创建，转换完成后在此处释放一次。
        let _ = DestroyIcon(icon);
    }
    image
}

/// 查询进程的 Win32 可执行文件路径；返回字符串前关闭临时进程句柄。
fn process_path(pid: u32) -> Option<String> {
    let process = unsafe {
        // SAFETY: 只请求受限查询权限，不读取进程内存；句柄由本函数关闭。
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?
    };
    // 预留固定 UTF-16 缓冲区，QueryFullProcessImageNameW 通过 length 返回实际字符数。
    let mut buffer = vec![0u16; 1024];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        // SAFETY: buffer 容量由 length 传入，PWSTR 指向有效可写内存，句柄在本函数内有效。
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    unsafe {
        // SAFETY: process 是 OpenProcess 返回的有效句柄，且只在本函数内关闭一次。
        let _ = CloseHandle(process);
    }
    result.ok()?;
    buffer.truncate(length as usize);
    (!buffer.is_empty()).then(|| String::from_utf16_lossy(&buffer))
}

/// 把渲染得到的 BGRA 像素整理成 Slint 所需的 RGBA 图片。
fn icon_to_image(icon: HICON) -> Image {
    // GDI 创建或绘制失败时返回空图，避免把无效句柄传给 Slint。
    let Some(mut pixels) = render_icon(icon) else {
        return Image::default();
    };
    let (chunks, _) = pixels.as_chunks_mut::<4>();
    for pixel in chunks {
        // GDI DIB 按 BGR 排列，而 Slint 的 Rgba8Pixel 需要 RGBA；无 alpha 的图标补不透明度。
        pixel.swap(0, 2);
        if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
            pixel[3] = 255;
        }
    }
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(ICON_SIZE as u32, ICON_SIZE as u32);
    // render_icon 已复制出独立 Vec，拷贝后 Slint 缓冲区不再依赖 GDI 内存。
    buffer.make_mut_bytes().copy_from_slice(&pixels);
    Image::from_rgba8(buffer)
}

/// 使用临时 GDI 位图绘制图标，成功时复制像素后释放全部 GDI 对象。
fn render_icon(icon: HICON) -> Option<Vec<u8>> {
    // hdc 与后续 DIB 只在本函数内使用，统一在所有返回路径释放。
    let hdc = unsafe {
        // SAFETY: 传入 None 创建独立的内存 DC，不借用外部 DC。
        CreateCompatibleDC(None)
    };
    if hdc.is_invalid() {
        return None;
    }
    let mut bits: *mut c_void = null_mut();
    // 负高度创建顶向下 DIB，使内存首行对应图像顶部，省去行序翻转。
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: ICON_SIZE,
            biHeight: -ICON_SIZE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let bitmap = unsafe {
        // SAFETY: info、bits 都是本地可写对象，DIB 使用 32 位顶向下像素缓冲。
        CreateDIBSection(Some(hdc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()
    };
    let Some(bitmap) = bitmap else {
        unsafe {
            // SAFETY: hdc 由 CreateCompatibleDC 创建，当前分支尚未选择 bitmap。
            let _ = DeleteDC(hdc);
        }
        return None;
    };
    // bitmap 选入 DC 后，DrawIconEx 才能直接写入这块像素；退出前必须恢复 old。
    let old = unsafe {
        // SAFETY: bitmap 是当前 DC 可选择的 DIB，句柄类型由 windows-rs 保证匹配。
        SelectObject(hdc, bitmap.into())
    };
    // 仅在 bitmap 已选入 DC 时构造切片，长度固定为 32×32×4 字节。
    let bytes = unsafe {
        // SAFETY: bits 指向 ICON_SIZE² 个 32 位像素，DrawIconEx 只写入已选中的 DIB。
        std::slice::from_raw_parts_mut(bits.cast::<u8>(), (ICON_SIZE * ICON_SIZE * 4) as usize)
    };
    bytes.fill(0);
    let drawn = unsafe {
        // SAFETY: hdc 和 icon 均在本函数调用期间有效，绘制区域与 DIB 尺寸一致。
        DrawIconEx(hdc, 0, 0, icon, ICON_SIZE, ICON_SIZE, 0, None, DI_NORMAL).is_ok()
    };
    // 先复制像素再恢复并删除 GDI 对象，确保返回 Vec 不再依赖 DIB 内存。
    let output = drawn.then(|| bytes.to_vec());
    unsafe {
        // SAFETY: 恢复原对象后删除 DIB 和内存 DC，避免 GDI 句柄泄漏。
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HBITMAP(bitmap.0).into());
        let _ = DeleteDC(hdc);
    }
    output
}
