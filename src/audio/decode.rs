//! Media Foundation 文件音频读取：mp3 / mp4 / m4a / flac / wma，视频轨只忽略不解码。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::core::{GUID, PCWSTR};

use super::com;
use super::wav::{self, WavData};
use crate::error::Error;

/// 进程内 MF 初始化闩锁，保证多个解码线程只启动一次 Media Foundation。
static MF_INIT: Once = Once::new();

/// 解码后的媒体元数据，时长沿用 Media Foundation 的 100 ns 时间基准。
#[derive(Clone, Copy, Debug)]
pub struct MediaInfo {
    /// 文件或流的总时长，单位为 100 ns；未知时为 0。
    pub duration_hns: u64,
    /// 解码输出的采样率，单位为 Hz。
    pub sample_rate: u32,
    /// 解码输出的交错声道数；每帧包含该数量的样本。
    pub channels: u16,
}

/// 统一文件读取器。WAV 直接读入内存，其余格式交给 MF 按块解码。
pub enum AudioReader {
    /// 已完整读入内存的 WAV；position 是下一次读取的交错样本下标。
    Wav {
        /// WAV 的采样率、声道数和样本数据，生命周期覆盖本读取器。
        data: WavData,
        /// 指向 `data.samples` 的交错样本游标，而不是字节偏移。
        position: usize,
    },
    /// Media Foundation 流式读取器，避免长文件一次性占满内存。
    Mf(MfReader),
}

/// Media Foundation 状态；`pending` 保存上次按块截断后尚未交付的样本。
pub struct MfReader {
    /// 持有 MF 源读取器 COM 引用，负责从文件产生音频样本。
    reader: IMFSourceReader,
    /// 已协商的浮点音频格式与媒体时长。
    info: MediaInfo,
    /// 最近一次 MF 样本中超过调用者请求上限的交错样本。
    pending: Vec<f32>,
}

/// 统一打开入口。
pub fn open(path: &Path) -> Result<AudioReader, Error> {
    // WAV 已能直接解析为交错 f32，绕过 MF 可避免线程/COM 开销并保留精确游标。
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("wav"))
    {
        return Ok(AudioReader::Wav {
            data: wav::read_wav(path)?,
            position: 0,
        });
    }
    Ok(AudioReader::Mf(MfReader::open(path)?))
}

/// 导入前验证扩展名与音频轨，避免无效条目进入音频库。
pub fn validate_file(path: &Path) -> Result<MediaInfo, Error> {
    // 扩展名先做白名单筛选，避免把任意文件交给 MF 后才得到模糊错误。
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "wav" | "mp3" | "mp4" | "m4a" | "flac" | "wma") {
        return Err(Error::Format("unsupported audio extension"));
    }
    if ext == "wav" {
        let (sample_rate, channels, duration_hns) = wav::probe_wav(path)?;
        return Ok(MediaInfo {
            duration_hns,
            sample_rate,
            channels,
        });
    }
    let reader = open(path)?;
    // 非 WAV 通过打开一次读取器完成真实媒体类型协商，而不是仅凭扩展名放行。
    let info = reader.info();
    if info.sample_rate == 0 || info.channels == 0 {
        return Err(Error::Format("unsupported audio format"));
    }
    Ok(info)
}

/// 统一读取器的元数据、定位和分块读取实现；保持 WAV 游标与 MF pending 语义一致。
impl AudioReader {
    /// 媒体元数据。
    pub fn info(&self) -> MediaInfo {
        match self {
            Self::Wav { data, .. } => MediaInfo {
                duration_hns: if data.sample_rate == 0 {
                    0
                } else {
                    (data.samples.len() as u64 / u64::from(data.channels.max(1))) * 10_000_000
                        / u64::from(data.sample_rate)
                },
                sample_rate: data.sample_rate,
                channels: data.channels,
            },
            Self::Mf(reader) => reader.info,
        }
    }

