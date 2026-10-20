//! WASAPI 共享模式渲染：从 ring 取 f32，写到用户选的设备。

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient, IAudioRenderClient,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use crate::error::Error;

use super::buffer::SpscRing;
use super::com;
use super::errors::classify_render_error;
use super::format::{AudioFormat, SampleKind, encode_from_f32};

/// 已 Initialize 的渲染客户端，由渲染线程独占。
pub struct RenderClient {
    /// 绑定到当前输出端点的 WASAPI 客户端；整个生命周期只由渲染线程访问。
    pub client: IAudioClient,
    /// 从客户端取得的渲染服务；只在 `client` 存活期间有效。
    pub render: IAudioRenderClient,
    /// WASAPI 写缓冲事件（自动复位）；由本结构体创建并在 Drop 中关闭。
    pub event: HANDLE,
    /// 设备协商出的 mix 格式，决定声道数、采样类型和每帧字节布局。
    pub format: AudioFormat,
    /// 客户端环形缓冲区的总帧数，用于从 padding 计算本轮可写上限。
    pub buffer_frames: u32,
    /// `GetMixFormat` 返回的 COM 内存；必须由 Drop 通过 `CoTaskMemFree` 释放。
    mix: *mut WAVEFORMATEX,
}

/// 渲染客户端的句柄和格式由打开线程创建，随后整体移动给唯一渲染线程。
// SAFETY: COM 接口、事件和 mix 指针只转移给一个已初始化 MTA 的渲染线程，不发生并发访问。
unsafe impl Send for RenderClient {}

/// 关闭事件并释放 WASAPI 分配的 mix 格式，保证客户端离开线程时资源成对回收。
impl Drop for RenderClient {
    /// 先关闭等待句柄，再释放格式块；二者都不再被渲染循环使用。
    fn drop(&mut self) {
        // SAFETY: 事件和 mix 指针都由本结构体拥有，Drop 运行时渲染循环已停止。
        unsafe {
            if !self.event.is_invalid() {
                let _ = CloseHandle(self.event);
                self.event = HANDLE::default();
            }
            if !self.mix.is_null() {
                CoTaskMemFree(Some(self.mix as *const _));
                self.mix = ptr::null_mut();
            }
        }
    }
}

/// 读取打开设备时取得的 mix 格式，供捕获端选择一致的目标布局。
impl RenderClient {
    /// 设备 mix WAVEFORMATEX，捕获侧 AUTOCONVERT 会克隆它。
    pub fn mix_format(&self) -> &WAVEFORMATEX {
        // SAFETY: `mix` 在 RenderClient 生命周期内保持有效，且只读借用不转移所有权。
        unsafe { &*self.mix }
    }
}

/// 在 MTA 中按不透明设备 ID 创建共享模式渲染流，并保留格式块直到客户端销毁。
pub fn open_render_device(device_id: &str) -> Result<RenderClient, Error> {
    let _com = com::init_mta()?;
    let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: 以下 COM/Win32 调用在当前 MTA 同步完成，返回的接口由 RenderClient 独占。
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDevice(windows::core::PCWSTR(wide.as_ptr()))?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let mix = client.GetMixFormat()?;
        if mix.is_null() {
            return Err(Error::Format("render mix format null"));
        }
        let format = AudioFormat::from_wave(&*mix).ok_or(Error::Format("render mix format"))?;
        let event = CreateEventW(None, false, false, None)?;
        // 共享模式要求使用设备 mix；传入其他格式会让 Initialize 直接拒绝。
        // 共享模式必须用 mix format；hns 填 0 让引擎选周期。
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0,
            &*mix,
            None,
        )?;
        client.SetEventHandle(event)?;
        let buffer_frames = client.GetBufferSize()?;
        let render: IAudioRenderClient = client.GetService()?;
        Ok(RenderClient {
            client,
            render,
            event,
            format,
            buffer_frames,
            mix,
        })
    }
}

