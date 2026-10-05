//! 全局诊断：运行时调试开关、tracing 捕获、日志落盘与用户可见问题中心。

mod control;
mod hub;
mod layer;
mod log;

use std::sync::{Arc, OnceLock};

pub(crate) use control::{CATEGORIES, Control, DebugLevel, Levels};
pub(crate) use hub::{Diagnostic, DiagnosticHub, Severity};

/// 安装后即可用的运行时调试开关。
///
/// 重载句柄只能在 `install_logging` 里拿到，初始化依赖运行期输入，
/// 因此保留 `OnceLock` 而不是 `LazyLock`。
static CONTROL: OnceLock<Control> = OnceLock::new();

/// 进程内全局诊断中心。
pub(crate) fn hub() -> &'static Arc<DiagnosticHub> {
    hub::shared()
}

/// 运行时调试开关；未安装日志时为 `None`。
pub(crate) fn control() -> Option<&'static Control> {
    CONTROL.get()
}

/// 一次性应用整组级别；未安装日志时静默忽略。
pub(crate) fn apply_levels(levels: &Levels) {
    let Some(control) = control() else {
        return;
    };
    control.set_master(levels.master);
    for (key, level) in &levels.overrides {
        control.set_category(key, Some(*level));
    }
}

/// 诊断日志文件路径；用于「打开日志」。
pub(crate) fn log_path() -> std::path::PathBuf {
    log::path()
}

/// 安装 tracing：控制台、可调级日志文件与诊断中心捕获。
///
/// 控制台固定 info；日志文件按运行时开关走；`HubLayer` 无条件捕获
/// WARN/ERROR，保证用户总能看见真实问题，不受开关影响。
pub(crate) fn install_logging() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer, fmt, registry, reload};

    let levels = Levels::default();
    let (file_filter, handle) = reload::Layer::new(EnvFilter::new(levels.filter_string()));
    let console = fmt::layer()
        .with_target(false)
        .with_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")));
    let file = log::make_writer().ok().map(|writer| {
        fmt::layer()
            .with_ansi(false)
            .with_target(true)
            .with_writer(move || writer.clone())
            .with_filter(file_filter)
    });
    let _ = registry()
        .with(console)
        .with(file)
        .with(layer::HubLayer)
        .try_init();
    let reload = Box::new(move |filter: EnvFilter| handle.reload(filter).is_ok());
    let _ = CONTROL.set(Control::new(levels, reload));
}
