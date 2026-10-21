//! 主题网络基础设施：通过 WinHTTP 下载淡粉主题的远程背景图字节。

use std::ffi::c_void;
use std::ptr::null_mut;

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::w;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// HTTP 图片响应；状态码与正文分开保存以识别成功但无图的 API 响应。
pub(crate) struct ThemeResponse {
    /// WinHTTP 最终响应状态码，跟随重定向后的值。
    pub(crate) status_code: u32,
    /// 有界读取的响应正文；HTTP 200 的空正文交给图片解码重试逻辑处理。
    pub(crate) body: Vec<u8>,
}

/// 下载淡粉主题图片，并返回最终状态码及大小受限的响应正文。
pub(crate) fn download_pink_background() -> Result<ThemeResponse, String> {
    unsafe {
        // SAFETY: WinHTTP 句柄只在当前同步调用链中使用，所有成功创建的句柄在返回前关闭。
        let session = WinHttpOpen(
            w!("Miccat/0.1 theme-image"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            None,
            None,
            0,
        );
        if session.is_null() {
            return Err("无法创建主题图片网络会话".into());
        }
        let result = download_with_session(session);
        let _ = WinHttpCloseHandle(session);
        result
    }
}

/// 使用已创建的 WinHTTP 会话完成连接、状态读取与有界正文读取。
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn download_with_session(session: *mut c_void) -> Result<ThemeResponse, String> {
    // SAFETY: 所有句柄来自当前函数的 WinHTTP 调用，指针生命周期不跨出本函数。
    WinHttpSetTimeouts(session, 5_000, 5_000, 5_000, 10_000)
        .map_err(|error| format!("主题图片网络超时设置失败：{error}"))?;
    let connection = WinHttpConnect(
        session,
        w!("www.loliapi.com"),
        INTERNET_DEFAULT_HTTPS_PORT,
        0,
    );
    if connection.is_null() {
        return Err("无法连接主题图片服务".into());
    }
    let request = WinHttpOpenRequest(
        connection,
        w!("GET"),
        w!("/acg/pc/"),
        None,
        None,
        null_mut(),
        WINHTTP_FLAG_SECURE,
    );
    if request.is_null() {
        let _ = WinHttpCloseHandle(connection);
        return Err("无法创建主题图片请求".into());
    }
    let result = request_body(request);
    let _ = WinHttpCloseHandle(request);
    let _ = WinHttpCloseHandle(connection);
    result
}

/// 发送 GET 请求并读取最终状态码；非 200 状态返回空正文供有界重试判断。
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn request_body(request: *mut c_void) -> Result<ThemeResponse, String> {
    // SAFETY: request 是当前线程创建的有效 WinHTTP 请求句柄。
    WinHttpSendRequest(request, None, None, 0, 0, 0)
        .map_err(|error| format!("主题图片请求发送失败：{error}"))?;
    WinHttpReceiveResponse(request, null_mut())
        .map_err(|error| format!("主题图片响应失败：{error}"))?;

    let mut status_code = 0u32;
    let mut status_length = std::mem::size_of_val(&status_code) as u32;
    WinHttpQueryHeaders(
        request,
        WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
        None,
        Some((&mut status_code as *mut u32).cast()),
        &mut status_length,
        null_mut(),
    )
    .map_err(|error| format!("主题图片 HTTP 状态读取失败：{error}"))?;
    if status_code != 200 {
        return Ok(ThemeResponse {
            status_code,
            body: Vec::new(),
        });
    }

    let mut body = Vec::new();
    loop {
        let mut available = 0u32;
        WinHttpQueryDataAvailable(request, &mut available)
            .map_err(|error| format!("主题图片长度读取失败：{error}"))?;
        if available == 0 {
            break;
        }
        let next_len = body
            .len()
            .checked_add(available as usize)
            .ok_or_else(|| "主题图片响应长度溢出".to_string())?;
        if next_len > MAX_IMAGE_BYTES {
            return Err("主题图片超过大小限制".into());
        }
        let start = body.len();
        body.resize(next_len, 0);
        let mut read = 0u32;
        WinHttpReadData(
            request,
            body[start..].as_mut_ptr() as *mut c_void,
            available,
            &mut read,
        )
        .map_err(|error| format!("主题图片读取失败：{error}"))?;
        body.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }
    Ok(ThemeResponse { status_code, body })
}
