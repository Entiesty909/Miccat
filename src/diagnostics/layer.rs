//! tracing Layer：把 WARN/ERROR 事件无条件写入诊断中心。

use super::hub::{self, Severity};

/// 捕获 WARN/ERROR 到全局诊断中心；INFO/DEBUG 不进入问题中心。
pub(crate) struct HubLayer;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for HubLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let severity = match *event.metadata().level() {
            tracing::Level::ERROR => Severity::Error,
            tracing::Level::WARN => Severity::Warn,
            _ => return,
        };

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let target = event.metadata().target();
        // 桥接层自己用 `report_error` 上报用户短句与建议，避免技术标题重复。
        if target.starts_with("mintcat_proc_mic::ui_bridge") {
            return;
        }
        let source = target
            .strip_prefix("mintcat_proc_mic::")
            .filter(|rest| !rest.is_empty())
            .unwrap_or(target);

        hub::shared().report(severity, source, &visitor.title, &visitor.detail, "");
    }
}

/// 收集事件字段：`message` 作为标题，其余字段拼成 `key=value` 作为详情。
#[derive(Default)]
struct EventVisitor {
    title: String,
    detail: String,
}

impl EventVisitor {
    /// 追加一个非 `message` 字段，多个字段以 `, ` 分隔。
    fn push_field(&mut self, name: &str, value: &str) {
        if !self.detail.is_empty() {
            self.detail.push_str(", ");
        }
        self.detail.push_str(name);
        self.detail.push('=');
        self.detail.push_str(value);
    }
}

impl tracing::field::Visit for EventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.title.push_str(value);
        } else {
            self.push_field(field.name(), value);
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.title.push_str(&format!("{value:?}"));
        } else {
            self.push_field(field.name(), &format!("{value:?}"));
        }
    }
}
