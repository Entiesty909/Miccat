//! CLI 进程环回采集与 WAV 导出验证路径。

use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::AUDCLNT_BUFFERFLAGS_SILENT;
use windows::Win32::System::Threading::WaitForSingleObject;

use crate::audio::capture::{CaptureClient, open_process_capture, pcm16_wave};
use crate::audio::com;
use crate::audio::format::{AudioFormat, Converter, SampleKind};
use crate::audio::wav;
use crate::error::Error;

/// 将指定进程的环回音频采集为 PCM16 WAV；不经过共享 ring 或实时 render。
pub(crate) fn capture_process_to_wav(pid: u32, path: &Path, seconds: f32) -> Result<(), Error> {
    // 该路径独占采集客户端，避免把验证数据写入实时路由 ring。
    let _com = com::init_mta()?;
    // COM 必须在执行 WASAPI/进程环回调用的线程初始化为 MTA。
    let pcm16 = pcm16_wave(44_100);
    // 固定 44.1 kHz PCM16 作为采集目标，便于生成通用 WAV；后面仍按实际格式转换。
    let capture: CaptureClient = open_process_capture(pid, Some(&pcm16))?;
    // CaptureClient 持有 event、IAudioClient 和捕获接口，离开函数前必须显式 Stop。

    let src_format = capture.format;
    // 转换器把设备实际格式统一成 f32，收集缓冲只保存可直接写 WAV 的归一化样本。
    let mut converter = Converter::new(
        src_format,
        AudioFormat {
            sample_rate: src_format.sample_rate,
            channels: src_format.channels,
            kind: SampleKind::F32,
        },
    );

    // SAFETY: capture.client 由 open_process_capture 返回且仍由当前线程独占，Start 只改变其采集状态。
    unsafe { capture.client.Start()? };

    let mut collected = Vec::new();
    // 两个 Vec 在循环外复用；实时等待期间不因每个 packet 反复分配临时缓冲。
    let mut converted = Vec::with_capacity(8192);
    let deadline = Instant::now() + Duration::from_secs_f32(seconds.max(0.2));
    // 至少采集 200 ms，避免极短或负数参数生成没有可用音频帧的文件。
    while Instant::now() < deadline {
        // SAFETY: event 是捕获客户端创建并持有的有效句柄；等待不会转移句柄所有权。
        let wait = unsafe { WaitForSingleObject(capture.event, 80) };
        // 事件只提示“可能有包”；醒来后仍需循环取尽队列，防止下一次事件前积压。
        if wait == WAIT_TIMEOUT {
            continue;
        }
        if wait != WAIT_OBJECT_0 {
            break;
        }
        loop {
            // GetNextPacketSize 返回 0 表示当前队列已清空，而非整个采集已结束。
            let packet = unsafe { capture.capture.GetNextPacketSize()? };
            if packet == 0 {
                break;
            }
            let mut data: *mut u8 = ptr::null_mut();
            // data 只在对应 GetBuffer/ReleaseBuffer 区间有效，不能跨 packet 保存裸指针。
            // SILENT 包无需读取 data；以空切片交给 converter，由 silent 参数生成静音帧。
            let mut frames = 0u32;
            let mut flags = 0u32;
            // SAFETY: data、frames、flags 都是本次 GetBuffer 的输出槽位，接口只在当前线程写入。
            unsafe {
                capture
                    .capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            }
            // SILENT 包无需读取 data；以空切片交给 converter，由 silent 参数生成静音帧。
            let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
            let nbytes = frames as usize * src_format.frame_bytes();
            let bytes = if data.is_null() || silent {
                &[][..]
            } else {
                // SAFETY: 非空 data 指向本次 GetBuffer 的 nbytes 字节；ReleaseBuffer 前仅只读该切片。
                unsafe { std::slice::from_raw_parts(data, nbytes) }
            };
            // data 只在 GetBuffer/ReleaseBuffer 区间有效；转换函数不保存裸指针。
            converter.convert(
                bytes,
                frames as usize,
                silent || data.is_null(),
                &mut converted,
            );
            // 转换后立即复制到拥有所有权的 collected，随后才能安全 ReleaseBuffer。
            collected.extend_from_slice(&converted);
            // 每个成功 GetBuffer 都必须配对 ReleaseBuffer，否则环回端点会停止继续投递。
            // SAFETY: frames 来自同一次成功 GetBuffer，调用顺序与捕获接口契约匹配。
            unsafe { capture.capture.ReleaseBuffer(frames)? };
        }
    }

    // Stop 失败不应遮蔽已经采集到的数据；句柄仍要关闭以释放内核事件资源。
    // SAFETY: Stop 和 CloseHandle 只操作本函数持有的客户端与事件句柄，且此时已停止读取。
    unsafe {
        let _ = capture.client.Stop();
        let _ = CloseHandle(capture.event);
    }

    // 输出采样率/声道与 converter 的目标格式一致，避免头部描述和 PCM 数据不匹配。
    wav::write_pcm16_wav(
        path,
        src_format.sample_rate,
        src_format.channels,
        &collected,
    )
}