    /// 从 100 ns 位置 seek。
    pub fn seek_hns(&mut self, hns: u64) -> Result<(), Error> {
        match self {
            Self::Wav { data, position } => {
                // 将 100 ns 时间换算为整帧位置；按声道回到交错样本下标，避免落在半帧。
                let frames = hns.saturating_mul(u64::from(data.sample_rate)) / 10_000_000;
                *position = (frames as usize)
                    .saturating_mul(usize::from(data.channels))
                    .min(data.samples.len());
                Ok(())
            }
            Self::Mf(reader) => reader.seek_hns(hns),
        }
    }

    /// 读取完整文件供当前 SFX 播放器使用；兼容旧调用。
    pub fn read_all(mut self) -> Result<WavData, Error> {
        // 分块读取并追加，兼容 MF 的有界样本交付，同时不要求调用者理解 pending。
        let info = self.info();
        let mut samples = Vec::new();
        let mut block = Vec::with_capacity(16_384);
        loop {
            let n = self.read_block(&mut block, 16_384)?;
            if n == 0 {
                break;
            }
            samples.extend_from_slice(&block);
        }
        Ok(WavData {
            sample_rate: info.sample_rate,
            channels: info.channels,
            samples,
        })
    }

    /// 读取一块交错 f32，返回样本数；0 表示 EOF。
    pub fn read_block(&mut self, dst: &mut Vec<f32>, max_samples: usize) -> Result<usize, Error> {
        dst.clear();
        match self {
            // WAV 已在内存中，游标只前进到实际样本末尾，超出请求不会越界。
            Self::Wav { data, position } => {
                let end = position.saturating_add(max_samples).min(data.samples.len());
                dst.extend_from_slice(&data.samples[*position..end]);
                *position = end;
                Ok(dst.len())
            }
            Self::Mf(reader) => reader.read_block(dst, max_samples),
        }
    }
}

