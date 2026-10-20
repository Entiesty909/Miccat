//! 解析 WAVEFORMATEX，捕获线程把 PCM 转成渲染端 f32。

use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVEFORMATEXTENSIBLE};
use windows::core::GUID;

/// WAVEFORMATEX 中基础 PCM 格式的标记值，供格式分派使用。
const WAVE_FORMAT_PCM: u16 = 1;
/// WAVEFORMATEX 中基础 IEEE 浮点格式的标记值。
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
/// WAVEFORMATEX 中携带扩展子格式 GUID 的标记值。
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
/// PCM 扩展格式对应的子格式标识。
const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x0000_0001_0000_0010_8000_00aa_0038_9b71);
/// IEEE 浮点扩展格式对应的子格式标识。
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_u128(0x0000_0003_0000_0010_8000_00aa_0038_9b71);

/// PCM 样本类型。ring 内部一律 f32。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    /// IEEE float 32-bit。
    F32,
    /// 有符号 16-bit。
    I16,
    /// 打包 24-bit（3 字节 LE）。
    I24,
    /// 有符号 32-bit PCM。
    I32,
    /// 无符号 8-bit。
    U8,
}

/// 样本类型只描述字节布局；数值归一化规则由解码和编码函数分别负责。
impl SampleKind {
    /// 返回一个交错样本占用的字节数，供帧边界计算使用。
    pub fn bytes(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::I16 => 2,
            Self::I24 => 3,
            Self::U8 => 1,
        }
    }
}

/// 交错 PCM 的逻辑格式；声道数和采样率必须非零，样本按 `kind` 紧密排列。
#[derive(Clone, Copy, Debug)]
pub struct AudioFormat {
    /// 采样率 Hz；重采样步长由源与目标采样率之比计算。
    pub sample_rate: u32,
    /// 每帧交错样本数，决定帧边界和声道布局。
    pub channels: u16,
    /// 每个交错样本的字节布局及解码规则。
    pub kind: SampleKind,
}

/// 负责校验 WASAPI 格式字段，并把设备描述转换成内部 PCM 约定。
impl AudioFormat {
    /// 计算一帧所有声道的字节数；不完整的末尾帧由调用方截断。
    pub fn frame_bytes(self) -> usize {
        self.channels as usize * self.kind.bytes()
    }

