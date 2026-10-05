//! ProcessMic：无参数开 UI；`--list-*` / `--capture` / `--route` 走 CLI。

mod audio;
mod cli;
mod config;
mod diagnostics;
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

    // 控制台固定 info；日志文件按运行时开关走；问题中心捕获 WARN/ERROR。
    diagnostics::install_logging();
    // 先读配置，让 CLI 路径也尊重用户保存的日志级别（配置损坏的告警也会被捕获）。
    let saved = config::load();
    diagnostics::apply_levels(&diagnostics::Levels::from_codes(
        saved.debug.master,
        &saved.debug.categories,
    ));
    tracing::info!(
        "ProcessMic {} 启动，日志文件 {}",
        env!("CARGO_PKG_VERSION"),
        diagnostics::log_path().display()
    );

    if let Err(e) = cli::run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