/// MF 源读取器的创建、定位与样本缓冲实现；所有 COM 接口只在此边界内操作。
impl MfReader {
    /// 初始化 MTA/MF，并将源文件协商为独立的交错 f32 音频流。
    fn open(path: &Path) -> Result<Self, Error> {
        let _com = com::init_mta()?;
        // COM 引用和 MF 读取器都在创建线程内取得，后续后台线程可独立初始化 apartment。
        // MFStartup 只需进程内成功一次；Once 避免并发打开重复初始化。
        MF_INIT.call_once(|| unsafe {
            // SAFETY: MFStartup 是进程级初始化；这里只传入 SDK 常量且由 Once 保证单次调用。
            let _ = MFStartup(MF_VERSION, MFSTARTUP_LITE);
        });
        let wide: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: wide 以 NUL 结尾，并在调用期间保持不变；MF 立即读取该路径。
        let reader = unsafe { MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), None) }
            .map_err(|_| Error::Format("Media Foundation cannot open file"))?;
        // SAFETY: reader 与 media_type 是受 COM 引用计数管理的有效接口；MF 方法在当前 MTA 调用。
        unsafe {
            // 先禁用所有流再只启用首条音频轨，确保视频轨不会进入解码热路径。
            let _ = reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false);
            reader
                .SetStreamSelection(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32, true)
                .map_err(|_| Error::Format("audio track missing"))?;
            let media_type = MFCreateMediaType().map_err(|_| Error::Format("create media type"))?;
            // 强制 MF 输出交错浮点样本，统一后续 WAV、试听和 SFX 的样本表示。
            media_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .map_err(|_| Error::Format("audio type"))?;
            media_type
                .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)
                .map_err(|_| Error::Format("float type"))?;
            media_type
                .SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)
                .map_err(|_| Error::Format("independent samples"))?;
            reader
                .SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                    None,
                    &media_type,
                )
                .map_err(|_| Error::Format("unsupported audio format"))?;
            let current = reader
                // 读取协商后的格式作为权威元数据，后续分块只按此声道数解释字节。
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)
                .map_err(|_| Error::Format("audio media type"))?;
            let rate = current
                .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                .map_err(|_| Error::Format("sample rate"))?;
            let channels = current
                .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
                .map_err(|_| Error::Format("channels"))? as u16;
            let duration = reader
                // 媒体时长缺失时使用 0；解码仍可继续，因为部分容器没有可靠的时长属性。
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
                .ok()
                .and_then(propvar_u64)
                .unwrap_or(0);
            Ok(Self {
                reader,
                info: MediaInfo {
                    duration_hns: duration,
                    sample_rate: rate,
                    channels,
                },
                pending: Vec::new(),
            })
        }
    }

    /// 清空待交付样本，并请求 MF 在指定 100 ns 位置重新开始读取。
    fn seek_hns(&mut self, hns: u64) -> Result<(), Error> {
        self.pending.clear();
        // seek 后必须丢弃旧块，否则新位置会先播放旧位置残留样本。
        // PROPVARIANT 的布局由 Windows ABI 定义；调用结束前由本函数独占该临时值。
        let mut value = unsafe { std::mem::zeroed::<PROPVARIANT>() };
        unsafe {
            // SAFETY: value 是刚初始化的栈上 PROPVARIANT，字段写入仅用于 SetCurrentPosition。
            (*value.Anonymous.Anonymous).vt = windows::Win32::System::Variant::VT_I8;
            (*value.Anonymous.Anonymous).Anonymous.lVal = hns as i32;
            self.reader
                .SetCurrentPosition(&GUID::zeroed(), &value)
                .map_err(|_| Error::Format("seek failed"))
        }
    }

    /// 从 MF 读取不超过上限的交错 f32 样本；不完整的 MF 样本留在 `pending`。
    fn read_block(&mut self, dst: &mut Vec<f32>, max_samples: usize) -> Result<usize, Error> {
        dst.clear();
        if max_samples == 0 {
            // 零上限是合法的探测调用，不能触发 MF 读取或改变 pending 状态。
            return Ok(0);
        }
        if !self.pending.is_empty() {
            // pending 属于上一块的尾部，优先交付才能保证样本顺序不被下一次 MF 读取打乱。
            let take = max_samples.min(self.pending.len());
            dst.extend(self.pending.drain(..take));
            return Ok(dst.len());
        }
        loop {
            let mut flags = 0u32;
            let mut sample = None;
            // SAFETY: 当前线程已初始化 MTA；COM 接口由局部引用持有，ReadSample 仅写入本地输出参数。
            // MF 样本和缓冲区均由 COM 引用持有；Lock 返回的裸指针只在 Unlock 前有效。
            unsafe {
                self.reader
                    .ReadSample(
                        MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags as *mut u32),
                        None,
                        Some(&mut sample),
                    )
                    .map_err(|_| Error::Format("decode failed"))?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    // MF 的 EOF 标志表示源流已结束；当前样本没有可交付数据，因此返回 EOF。
                    return Ok(0);
                }
                let Some(sample) = sample else {
                    continue;
                };
                let buffer = sample
                    .ConvertToContiguousBuffer()
                    .map_err(|_| Error::Format("audio buffer"))?;
                let mut ptr = std::ptr::null_mut();
                let mut current = 0u32;
                buffer
                    .Lock(&mut ptr, None, Some(&mut current))
                    .map_err(|_| Error::Format("lock audio buffer"))?;
                let bytes = if ptr.is_null() {
                    &[][..]
                } else {
                    // SAFETY: Lock 成功后 ptr 指向 current 字节的连续缓冲区，且仅在 Unlock 前读取。
                    std::slice::from_raw_parts(ptr as *const u8, current as usize)
                };
                // 只在 Lock/Unlock 生命周期内解释字节；复制出 Vec 后再安全地截断与暂存。
                // SAFETY: Lock 成功后 ptr 指向 current 字节的连续缓冲区，且仅在 Unlock 前读取。
                let samples = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect::<Vec<_>>();
                buffer
                    .Unlock()
                    .map_err(|_| Error::Format("unlock audio buffer"))?;
                // Unlock 归还 MF 缓冲区的借用，之后不再使用 bytes 或 ptr。
                if samples.is_empty() {
                    continue;
                }
                let take = max_samples.min(samples.len());
                // 调用者的上限按交错样本计数，超出的尾部留给下一次读取保持无损。
                dst.extend_from_slice(&samples[..take]);
                if take < samples.len() {
                    self.pending.extend_from_slice(&samples[take..]);
                }
                return Ok(dst.len());
            }
        }
    }
}
/// 在后台线程以有界通道解码文件；消费者可在首个块到达时开始播放，取消由原子标志传播。
/// `start_hns` 使用 MF 的 100 ns 时间单位，`looped` 只控制自然 EOF，不覆盖取消。
pub fn stream_to_channel(
    path: PathBuf,
    start_hns: u64,
    looped: bool,
    cancel: Arc<AtomicBool>,
) -> Result<(MediaInfo, Receiver<crate::audio::sfx::StreamChunk>), Error> {
    // 先在调用线程取得元数据，失败立即反馈而不是启动一个无法报告错误的线程。
    let info = open(&path)?.info();
    let (tx, rx) = std::sync::mpsc::sync_channel(32);
    // sync_channel 给解码线程设置背压上限，避免消费者暂停时无限堆积音频内存。
    thread::Builder::new()
        .name("audio-stream-decode".into())
        .spawn(move || {
            // 线程拥有 reader 和发送端；函数返回后不会把 COM 对象或文件句柄带回控制线程。
            // 在线程内重新打开，使 COM/MF 对象归属解码线程并避免跨线程借用。
            let Ok(mut reader) = open(&path) else {
                return;
            };
            if start_hns > 0 && reader.seek_hns(start_hns).is_err() {
                // 跳转失败就从头放，总比整段没声音好。
                tracing::warn!("seek to {start_hns} failed; playing from start");
            }
            // block 在循环外复用；发送时复制，因而接收端拥有独立样本所有权。
            let mut block = Vec::with_capacity(16_384);
            loop {
                // 取消优先于下一次读取和阻塞发送，确保停止不会继续消耗文件。
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                match reader.read_block(&mut block, 16_384) {
                    // 空块且循环播放时只重置读取位置，不改变取消协议。
                    Ok(0) if looped => {
                        // 循环播放只重置读取位置；取消标志仍由循环顶部统一优先处理。
                        let _ = reader.seek_hns(0);
                    }
                    Ok(0) => {
                        // 统一发送空块 EOF，让试听与 SFX 都能在消费完缓存后结束。
                        let _ = tx.send(crate::audio::sfx::StreamChunk {
                            samples: Vec::new(),
                            done: true,
                        });
                        break;
                    }
                    Ok(n) => {
                        // 有界发送可能因消费者停止而失败，此时释放发送端并安静退出线程。
                        if tx
                            .send(crate::audio::sfx::StreamChunk {
                                samples: block[..n].to_vec(),
                                done: false,
                            })
                            .is_err()
                        {
                            // 有界发送可能因消费者停止而失败，此时线程应安静退出。
                            break;
                        }
                    }
                    Err(_) => {
                        // 解码错误无法通过该通道携带错误类型，发送 EOF 以阻止消费者继续等待。
                        let _ = tx.send(crate::audio::sfx::StreamChunk {
                            samples: Vec::new(),
                            done: true,
                        });
                        break;
                    }
                }
            }
        })
        .map_err(|_| Error::RenderInit("spawn audio stream"))?;
    Ok((info, rx))
}

/// 从 MF 返回的 PROPVARIANT 读取无符号 64 位字段；调用方已确认其语义为时长。
fn propvar_u64(value: PROPVARIANT) -> Option<u64> {
    // 该值只用于媒体时长；未知或不匹配的 PROPVARIANT 由调用方统一视为 0。
    // SAFETY: MF_PD_DURATION 返回的 PROPVARIANT 生命周期覆盖本次读取，布局字段由 Windows ABI 定义。
    unsafe { Some((*value.Anonymous.Anonymous).Anonymous.uhVal) }
}