    /// 从基础或扩展 WAVEFORMATEX 识别样本类型，无法安全解释时返回 None。
    pub fn from_wave(fmt: &WAVEFORMATEX) -> Option<Self> {
        let kind = match fmt.wFormatTag {
            WAVE_FORMAT_IEEE_FLOAT if fmt.wBitsPerSample == 32 => SampleKind::F32,
            WAVE_FORMAT_PCM => match fmt.wBitsPerSample {
                8 => SampleKind::U8,
                16 => SampleKind::I16,
                24 => SampleKind::I24,
                32 => SampleKind::I32,
                _ => return None,
            },
            WAVE_FORMAT_EXTENSIBLE => {
                if (fmt.cbSize as usize)
                    < size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()
                {
                    return None;
                }
                // SAFETY: WASAPI 仅在 cbSize 足够时提供完整扩展尾部，此处按同一布局读取。
                let ext = unsafe { &*(fmt as *const WAVEFORMATEX as *const WAVEFORMATEXTENSIBLE) };
                let sub = ext.SubFormat;
                if sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT && fmt.wBitsPerSample == 32 {
                    SampleKind::F32
                } else if sub == KSDATAFORMAT_SUBTYPE_PCM {
                    match fmt.wBitsPerSample {
                        8 => SampleKind::U8,
                        16 => SampleKind::I16,
                        24 => SampleKind::I24,
                        32 => SampleKind::I32,
                        _ => return None,
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        // 采样率和声道为零时无法定义帧边界，宁可让上层走格式回退。
        if fmt.nChannels == 0 || fmt.nSamplesPerSec == 0 {
            return None;
        }
        Some(Self {
            sample_rate: fmt.nSamplesPerSec,
            channels: fmt.nChannels,
            kind,
        })
    }
}

/// 捕获格式 → 渲染 f32。重采样分数和残余帧留在捕获线程。
pub struct Converter {
    /// 输入 PCM 的采样率、声道数和样本编码。
    src: AudioFormat,
    /// 输出的采样率与声道布局；转换结果统一以 f32 承载。
    dst: AudioFormat,
    /// 重采样跨包暂存的目标声道帧，至少保留插值锚点。
    hold: Vec<f32>,
    /// 当前插值位置，以 `hold` 的帧为单位并跨包保留小数部分。
    pos: f64,
    /// 复用的输入解码缓冲，避免每包重新分配。
    decoded: Vec<f32>,
    /// 复用的声道布局转换缓冲，供重采样读取。
    mixed: Vec<f32>,
}

/// 转换器持有跨包状态；捕获线程独占它，避免重采样分数被并发修改。
impl Converter {
    /// 将已是 f32 的输入送入声道混合和重采样管线，供多输出 fan-out 复用。
    pub fn convert_f32(&mut self, samples: &[f32], frames: usize, out: &mut Vec<f32>) {
        out.clear();
        self.decoded.clear();
        self.decoded.extend_from_slice(
            &samples[..samples
                .len()
                .min(frames.saturating_mul(self.src.channels as usize))],
        );
        self.mixed.clear();
        mix_channels(
            &self.decoded,
            self.src.channels as usize,
            self.dst.channels as usize,
            &mut self.mixed,
        );
        // 声道混合先于重采样，保证插值始终在目标声道布局上进行。
        if self.src.sample_rate == self.dst.sample_rate {
            out.extend_from_slice(&self.mixed);
            return;
        }
        self.hold.extend_from_slice(&self.mixed);
        let dst_ch = self.dst.channels as usize;
        let src_frames = self.hold.len() / dst_ch;
        if src_frames < 2 {
            return;
        }
        // `step` 是源帧/目标帧比；`pos` 跨包保留小数部分以避免时钟漂移。
        let step = f64::from(self.src.sample_rate) / f64::from(self.dst.sample_rate);
        while self.pos + 1.0 < src_frames as f64 {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            let a = i * dst_ch;
            let b = (i + 1) * dst_ch;
            for ch in 0..dst_ch {
                out.push(self.hold[a + ch] * (1.0 - frac) + self.hold[b + ch] * frac);
            }
            self.pos += step;
        }
        // 已经安全消费的完整源帧从 hold 移除，保留最后一帧作为下一包插值锚点。
        let drop_frames = self.pos.floor() as usize;
        let drop = drop_frames.min(src_frames.saturating_sub(1)) * dst_ch;
        if drop > 0 {
            self.hold.drain(..drop);
            self.pos -= drop_frames.min(src_frames.saturating_sub(1)) as f64;
        }
    }
    /// 创建带有预分配 decode/mix 缓冲的转换器，避免捕获循环首次扩容。
    pub fn new(src: AudioFormat, dst: AudioFormat) -> Self {
        Self {
            src,
            dst,
            hold: Vec::with_capacity(64),
            pos: 0.0,
            decoded: Vec::with_capacity(8192),
            mixed: Vec::with_capacity(8192),
        }
    }

    /// 解码一包 PCM；静音包直接生成零样本，避免访问 WASAPI 的空数据指针。
    pub fn convert(&mut self, bytes: &[u8], frames: usize, silent: bool, out: &mut Vec<f32>) {
        out.clear();
        let src_ch = self.src.channels as usize;
        let dst_ch = self.dst.channels as usize;
        // decoded 与 mixed 是线程私有暂存区，循环复用容量而不是不断分配。
        self.decoded.clear();
        if silent {
            self.decoded.resize(frames.saturating_mul(src_ch), 0.0);
        } else {
            decode_to_f32(bytes, frames, self.src, &mut self.decoded);
        }

        self.mixed.clear();
        mix_channels(&self.decoded, src_ch, dst_ch, &mut self.mixed);

        if self.src.sample_rate == self.dst.sample_rate {
            out.extend_from_slice(&self.mixed);
            return;
        }

        // 跨包保留末尾帧，确保任意包边界都能连续线性插值。
        self.hold.extend_from_slice(&self.mixed);
        let src_frames = self.hold.len() / dst_ch;
        if src_frames < 2 {
            return;
        }
        // 线性插值的步长由真实设备采样率决定，不假设固定 48 kHz。
        let step = f64::from(self.src.sample_rate) / f64::from(self.dst.sample_rate);
        while self.pos + 1.0 < src_frames as f64 {
            let i = self.pos.floor() as usize;
            let frac = (self.pos - i as f64) as f32;
            let a = i * dst_ch;
            let b = (i + 1) * dst_ch;
            for ch in 0..dst_ch {
                let s = self.hold[a + ch] * (1.0 - frac) + self.hold[b + ch] * frac;
                out.push(s);
            }
            self.pos += step;
        }
        // 丢弃已输出帧后修正位置；保留至少一帧供下一次插值。
        let drop_frames = self.pos.floor() as usize;
        let drop = drop_frames.min(src_frames.saturating_sub(1)) * dst_ch;
        if drop > 0 {
            self.hold.drain(..drop);
            self.pos -= drop_frames.min(src_frames.saturating_sub(1)) as f64;
        }
        // 异常长时间没有可消费进度时清空历史，防止坏时钟让 hold 无限增长。
        if self.hold.len() > dst_ch * 48_000 {
            self.hold.clear();
            self.pos = 0.0;
        }
    }
}

/// 按输入格式读取交错字节并归一化到 [-1, 1] 的 f32，短包只解码完整帧。
fn decode_to_f32(bytes: &[u8], frames: usize, fmt: AudioFormat, out: &mut Vec<f32>) {
    let ch = fmt.channels as usize;
    let need = frames * fmt.frame_bytes();
    let bytes = if bytes.len() < need {
        bytes
    } else {
        &bytes[..need]
    };
    let frames = bytes.len() / fmt.frame_bytes();
    // reserve 只发生在捕获线程预热阶段；调用方通常已提供足够容量。
    out.reserve(frames * ch);
    match fmt.kind {
        SampleKind::F32 => {
            for chunk in bytes.as_chunks::<4>().0.iter().take(frames * ch) {
                out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        SampleKind::I16 => {
            for chunk in bytes.as_chunks::<2>().0.iter().take(frames * ch) {
                let v = i16::from_le_bytes([chunk[0], chunk[1]]);
                out.push(f32::from(v) / 32768.0);
            }
        }
        SampleKind::I32 => {
            for chunk in bytes.as_chunks::<4>().0.iter().take(frames * ch) {
                let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                out.push(v as f32 / 2147483648.0);
            }
        }
        SampleKind::I24 => {
            // 24 位补码先扩展符号位到 i32，再按其负向满量程 2^23 归一化。
            for chunk in bytes.as_chunks::<3>().0.iter().take(frames * ch) {
                let mut b = [0u8; 4];
                b[0] = chunk[0];
                b[1] = chunk[1];
                b[2] = chunk[2];
                if chunk[2] & 0x80 != 0 {
                    b[3] = 0xFF;
                }
                let v = i32::from_le_bytes(b);
                out.push(v as f32 / 8388608.0);
            }
        }
        SampleKind::U8 => {
            // 无符号 8 位 PCM 以 128 为零点，先居中再缩放到浮点幅度。
            for &b in bytes.iter().take(frames * ch) {
                out.push((f32::from(b) - 128.0) / 128.0);
            }
        }
    }
}

/// 将源声道布局变换为目标布局：单声道复制、多声道折叠，其余声道补零。
fn mix_channels(src: &[f32], src_ch: usize, dst_ch: usize, out: &mut Vec<f32>) {
    if src_ch == 0 || dst_ch == 0 {
        return;
    }
    // 相同布局无需复制之外的混音计算，保留原始交错顺序。
    if src_ch == dst_ch {
        out.extend_from_slice(src);
        return;
    }
    let frames = src.len() / src_ch;
    out.reserve(frames * dst_ch);
    // 每次只处理完整源帧，避免把交错声道错位到下一帧。
    for frame in 0..frames {
        let base = frame * src_ch;
        if src_ch == 1 {
            let s = src[base];
            for _ in 0..dst_ch {
                out.push(s);
            }
        } else if dst_ch == 1 {
            let mut sum = 0.0;
            for ch in 0..src_ch {
                sum += src[base + ch];
            }
            out.push(sum / src_ch as f32);
        } else {
            for ch in 0..dst_ch {
                if ch < src_ch {
                    out.push(src[base + ch]);
                } else {
                    out.push(0.0);
                }
            }
        }
    }
}

/// 把归一化 f32 限幅并编码为 WASAPI 设备所需的 PCM 字节布局。
pub fn encode_from_f32(src: &[f32], kind: SampleKind, dst: &mut [u8]) {
    // 所有整数格式都先限幅，避免上层混音溢出污染设备缓冲区。
    match kind {
        SampleKind::F32 => {
            for (i, sample) in src.iter().enumerate() {
                let bytes = sample.clamp(-1.0, 1.0).to_le_bytes();
                let o = i * 4;
                if o + 4 <= dst.len() {
                    dst[o..o + 4].copy_from_slice(&bytes);
                }
            }
        }
        SampleKind::I16 => {
            for (i, sample) in src.iter().enumerate() {
                // 整数编码使用正向最大值缩放，避免把 +1.0 转成不可表示的正端点。
                let v = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                let o = i * 2;
                if o + 2 <= dst.len() {
                    dst[o..o + 2].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
        SampleKind::I32 => {
            for (i, sample) in src.iter().enumerate() {
                let v = (sample.clamp(-1.0, 1.0) * 2147483647.0) as i32;
                let o = i * 4;
                if o + 4 <= dst.len() {
                    dst[o..o + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
        SampleKind::U8 => {
            for (i, sample) in src.iter().enumerate() {
                if i < dst.len() {
                    dst[i] = (sample.clamp(-1.0, 1.0) * 127.0 + 128.0) as u8;
                }
            }
        }
        SampleKind::I24 => {
            for (i, sample) in src.iter().enumerate() {
                let v = (sample.clamp(-1.0, 1.0) * 8388607.0) as i32;
                let o = i * 3;
                if o + 3 <= dst.len() {
                    let b = v.to_le_bytes();
                    dst[o] = b[0];
                    dst[o + 1] = b[1];
                    dst[o + 2] = b[2];
                }
            }
        }
    }
}
