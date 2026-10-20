//! WASAPI Process Loopback：按 PID 捕获该进程树的 PCM。

use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{CloseHandle, HANDLE, S_OK, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioCaptureClient, IAudioClient,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX,
};
use windows::Win32::Media::Audio::{
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
    IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{IUnknown, Interface, PCWSTR, Result as WinResult, implement};

use crate::error::Error;

use super::buffer::SpscRing;
use super::com;
use super::errors::classify_capture_error;
use super::format::{AudioFormat, Converter};

/// 保存异步激活回调发布的 HRESULT 和 COM 接口引用；等待线程在事件通知后读取。
struct ActivateState {
    /// 激活完成 HRESULT；回调成功读取时写入，等待线程收到事件后检查结果。
    hr: windows::core::HRESULT,
    /// 回调取得的接口引用；调用线程在完成通知后转换为 IAudioClient。
    unknown: Option<IUnknown>,
}

/// 由 COM `implement` 宏包装的完成处理器；回调与等待线程共享事件和激活结果。
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationHandler {
    /// 手动复位完成事件；回调仅 SetEvent，等待线程负责关闭句柄。
    event: HANDLE,
    /// 供异步回调与等待线程共享激活结果，Mutex 保护 HRESULT 和接口引用的一致性。
    state: Arc<Mutex<ActivateState>>,
}

/// 在回调线程读取激活结果并唤醒等待方，不执行音频初始化或其他阻塞工作。
impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationHandler_Impl {
    /// 复制异步激活结果到共享状态，并发出完成信号供调用线程继续处理。
    fn ActivateCompleted(
        &self,
        activate_operation: windows::core::Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> WinResult<()> {
        // SAFETY: 仅在 COM 回调期间借用其传入的操作接口；GetActivateResult 在共享状态锁内发布引用。
        unsafe {
            if let Some(op) = activate_operation.as_ref() {
                let mut hr = windows::core::HRESULT::default();
                let mut unk = None;
                // GetActivateResult 给出 hr + IUnknown；失败也要唤醒等待方。
                // 失败时不覆盖初始 HRESULT，但仍通过事件通知等待方检查激活结果。
                if op.GetActivateResult(&mut hr, &mut unk).is_ok()
                    && let Ok(mut guard) = self.state.lock()
                {
                    // 将 COM 接口引用移入共享状态，确保回调返回后仍由调用方持有。
                    guard.hr = hr;
                    guard.unknown = unk;
                }
            }
            let _ = SetEvent(self.event);
        }
        Ok(())
    }
}

/// 已 Initialize 的 loopback 捕获客户端，供捕获线程独占使用。
pub struct CaptureClient {
    /// 已激活的 WASAPI 客户端，由捕获线程负责 Start、Stop 和销毁。
    pub client: IAudioClient,
    /// 客户端提供的捕获服务；与 client 一起由捕获线程独占调用。
    pub capture: IAudioCaptureClient,
    /// 通知捕获数据就绪的 Win32 事件，随客户端一起移交并在退出时关闭。
    pub event: HANDLE,
    /// Initialize 实际采用的采样率、声道数与样本表示。
    pub format: AudioFormat,
    /// 原生 mix 格式指针的 RAII 所有者；非原生候选格式时为空。
    pub wave: MixFormat,
}

/// 堆上的 WAVEFORMATEX（含 cbSize 扩展）。
pub struct MixFormat {
    /// WASAPI 分配的格式块指针；由此类型独占，并在 Drop 中经 CoTaskMemFree 释放。
    ptr: *mut WAVEFORMATEX,
}

// SAFETY: 格式指针由 CoTaskMem 分配并随 MixFormat 独占持有；音频客户端只交给一个 MTA 捕获线程。
unsafe impl Send for MixFormat {}
unsafe impl Send for CaptureClient {}

/// 释放 WASAPI 分配的格式块；该指针只能由 `GetMixFormat` 的分配器回收。
impl Drop for MixFormat {
    /// 释放一次性格式指针，并把指针清空以防重复释放。
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: `ptr` 由 GetMixFormat/CoTaskMemAlloc 返回，且此 Drop 只执行一次释放。
            unsafe { CoTaskMemFree(Some(self.ptr as *const _)) };
            self.ptr = ptr::null_mut();
        }
    }
}
/// 按 PID 激活 Process Loopback 的 IAudioClient，并在当前 MTA 中同步等待完成。
#[allow(clippy::arc_with_non_send_sync)]
pub fn activate_process_loopback(pid: u32) -> Result<IAudioClient, Error> {
    // 事件与状态由激活调用持有；回调中的 COM 引用仅在收到完成通知后读取。
    // 手动复位事件：回调 SetEvent 后 Wait 返回。
    // SAFETY: 事件句柄由本线程创建并在同一线程等待、关闭。
    let event = unsafe { CreateEventW(None, true, false, None)? };
    // COM 回调持有共享状态副本；局部 handler 引用在等待期间保证回调对象存活。
    let state = Arc::new(Mutex::new(ActivateState {
        hr: S_OK,
        unknown: None,
    }));
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationHandler {
        event,
        state: state.clone(),
    }
    .into();

    // INCLUDE_TARGET_PROCESS_TREE：游戏启动器/子进程出声也能抓到。
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: windows::Win32::Media::Audio::AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };

    // blob 指向栈上 params。PROPVARIANT Drop 会 CoTaskMemFree，必须 ManuallyDrop。
    // SAFETY: 这里仅构造手动管理的空 PROPVARIANT；其 blob 指针随后改为借用栈上参数。
    let mut activate_params =
        std::mem::ManuallyDrop::new(unsafe { std::mem::zeroed::<PROPVARIANT>() });
    // SAFETY: ManuallyDrop 中的零初始化 PROPVARIANT 随后仅写入 VT_BLOB 字段；析构被抑制以免释放栈上借用数据。
    unsafe {
        let inner = &mut *activate_params.Anonymous.Anonymous;
        ptr::write(&mut inner.vt, VT_BLOB);
        ptr::write(
            &mut inner.Anonymous.blob.cbSize,
            size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        );
        ptr::write(
            &mut inner.Anonymous.blob.pBlobData,
            &params as *const AUDIOCLIENT_ACTIVATION_PARAMS as *mut u8,
        );

        // 必须持有 IActivateAudioInterfaceAsyncOperation 直到 Wait 返回，否则激活会被取消。
        // 异步操作必须在等待期间保持引用，否则完成回调可能在调用方读取结果前失效。
        let async_op = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*activate_params as *const PROPVARIANT),
            &handler,
        )?;
        // 异步操作与 handler 在等待期间保持强引用，避免请求取消或回调对象提前销毁。
        // 超时后不再读取回调状态；关闭事件并释放异步操作与 handler，结束本次激活等待。
        let wait = WaitForSingleObject(event, 8000);
        let _ = CloseHandle(event);
        drop(async_op);
        drop(handler);
        if wait != WAIT_OBJECT_0 {
            return Err(Error::TimedOut("process loopback activate"));
        }
    }
    // 回调在 SetEvent 前发布结果；等待成功后复制接口引用，再检查 HRESULT 并转换接口。
    let (hr, unknown) = {
        let guard = state
            .lock()
            .map_err(|_| Error::CaptureInit("activate lock poisoned"))?;
        (guard.hr, guard.unknown.clone())
    };
    hr.ok()?;
    let unknown = unknown.ok_or(Error::CaptureInit("no audio client"))?;
    let client: IAudioClient = unknown.cast()?;
    Ok(client)
}

