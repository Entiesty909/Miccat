//! tracing Layer：把 WARN/ERROR 事件无条件写入诊断中心。

use super::hub::{self, Severity};

/// 捕获 WARN/ERROR 到全局诊断中心；INFO/DEBUG 不进入问题中心。
pub(crate) struct HubLayer;

/// 事件回调只把问题级别交给诊断中心，绕过运行时日志过滤开关。

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for HubLayer {
    /// 将 tracing WARN/ERROR 转为用户可见诊断；其他级别继续留给日志层处理。

    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // HubLayer 不受 EnvFilter 控制，故明确只接收问题级别，确保用户能看到故障。

        let severity = match *event.metadata().level() {
            tracing::Level::ERROR => Severity::Error,
            tracing::Level::WARN => Severity::Warn,
            _ => return,
        };

        // Visit 先收集字段，再统一生成标题和详情，避免在 tracing 回调中重复上报。

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        // target 是稳定的模块来源；去掉 crate 前缀后，列表文案更短且便于定位。

        let target = event.metadata().target();
        // 桥接层自己用 `report_error` 上报用户短句与建议，避免技术标题重复。
        if target.starts_with("mintcat_proc_mic::ui_bridge") {
            return;
        }
        let source = target
            .strip_prefix("mintcat_proc_mic::")
            .filter(|rest| !rest.is_empty())
            .unwrap_or(target);

        // 标题来自 message，详情保留结构化字段；空 hint 让桥接层继续负责用户建议。
        hub::shared().report(severity, source, &visitor.title, &visitor.detail, "");
    }
}

/// 收集事件字段：`message` 作为标题，其余字段拼成 `key=value` 作为详情。
#[derive(Default)]
struct EventVisitor {
    /// `message` 字段形成标题，其余字段按出现顺序拼入详情。
    title: String,
    /// 附加字段的可读文本，多个字段之间用逗号分隔。
    detail: String,
}

/// 按 tracing 字段协议把 message 与附加字段拆成诊断标题和详情。

impl EventVisitor {
    /// 追加一个非 `message` 字段，多个字段以 `, ` 分隔。
    fn push_field(&mut self, name: &str, value: &str) {
        // 详情字段共享一行文本，分隔符让多个结构化字段仍能直接复制阅读。
        if !self.detail.is_empty() {
            self.detail.push_str(", ");
        }
        self.detail.push_str(name);
        self.detail.push('=');
        self.detail.push_str(value);
    }
}

/// 兼容字符串和 Debug 值，确保结构化 tracing 字段不会丢失。

impl tracing::field::Visit for EventVisitor {
    /// 字符串字段直接保留，message 作为标题、其他字段进入详情。

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.title.push_str(value);
        } else {
            self.push_field(field.name(), value);
        }
    }

    /// 非字符串字段使用 Debug 格式，保持 HRESULT、数字和结构体信息可复制。

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.title.push_str(&format!("{value:?}"));
        } else {
            self.push_field(field.name(), &format!("{value:?}"));
        }
    }
}
