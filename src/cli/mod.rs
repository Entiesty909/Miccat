//! CLI 参数解析、设备/进程命令和播放探针。

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::engine::AudioEngine;
use crate::error::Error;
use crate::{audio, process, ui_bridge};

/// 解析 argv。无参数启动 Slint。
pub(crate) fn run() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return ui_bridge::run_ui();
    }
    match args[0].as_str() {
        "--list-processes" => {
            for p in process::list_processes()? {
                println!("{}", p.label());
            }
        }
        "--list-devices" => {
            for d in audio::device::list_destinations(true)? {
                println!("{}\t{}", d.render_id, d.label);
            }
        }
        "--capture" => {
            let pid = parse_flag_u32(&args, "--capture")?;
            let wav = parse_flag_str(&args, "--wav").unwrap_or("capture.wav");
            let seconds = parse_flag_f32(&args, "--seconds").unwrap_or(5.0);
            println!("capturing pid {pid} for {seconds}s -> {wav}");
            audio::dump::capture_process_to_wav(pid, &PathBuf::from(wav), seconds)?;
            println!("wrote {wav}");
        }
        "--route" => {
            let pid = parse_flag_u32(&args, "--route")?;
            let device = parse_flag_str(&args, "--device")
                .ok_or_else(|| Error::InvalidArgs("missing --device <id>".into()))?;
            let seconds = parse_flag_f32(&args, "--seconds").unwrap_or(8.0);
            let engine = AudioEngine::new();
            engine.open_output(device)?;
            engine.set_process_source(pid)?;
            println!("routing pid {pid} for {seconds}s");
            thread::sleep(Duration::from_secs_f32(seconds.max(0.5)));
            engine.stop();
            println!("stopped ({:?})", engine.status());
        }
        "--help" | "-h" => print_help(),
        "--snapshot-ui" => {
            let out = parse_flag_str(&args, "--out")
                .unwrap_or("snap.raw")
                .to_string();
            let menu = args_has(&args, "--menu");
            let category_menu = args_has(&args, "--category-menu");
            let category_row_menu = args_has(&args, "--category-row-menu");
            let source_dialog_tab = match parse_flag_str(&args, "--source-dialog-tab") {
                Some("process") => 0,
                Some("audio") => 1,
                Some("microphone") => 2,
                Some(_) => {
                    return Err(Error::InvalidArgs("unknown source dialog tab".into()));
                }
                None => -1,
            };
            let source_dialog_click_tab = match parse_flag_str(&args, "--source-dialog-click-tab") {
                Some("process") => 0,
                Some("audio") => 1,
                Some("microphone") => 2,
                Some(_) => {
                    return Err(Error::InvalidArgs(
                        "unknown source dialog click target".into(),
                    ));
                }
                None => -1,
            };
            let source_dialog_reopen = args_has(&args, "--source-dialog-reopen");
            let source_menu_click = args_has(&args, "--source-menu-click");
            let source_meter = args_has(&args, "--source-meter");
            let page = parse_flag_str(&args, "--page").unwrap_or("player");
            let diagnostics = args_has(&args, "--diagnostics");
            let diagnostics_log = args_has(&args, "--diagnostics-log");
            let diagnostics_badge = args_has(&args, "--diagnostics-badge");
            let diagnostics_live = args_has(&args, "--diagnostics-live");
            let settings_section = parse_flag_u32(&args, "--section")
                .map(|value| value as i32)
                .unwrap_or(-1);
            ui_bridge::snapshot_to_file(ui_bridge::SnapshotOptions {
                path: &out,
                page,
                entry_menu: menu,
                empty_category_menu: category_menu,
                category_row_menu,
                source_dialog_tab,
                source_dialog_click_tab,
                source_dialog_reopen,
                source_menu_click,
                source_meter,
                diagnostics,
                diagnostics_log,
                diagnostics_badge,
                diagnostics_live,
                settings_section,
            })?;
        }
        "--probe-playback" => {
            let file = parse_flag_str(&args, "--probe-playback")
                .ok_or_else(|| Error::InvalidArgs("missing audio file".into()))?;
            let seconds = parse_flag_f32(&args, "--seconds").unwrap_or(8.0);
            let start = parse_flag_f32(&args, "--start").unwrap_or(0.0);
            let out = parse_flag_str(&args, "--out")
                .unwrap_or("probe.wav")
                .to_string();
            let sfx = args.iter().any(|a| a == "--sfx");
            probe_playback(&PathBuf::from(file), seconds, start, &out, sfx)?;
        }
        other => {
            eprintln!("unknown argument: {other}");
            print_help();
            return Err(Error::InvalidArgs("unknown argument".into()));
        }
    }
    Ok(())
}

