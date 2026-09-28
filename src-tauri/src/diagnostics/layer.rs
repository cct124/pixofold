//! 只接收pixofold诊断目标和白名单字段；第三方日志、message、路径、凭据正文一律不收集。
use super::Logger;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

#[derive(Default, Clone)]
struct Fields(Map<String, Value>);
impl Fields {
    fn accepts(field: &Field) -> bool {
        matches!(
            field.name(),
            "event"
                | "batch_id"
                | "selection_id"
                | "job_id"
                | "attempt"
                | "mode"
                | "quality"
                | "output_policy"
                | "metadata_policy"
                | "stage"
                | "result"
                | "error_code"
                | "io_kind"
                | "elapsed_ms"
                | "input_bytes"
                | "output_bytes"
                | "reserved_bytes"
                | "workers"
                | "cpu_threads"
                | "budget_bytes"
                | "count"
                | "revision"
                | "credentials_removed"
                | "succeeded"
                | "failed"
                | "no_gain"
                | "cancelled"
                | "source_unchanged"
                | "operation"
                | "subscription_id"
                | "exit_code"
        )
    }
    fn insert(&mut self, field: &Field, value: Value) {
        if Self::accepts(field) {
            self.0.insert(field.name().into(), value);
        }
    }
}
impl Visit for Fields {
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, json!(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, json!(value));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if !Self::accepts(field) {
            return;
        }
        let safe: String = value.chars().take(256).collect();
        self.insert(field, Value::String(safe));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // 调用点只允许无路径枚举；不记录任意Debug错误对象/请求/快照。
        if !Self::accepts(field) {
            return;
        }
        use std::fmt::Write;
        let mut bounded = BoundedDebug(String::new());
        let _ = write!(bounded, "{value:?}");
        self.insert(field, Value::String(bounded.0));
    }
}

struct BoundedDebug(String);
impl std::fmt::Write for BoundedDebug {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let remaining = 256_usize.saturating_sub(self.0.chars().count());
        self.0.extend(value.chars().take(remaining));
        if value.chars().count() > remaining {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

pub(super) struct EventLayer(pub Arc<Logger>);
impl<S: Subscriber + for<'lookup> LookupSpan<'lookup>> Layer<S> for EventLayer {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().target() != "pixofold" {
            return;
        }
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(fields);
        }
    }
    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(fields) = span.extensions_mut().get_mut::<Fields>()
        {
            values.record(fields);
        }
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != "pixofold" {
            return;
        }
        let mut fields = Fields::default();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(parent) = span.extensions().get::<Fields>() {
                    fields.0.extend(parent.0.clone());
                }
            }
        }
        event.record(&mut fields);
        if fields.0.contains_key("event") {
            self.0
                .event(event.metadata().level().as_str(), Value::Object(fields.0));
        }
    }
}
