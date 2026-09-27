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

/// Process Loopback → PCM16 WAV。不经过 ring / render。
pub(crate) fn capture_process_to_wav(pid: u32, path: &Path, seconds: f32) -> Result<(), Error> {
    let _com = com::init_mta()?;
    let pcm16 = pcm16_wave(44_100);
    let capture: CaptureClient = open_process_capture(pid, Some(&pcm16))?;

    let src_format = capture.format;
    let mut converter = Converter::new(
        src_format,
        AudioFormat {
            sample_rate: src_format.sample_rate,
            channels: src_format.channels,
            kind: SampleKind::F32,
        },
    );

    unsafe { capture.client.Start()? };

    let mut collected = Vec::new();
    let mut converted = Vec::with_capacity(8192);
    let deadline = Instant::now() + Duration::from_secs_f32(seconds.max(0.2));
    while Instant::now() < deadline {
        let wait = unsafe { WaitForSingleObject(capture.event, 80) };
        if wait == WAIT_TIMEOUT {
            continue;
        }
        if wait != WAIT_OBJECT_0 {
            break;
        }
        loop {
            let packet = unsafe { capture.capture.GetNextPacketSize()? };
            if packet == 0 {
                break;
            }
            let mut data: *mut u8 = ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            unsafe {
                capture
                    .capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            }
            let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
            let nbytes = frames as usize * src_format.frame_bytes();
            let bytes = if data.is_null() || silent {
                &[][..]
            } else {
                unsafe { std::slice::from_raw_parts(data, nbytes) }
            };
            converter.convert(
                bytes,
                frames as usize,
                silent || data.is_null(),
                &mut converted,
            );
            collected.extend_from_slice(&converted);
            unsafe { capture.capture.ReleaseBuffer(frames)? };
        }
    }

    unsafe {
        let _ = capture.client.Stop();
        let _ = CloseHandle(capture.event);
    }

    wav::write_pcm16_wav(
        path,
        src_format.sample_rate,
        src_format.channels,
        &collected,
    )
}