/// 参数里是否出现该 flag。
fn args_has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// 临时验证：按设备时钟消费播放源，落 WAV 并与原文件逐采样比对。
fn probe_playback(
    path: &std::path::Path,
    seconds: f32,
    start_secs: f32,
    out: &str,
    use_sfx: bool,
) -> Result<(), Error> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32};
    use std::time::Instant;

    use audio::buffer::SpscRing;
    use audio::format::{AudioFormat, SampleKind};

    let reference = audio::decode::open(path)?.read_all()?;
    let rate = reference.sample_rate;
    let channels = reference.channels;
    let target = AudioFormat {
        sample_rate: rate,
        channels,
        kind: SampleKind::F32,
    };
    let start_hns = (start_secs.max(0.0) as f64 * 10_000_000.0) as u64;
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let capacity = ((rate as usize).saturating_mul(channels as usize) / 2).max(8192);
    let ring = Arc::new(SpscRing::with_capacity_samples(capacity));
    let (info, rx) =
        audio::decode::stream_to_channel(path.to_path_buf(), start_hns, false, cancel)?;
    if args_has(&std::env::args().collect::<Vec<_>>(), "--raw") {
        let mut got: Vec<f32> = Vec::new();
        while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(500)) {
            got.extend_from_slice(&chunk.samples);
            if chunk.done {
                break;
            }
        }
        let compare = got.len().min(reference.samples.len());
        let mut first = None;
        let mut mismatch = 0usize;
        for (i, sample) in got.iter().enumerate().take(compare) {
            if (*sample - reference.samples[i]).abs() > 1e-3 {
                mismatch += 1;
                if first.is_none() {
                    first = Some(i);
                }
            }
        }
        println!(
            "source=raw 解码 {} 采样；首个不匹配 {:?}；不匹配 {}/{}",
            got.len(),
            first,
            mismatch,
            compare
        );
        return Ok(());
    }
    let mut keep_tx = None;
    let source = if use_sfx {
        let (tx, commands) = audio::sfx::channel();
        let ring_mixer = vec![ring.clone()];
        let stop_mixer = stop.clone();
        let peak = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let states = Arc::new(vec![AtomicU8::new(0)]);
        let source = thread::spawn(move || {
            audio::sfx::run_sfx_loop(ring_mixer, target, stop_mixer, peak, states, commands)
        });
        tx.send(audio::sfx::SfxCommand::PlayStream {
            slot: 0,
            sample_rate: rate,
            channels,
            volume: 1.0,
            rx,
        })
        .map_err(|_| Error::RenderInit("probe: sfx channel closed"))?;
        keep_tx = Some(tx);
        Some(source)
    } else {
        let ring_source = ring.clone();
        let stop_source = stop.clone();
        Some(thread::spawn(move || {
            audio::preview::run_stream_source(ring_source, target, info, rx, false, stop_source)
        }))
    };

    // 设备时钟：每 10 ms 取一块，余量不足的记录为空取次数。
    let frames = (rate as usize / 100).max(64);
    let block_samples = frames * channels as usize;
    let mut block = vec![0.0f32; block_samples];
    let mut got: Vec<f32> = Vec::with_capacity(rate as usize * channels as usize * 2);
    let mut short = 0usize;
    let start = Instant::now();
    let mut next = start;
    while start.elapsed().as_secs_f32() < seconds {
        next += Duration::from_millis(10);
        let n = ring.pop(&mut block);
        if n < block_samples {
            short += 1;
        }
        got.extend_from_slice(&block[..n]);
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        } else {
            next = now;
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Release);

    // 丢弃生产线程启动时写入的静音，再按 seek 附近的样本对齐。
    let leading_silence = got
        .iter()
        .position(|sample| sample.abs() > 1e-5)
        .unwrap_or(0);
    let captured = &got[leading_silence..];
    let skip_frames = (start_hns as u128 * rate as u128 / 10_000_000) as usize;
    let skip = skip_frames * channels as usize;
    let probe_len = (channels as usize * 2000).min(captured.len());
    let search_samples = (rate as usize * channels as usize).max(channels as usize * 4096);
    let mut offset = skip;
    if probe_len > 0 {
        let mut best = f64::MAX;
        let low = skip.saturating_sub(search_samples);
        let high = (skip + search_samples).min(reference.samples.len());
        let mut candidate = low;
        while candidate + probe_len <= high {
            let err: f64 = (0..probe_len)
                .step_by(channels as usize * 7)
                .map(|i| (captured[i] - reference.samples[candidate + i]).abs() as f64)
                .sum();
            if err < best {
                best = err;
                offset = candidate;
            }
            candidate += channels as usize;
        }
    }
    let compare = captured
        .len()
        .min(reference.samples.len().saturating_sub(offset));
    let mut first = None;
    let mut mismatch = 0usize;
    for (i, sample) in captured.iter().enumerate().take(compare) {
        if (*sample - reference.samples[offset + i]).abs() > 1e-3 {
            mismatch += 1;
            if first.is_none() {
                first = Some(i);
            }
        }
    }
    let drift = offset as i64 - skip as i64;
    println!(
        "source={} 起始 {:.1}s 对齐帧 {}/{}（偏移 {} 采样）；消费 {:.2}s / {} 采样；空取 {} 次；首个不匹配 {:?}；不匹配 {}/{}",
        if use_sfx { "sfx" } else { "preview" },
        start_secs,
        offset / channels as usize,
        skip / channels as usize,
        drift,
        got.len() as f32 / (rate as f32 * channels as f32),
        got.len(),
        short,
        first,
        mismatch,
        compare
    );
    audio::wav::write_pcm16_wav(std::path::Path::new(out), rate, channels, &got)?;
    let ref_out = format!("{out}.ref.wav");
    audio::wav::write_pcm16_wav(
        std::path::Path::new(&ref_out),
        rate,
        channels,
        &reference.samples,
    )?;
    println!("写出 {out} / {ref_out}");
    drop(keep_tx);
    if let Some(handle) = source {
        let _ = handle.join();
    }
    Ok(())
}