/// 单次 Initialize。失败后 IAudioClient 不能复用，调用方必须重新 activate。
fn init_capture_client(
    client: IAudioClient,
    fmt: &WAVEFORMATEX,
    flags: u32,
) -> Result<CaptureClient, Error> {
    // SAFETY: Initialize、事件和 GetService 都在已初始化 COM 的音频线程边界内调用。
    unsafe {
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, fmt, None)?;
        let event = CreateEventW(None, false, false, None)?;
        client.SetEventHandle(event)?;
        let capture: IAudioCaptureClient = client.GetService()?;
        let format = AudioFormat::from_wave(fmt).ok_or(Error::Format("capture format"))?;
        Ok(CaptureClient {
            client,
            capture,
            event,
            format,
            wave: MixFormat {
                ptr: ptr::null_mut(),
            },
        })
    }
}

/// 将驱动返回的扩展格式压缩为 Initialize 可接受的基础格式，并补齐缺省值。
fn sanitize_wave(fmt: &WAVEFORMATEX) -> WAVEFORMATEX {
    let channels = fmt.nChannels.max(1);
    let rate = if fmt.nSamplesPerSec == 0 {
        48_000
    } else {
        fmt.nSamplesPerSec
    };
    let bits = if fmt.wBitsPerSample == 0 {
        32
    } else {
        fmt.wBitsPerSample
    };
    // 不能识别的采样率、位深和声道数采用 WASAPI 常用安全缺省，避免构造零字节帧。
    let is_float = fmt.wFormatTag == 3
        || bits == 32
            && fmt.nBlockAlign == channels * 4
            && fmt.wFormatTag != WAVE_FORMAT_PCM as u16;
    if is_float {
        float_wave(rate, channels)
    } else {
        pcm_wave(rate, channels, bits)
    }
}

