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
    // 先覆盖全局默认级别，再逐项恢复分类设置，使持久化配置完整重现。
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

    // 初始级别先用于构造可热替换的文件过滤器；句柄由 Control 保存到进程结束。

    let levels = Levels::default();
    let (file_filter, handle) = reload::Layer::new(EnvFilter::new(levels.filter_string()));
    // 控制台沿用环境变量，便于开发期查看；文件过滤器则由界面分类开关单独管理。

    let console = fmt::layer()
        .with_target(false)
        .with_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")));
    // 文件不可创建时仍保留控制台和 HubLayer，诊断中心不能因日志落盘失败而消失。

    let file = log::make_writer().ok().map(|writer| {
        fmt::layer()
            .with_ansi(false)
            .with_target(true)
            .with_writer(move || writer.clone())
            .with_filter(file_filter)
    });
    // Layer 组合顺序决定同一事件可同时输出到控制台、文件和问题中心。

    let _ = registry()
        .with(console)
        .with(file)
        .with(layer::HubLayer)
        .try_init();
    // 重载回调只接收新的过滤器，不持有 UI 或文件锁，避免运行时调整形成锁环。

    let reload = Box::new(move |filter: EnvFilter| handle.reload(filter).is_ok());
    let _ = CONTROL.set(Control::new(levels, reload));
}