/// CLI 用法。
fn print_help() {
    eprintln!(
        "miccat\n\
         \n\
         (no args)                         launch UI\n\
         --list-processes\n\
         --list-devices\n\
         --capture <pid> --wav <path> --seconds N\n\
         --route <pid> --device <id> --seconds N\n\
         --snapshot-ui --out <path> [--page route|player|settings] [--source-dialog-tab ...] [--source-dialog-click-tab ...] [--source-dialog-reopen] [--source-menu-click] [--source-meter] [菜单参数]\
         --probe-audio --device <id> [--mode reopen|monitor|multi] [--device2 <id>]\
         --probe-playback <file> [--seconds N] [--start N] [--out path]"
    );
}

/// 取 `--flag value` 的 value。
fn parse_flag_str<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].as_str())
}

/// 解析 PID。
fn parse_flag_u32(args: &[String], flag: &str) -> Result<u32, Error> {
    parse_flag_str(args, flag)
        .ok_or_else(|| Error::InvalidArgs("missing pid".into()))?
        .parse()
        .map_err(|_| Error::InvalidArgs("pid must be u32".into()))
}

/// 可选浮点 flag，解析失败当没传。
fn parse_flag_f32(args: &[String], flag: &str) -> Option<f32> {
    parse_flag_str(args, flag)?.parse().ok()
}
