//! Miccat 组合根：无参数开 UI；`--list-*` / `--capture` / `--route` 走 CLI。

mod audio;
mod cli;
mod config;
mod diagnostics;
mod engine;
mod error;
mod hotkey;
mod library;
mod platform;
mod presentation;
mod process;
mod ui_bridge;
mod vbcable;

/// 初始化进程级 DPI、日志与配置后，转交 CLI 或 Slint UI 执行。
fn main() {
    // 声明 Per-Monitor V2 DPI 感知。不声明时系统会把窗口位图拉伸到 150%，界面发虚，
    // 而且 Win32 返回的工作区是虚拟化坐标，窗口就会算得比屏幕还大、底部跑到屏幕外。
    // 必须在创建 Slint 窗口前设置进程级上下文；返回值仅表示系统是否接受请求，不能改变后续清理流程。
    // SAFETY: 仅设置当前进程的 DPI 上下文，不借用指针，也不跨线程访问窗口对象。
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }

    // 先安装基础 subscriber，再读取配置，确保配置损坏或日志目录不可写等告警也进入问题中心。
    diagnostics::install_logging();
    // 配置中的级别覆盖默认过滤器；CLI 与 UI 因此共享同一套日志开关。
    // 先读配置，让 CLI 路径也尊重用户保存的日志级别（配置损坏的告警也会被捕获）。
    let saved = config::load();
    diagnostics::apply_levels(&diagnostics::Levels::from_codes(
        saved.debug.master,
        &saved.debug.categories,
    ));
    // 启动日志记录最终生效的版本和文件位置，便于用户从问题报告定位运行实例。
    tracing::info!(
        "Miccat {} 启动，日志文件 {}",
        env!("CARGO_PKG_VERSION"),
        diagnostics::log_path().display()
    );
    // CLI 统一承接命令行与无参数 UI 分派；错误在边界转为 stderr 和非零退出码。

    if let Err(e) = cli::run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