/// PCM WAVEFORMATEX，cbSize 必须为 0。
fn pcm_wave(sample_rate: u32, channels: u16, bits: u16) -> WAVEFORMATEX {
    let channels = channels.max(1);
    let bits = if bits == 8 || bits == 16 || bits == 24 || bits == 32 {
        bits
    } else {
        16
    };
    let bytes = (u32::from(bits) / 8) * u32::from(channels);
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: sample_rate * bytes,
        nBlockAlign: bytes as u16,
        wBitsPerSample: bits,
        cbSize: 0,
    }
}

/// 立体声 PCM16。
pub(crate) fn pcm16_wave(sample_rate: u32) -> WAVEFORMATEX {
    pcm_wave(sample_rate, 2, 16)
}

/// IEEE float WAVEFORMATEX。
fn float_wave(sample_rate: u32, channels: u16) -> WAVEFORMATEX {
    let channels = channels.max(1);
    let bytes = 4u32 * u32::from(channels);
    WAVEFORMATEX {
        wFormatTag: 3,
        nChannels: channels,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: sample_rate * bytes,
        nBlockAlign: bytes as u16,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

/// 进程环回 Initialize。本机实测 44.1k PCM16 + LOOPBACK|AUTOCONVERT 能成功。
pub fn open_process_capture(
    pid: u32,
    preferred: Option<&WAVEFORMATEX>,
) -> Result<CaptureClient, Error> {
    let conv = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    let ms = conv | AUDCLNT_STREAMFLAGS_LOOPBACK;
    // 依次尝试常见 PCM16、调用方格式和浮点格式，兼顾不同驱动可接受的环回组合。
    let mut attempts: Vec<(WAVEFORMATEX, u32, &'static str)> = Vec::new();
    attempts.push((pcm16_wave(44_100), ms, "44100-pcm-loopback"));
    attempts.push((pcm16_wave(48_000), ms, "48000-pcm-loopback"));
    if let Some(fmt) = preferred {
        let fmt = sanitize_wave(fmt);
        attempts.push((fmt, conv, "preferred"));
        attempts.push((fmt, ms, "preferred-loopback"));
    }
    attempts.push((float_wave(48_000, 2), ms, "48000-f32-loopback"));
    attempts.push((pcm16_wave(44_100), conv, "44100-pcm"));

    // 保留最近一次候选失败，方便最终回退也失败时返回具体错误。
    let mut last: Option<Error> = None;
    // 每次尝试都重新激活客户端；Initialize 失败后 COM 约定该客户端不可复用。
    for (fmt, flags, name) in attempts {
        let client = match activate_process_loopback(pid) {
            Ok(c) => c,
            Err(e) => {
                last = Some(e);
                continue;
            }
        };
        match init_capture_client(client, &fmt, flags) {
            Ok(c) => {
                tracing::info!(
                    "capture initialize ok ({name}, {} Hz {} ch)",
                    c.format.sample_rate,
                    c.format.channels
                );
                return Ok(c);
            }
            Err(e) => {
                tracing::debug!("capture initialize {name} failed: {e}");
                last = Some(e);
            }
        }
    }

    // 最后一条回退读取设备原生 mix，避免所有候选格式都被驱动拒绝时完全失去捕获。
    match open_process_capture_native_mix(pid) {
        Ok(c) => {
            tracing::info!(
                "capture initialize ok (loopback-mix, {} Hz {} ch)",
                c.format.sample_rate,
                c.format.channels
            );
            Ok(c)
        }
        Err(e) => Err(last.unwrap_or(e)),
    }
}

/// 用进程环回客户端自己的 mix 格式 Initialize。
fn open_process_capture_native_mix(pid: u32) -> Result<CaptureClient, Error> {
    // 从客户端取得原生 mix 格式；失败时重新激活客户端再尝试非 loopback 标志。
    let client = activate_process_loopback(pid)?;
    // SAFETY: `mix` 由 WASAPI 分配并保持有效到 Initialize 返回，随后统一 CoTaskMemFree。
    unsafe {
        let mix = client.GetMixFormat()?;
        if mix.is_null() {
            return Err(Error::Format("loopback mix format null"));
        }
        let event = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        let with_loop = event | AUDCLNT_STREAMFLAGS_LOOPBACK;
        let result = init_capture_client(client, &*mix, with_loop).or_else(|_| {
            let client = activate_process_loopback(pid)?;
            init_capture_client(client, &*mix, event)
        });
        CoTaskMemFree(Some(mix as *const _));
        result
    }
}

/// 打开物理麦克风（普通 WASAPI 捕获，不是 loopback），优先尝试调用方目标格式。
pub fn open_mic_capture(
    device_id: &str,
    preferred: Option<&WAVEFORMATEX>,
) -> Result<CaptureClient, Error> {
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    // 自动转换仅用于首选格式；原生 mix 回退不要求驱动执行格式转换。
    if let Some(fmt) = preferred {
        let fmt = sanitize_wave(fmt);
        let client = activate_mic_device(device_id)?;
        match init_capture_client(client, &fmt, flags) {
            Ok(c) => {
                tracing::info!("mic initialize ok (preferred)");
                return Ok(c);
            }
            Err(e) => tracing::debug!("mic preferred format failed: {e}"),
        }
    }
    let client = activate_mic_device(device_id)?;
    // SAFETY: GetMixFormat 返回由 WASAPI 分配的有效格式块，Initialize 同步借用；调用后以 CoTaskMemFree 释放。
    unsafe {
        let mix = client.GetMixFormat()?;
        if mix.is_null() {
            return Err(Error::Format("mic mix format null"));
        }
        let result = init_capture_client(client, &*mix, AUDCLNT_STREAMFLAGS_EVENTCALLBACK);
        CoTaskMemFree(Some(mix as *const _));
        result
    }
}

/// 用不透明 WASAPI 设备 ID 激活录音端点；字符串只在 COM 调用期间借用。
fn activate_mic_device(device_id: &str) -> Result<IAudioClient, Error> {
    let _com = com::init_mta()?;
    // COM 初始化令牌使当前线程的 apartment 在设备激活期间保持有效。
    let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: UTF-16 缓冲在 GetDevice 返回前保持不移动，COM 只同步读取其终止字符串。
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
        Ok(device.Activate(CLSCTX_ALL, None)?)
    }
}

/// 捕获线程主循环：等事件 → 取包 → 转 f32 → 推进 ring。
pub fn run_capture_loop(
    capture: CaptureClient,
    ring: Arc<SpscRing>,
    mut converter: Converter,
    stop: Arc<AtomicBool>,
    peak: Option<Arc<AtomicU32>>,
    on_fail: impl FnOnce(Error),
) {
    // 捕获客户端、事件和格式所有权随函数进入此线程；退出路径统一 Stop/CloseHandle。
    let CaptureClient {
        client,
        capture,
        event,
        format,
        wave: _wave,
    } = capture;

    // Start 失败时不进入 GetBuffer，避免在未启动的客户端上消费无效数据。
    // SAFETY: 客户端已在本线程初始化，Start 不与其他线程并发调用。
    if let Err(e) = unsafe { client.Start() } {
        on_fail(e.into());
        // SAFETY: 事件句柄由本线程创建，失败路径不再有等待者。
        unsafe {
            let _ = CloseHandle(event);
        }
        return;
    }

    // 预分配，循环内只 clear/extend，不扩 cap（包大小通常稳定）。
    let mut converted = Vec::with_capacity(8192);
    let mut failed = None;
    // Acquire 与停止方的原子发布配对，确保退出请求在循环边界可见。
    while !stop.load(Ordering::Acquire) {
        // SAFETY: 事件句柄仍由当前线程独占，等待只读取其信号状态。
        let wait = unsafe { WaitForSingleObject(event, 80) };
        if wait == WAIT_TIMEOUT {
            continue;
        }
        if wait != WAIT_OBJECT_0 {
            failed = Some(Error::CaptureInit("capture wait failed"));
            break;
        }
        loop {
            // 一个事件可能对应多个待取包，必须排空后再等待下一次信号。
            // SAFETY: 捕获客户端由当前线程独占，且每个包均在 ReleaseBuffer 后再取下一包。
            let packet = match unsafe { capture.GetNextPacketSize() } {
                Ok(n) => n,
                Err(e) => {
                    failed = Some(classify_capture_error(e));
                    break;
                }
            };
            if packet == 0 {
                break;
            }
            // WASAPI 只在 GetBuffer 成功期间借出数据指针，ReleaseBuffer 前不得跨线程保存。
            let mut data: *mut u8 = ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            // SAFETY: 当前线程独占客户端，并为返回包保留 data/frames/flags 直到 ReleaseBuffer。
            if let Err(e) =
                unsafe { capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
            {
                failed = Some(classify_capture_error(e));
                break;
            }
            // SILENT 标志表示这一包应写成零，不要读野指针。
            let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
            let nbytes = frames as usize * format.frame_bytes();
            let bytes = if data.is_null() || silent {
                &[][..]
            } else {
                // SAFETY: WASAPI 保证缓冲区至少包含当前包的帧数与格式字节数。
                unsafe { std::slice::from_raw_parts(data, nbytes) }
            };
            // Converter 统一完成解码、声道布局和重采样，ring 因此只承载交错 f32。
            converter.convert(
                bytes,
                frames as usize,
                silent || data.is_null(),
                &mut converted,
            );
            if let Some(peak) = &peak {
                let value = converted
                    .iter()
                    .map(|sample| sample.abs())
                    .fold(0.0f32, f32::max);
                peak.store(value.to_bits(), Ordering::Relaxed);
            }
            // ring 是捕获与渲染间的固定容量交接点；写入最新帧，不等待消费者。
            ring.push_latest(&converted);
            // ReleaseBuffer 是归还包所有权的边界；失败后继续读取会破坏 WASAPI 状态。
            // SAFETY: 当前包仍由本线程持有，ReleaseBuffer 正好归还 WASAPI 借出的所有权。
            if let Err(e) = unsafe { capture.ReleaseBuffer(frames) } {
                failed = Some(classify_capture_error(e));
                break;
            }
        }
        if failed.is_some() {
            break;
        }
    }

    // SAFETY: 循环已结束且客户端仍由本线程持有，Stop 不会与消费并发。
    let _ = unsafe { client.Stop() };
    // SAFETY: 线程不再等待该句柄，关闭后不会有其他使用者。
    unsafe {
        let _ = CloseHandle(event);
    }
    // 主动停止触发的收尾不作为故障上报，避免把正常关停误报给调用方。
    if let Some(e) = failed
        && !stop.load(Ordering::Acquire)
    {
        on_fail(e);
    }
}
