//! ProcessMic：无参数开 UI；`--list-*` / `--capture` / `--route` 走 CLI。

mod audio;
mod cli;
mod config;
mod engine;
mod error;
mod hotkey;
mod library;
mod platform;
mod process;
mod ui_bridge;
mod vbcable;

fn main() {
    // 声明 Per-Monitor V2 DPI 感知。不声明时系统会把窗口位图拉伸到 150%，界面发虚，
    // 而且 Win32 返回的工作区是虚拟化坐标，窗口就会算得比屏幕还大、底部跑到屏幕外。
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }

    // RUST_LOG 可覆盖；默认 info，音频热路径不打日志。
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    if let Err(e) = cli::run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