/// 消费各路 SPSC ring，在一个设备周期内完成独立增益、混音和 PCM 编码。
#[allow(clippy::too_many_arguments)]
pub fn run_render_loop(
    session: RenderClient,
    process_rings: Vec<Arc<SpscRing>>,
    mic_ring: Option<Arc<SpscRing>>,
    sfx_rings: Vec<Arc<SpscRing>>,
    process_volumes: Vec<Arc<AtomicU32>>,
    mic_volume: Option<Arc<AtomicU32>>,
    sfx_volumes: Vec<Arc<AtomicU32>>,
    master: Arc<AtomicU32>,
    route_peak: Option<Arc<AtomicU32>>,
    stop: Arc<AtomicBool>,
    on_fail: impl FnOnce(Error),
) {
    // 所有输入 ring 与音量原子按调用方传入的槽位一一对应；
    // 循环不改变这些集合的长度或顺序。
    // 所有转换缓冲在线程启动时分配，循环内只复用容量，避免实时路径扩容。
    // Start 失败时客户端尚未进入可消费状态，直接通知调用方并结束线程。
    // SAFETY: 客户端已在本线程初始化，Start 不与其他线程并发调用。
    if let Err(e) = unsafe { session.client.Start() } {
        on_fail(e.into());
        return;
    }

    let channels = session.format.channels as usize;
    let buffer_frames = session.buffer_frames;
    let mut tmp = vec![0.0f32; buffer_frames as usize * channels];
    let mut mic_tmp = vec![0.0f32; buffer_frames as usize * channels];
    let mut sfx_tmp = vec![0.0f32; buffer_frames as usize * channels];
    let mut failed = None;

    // 事件只表示设备可能有空间；每次仍以 padding 计算可写帧数，避免越界写入。
    while !stop.load(Ordering::Acquire) {
        // 80ms 超时让线程即使未收到设备事件也能及时观察 stop 标志。
        // SAFETY: 事件句柄由当前渲染线程创建并独占等待。
        let wait = unsafe { WaitForSingleObject(session.event, 80) };
        if wait == WAIT_TIMEOUT {
            continue;
        }
        // 非超时、非信号状态代表等待句柄或线程环境异常，不能继续索引设备缓冲。
        if wait != WAIT_OBJECT_0 {
            failed = Some(Error::RenderInit("render wait failed"));
            break;
        }
        // padding 是设备已经占用的帧数；只用剩余容量向 WASAPI 请求缓冲区。
        // SAFETY: 客户端只在渲染线程调用 GetCurrentPadding，返回值仅用于本周期边界计算。
        let padding = match unsafe { session.client.GetCurrentPadding() } {
            Ok(p) => p,
            Err(e) => {
                failed = Some(classify_render_error(e));
                break;
            }
        };
        let avail = buffer_frames.saturating_sub(padding);
        // 设备没有可写帧时不触碰 ring，下一次事件再重试，保持源端节奏。
        if avail == 0 {
            continue;
        }
        // GetBuffer 成功后必须在所有正常路径调用 ReleaseBuffer，借用期间不得再次取缓冲。
        // SAFETY: GetBuffer 借出的设备内存由当前线程独占，直至下面的 ReleaseBuffer。
        let ptr_data = match unsafe { session.render.GetBuffer(avail) } {
            Ok(p) => p,
            Err(e) => {
                failed = Some(classify_render_error(e));
                break;
            }
        };
        // tmp 是本周期交错样本的混音总线；先清零使每个源从静音基线开始累加。
        let samples = avail as usize * channels;
        tmp[..samples].fill(0.0);
        // 每个进程 ring 都写入同一个临时区；消费后清零尾部，
        // 避免短包残留到下一个源或周期。
        // 逐路清零未取满的尾部，保证短包不会把上一周期的样本带入本周期。
        for (index, process_ring) in process_rings.iter().enumerate() {
            let n = process_ring.pop(&mut mic_tmp[..samples]);
            mic_tmp[n..samples].fill(0.0);
            // 音量原子按 process ring 的索引配对；读取一次后用于本周期全部样本，
            // 避免半周期跳变。
            let volume = process_volumes
                .get(index)
                .map(|gain| f32::from_bits(gain.load(Ordering::Relaxed)))
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            for i in 0..samples {
                tmp[i] += mic_tmp[i] * volume;
            }
        }
        // 每个进程槽使用自己的 gain 原子，互不影响；缺失 gain 时保持兼容的单位增益。
        // 物理麦克风是可选独立槽位；不存在时不从其他 ring 借用数据。
        if let Some(mic) = &mic_ring {
            let mic_vol = mic_volume
                .as_ref()
                .map(|gain| f32::from_bits(gain.load(Ordering::Relaxed)))
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let mn = mic.pop(&mut mic_tmp[..samples]);
            mic_tmp[mn..samples].fill(0.0);
            for i in 0..samples {
                tmp[i] += mic_tmp[i] * mic_vol;
            }
        }
        // 麦克风与进程路由共享混音总线，但保留独立音量原子以便单卡片调节。
        // SFX 每个槽位独立消费并单独增益，停止一个槽位不会清掉其他槽位的已排队音频。
        for (index, sfx) in sfx_rings.iter().enumerate() {
            let sfx_vol = sfx_volumes
                .get(index)
                .map(|gain| f32::from_bits(gain.load(Ordering::Relaxed)))
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let sn = sfx.pop(&mut sfx_tmp[..samples]);
            sfx_tmp[sn..samples].fill(0.0);
            for i in 0..samples {
                tmp[i] += sfx_tmp[i] * sfx_vol;
            }
        }
        let master_vol = f32::from_bits(master.load(Ordering::Relaxed)).clamp(0.0, 1.0);
        // 主音量位于所有源混合之后；限制到 [-1, 1] 防止编码前溢出，
        // 并记录本周期峰值。
        let mut output_peak = 0.0f32;
        for sample in &mut tmp[..samples] {
            *sample = (*sample * master_vol).clamp(-1.0, 1.0);
            output_peak = output_peak.max(sample.abs());
        }
        // 峰值只供监视显示，不参与渲染控制；Relaxed 原子更新即可
        // 避免额外同步开销。
        if let Some(peak) = &route_peak {
            peak.fetch_max(output_peak.to_bits(), Ordering::Relaxed);
        }
        // 设备可能返回空指针；此时仍完成环消费，避免源端因未写出而失去节拍。
        // 只有拿到有效目标地址才写设备；空地址仍释放本轮帧数，保持 ring 与设备进度一致。
        if !ptr_data.is_null() {
            let nbytes = samples * session.format.kind.bytes();
            // SAFETY: WASAPI 返回的缓冲区至少覆盖 `samples * bytes_per_sample` 字节。
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr_data, nbytes) };
            // F32 已与设备样本类型一致，直接按小端字节复制；其他格式统一走量化编码器。
            if session.format.kind == SampleKind::F32 {
                for (i, sample) in tmp[..samples].iter().enumerate() {
                    let bytes = sample.to_le_bytes();
                    let o = i * 4;
                    dst[o..o + 4].copy_from_slice(&bytes);
                }
            } else {
                encode_from_f32(&tmp[..samples], session.format.kind, dst);
            }
        }
        // SAFETY: 当前线程仍持有本周期的设备缓冲，调用正是归还该内存的边界。
        // ReleaseBuffer 是 GetBuffer 的所有权边界；失败时停止循环并按 HRESULT 分类上报。
        if let Err(e) = unsafe { session.render.ReleaseBuffer(avail, 0) } {
            failed = Some(classify_render_error(e));
            break;
        }
    }

    // Stop 只在循环结束后调用；正常 stop 不上报错误，设备故障才交给回调。
    // SAFETY: 循环已退出且客户端由当前线程独占，Stop 不与消费并发。
    let _ = unsafe { session.client.Stop() };
    // 只有非主动停止导致的失败才回调；主动关停期间的设备错误
    // 不应覆盖正常退出状态。
    if let Some(e) = failed
        && !stop.load(Ordering::Acquire)
    {
        on_fail(e);
    }
}

/// 把设备忙 HRESULT 翻译成用户可理解的端点占用原因。
pub(crate) fn describe_open_failure(device_id: &str, error: Error) -> Error {
    // 只有 COM 错误携带 HRESULT；其他领域错误必须原样返回，
    // 避免丢失已有语义。
    let Error::Com(com) = &error else {
        return error;
    };
    // 0x8889000A 是 AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED，表示端点占用。
    // 将该 HRESULT 转为用户可理解的设备忙错误。
    if com.code().0 as u32 != 0x8889_000A {
        return error;
    }
    // 重新枚举仅用于取得可读标签；失败时仍使用调用方传入的
    // 不透明设备 ID。
    let label = crate::audio::device::list_destinations(true)
        .ok()
        .and_then(|devices| {
            devices
                .into_iter()
                .find(|device| device.render_id == device_id)
                .map(|device| device.label)
        })
        .unwrap_or_else(|| device_id.to_string());
    // label 仅影响用户提示，不参与后续路由或设备匹配。
    Error::DeviceBusy(format!(
        "「{label}」已被同一驱动的另一个端点占用。\n\
         该驱动不允许两个端点同时输出，请把其中一路改到别的设备。"
    ))
}
