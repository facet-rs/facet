//! Telemetry middleware for roam RPC.
//!
//! Creates spans for incoming RPC requests with W3C Trace Context propagation.

use std::future::Future;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use facet_pretty::PrettyPrinter;
use roam_session::{Context, MethodOutcome, Middleware, Rejection, SendPeek};

use crate::client::CurrentTrace;
use crate::exporter::SpanExporter;
use crate::otlp::{
    KeyValue, Span, SpanKind, Status, TraceContext, generate_span_id, generate_trace_id,
};

/// A span that's been started but not yet finished.
///
/// Stored in `ctx.extensions` during request processing.
#[derive(Debug, Clone)]
pub struct PendingSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub start_time_ns: u64,
    pub attributes: Vec<KeyValue>,
}

impl PendingSpan {
    /// Start a new span, extracting trace context from metadata if present.
    pub fn start(ctx: &Context) -> Self {
        let name = ctx.method_name().unwrap_or("unknown").to_string();

        // Try to extract trace context from metadata
        let trace_ctx = ctx
            .metadata()
            .iter()
            .find(|(k, _)| k == TraceContext::TRACEPARENT_KEY)
            .and_then(|(_, v)| match v {
                roam_wire::MetadataValue::String(s) => TraceContext::parse(s),
                _ => None,
            });

        let (trace_id, parent_span_id) = match trace_ctx {
            Some(tc) => (tc.trace_id, Some(tc.parent_span_id)),
            None => (generate_trace_id(), None),
        };

        let span_id = generate_span_id();

        let start_time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // Add standard attributes
        let mut attributes = vec![
            KeyValue::string("rpc.system", "roam"),
            KeyValue::string("rpc.method", &name),
            KeyValue::int("rpc.request_id", ctx.request_id().raw() as i64),
            KeyValue::int("network.peer.connection_id", ctx.conn_id().raw() as i64),
        ];

        // Add metadata as attributes (limited to avoid bloat)
        for (key, value) in ctx.metadata().iter().take(10) {
            if key != TraceContext::TRACEPARENT_KEY {
                let value_str = match value {
                    roam_wire::MetadataValue::String(s) => s.clone(),
                    roam_wire::MetadataValue::Bytes(b) => format!("<{} bytes>", b.len()),
                    roam_wire::MetadataValue::U64(n) => n.to_string(),
                };
                attributes.push(KeyValue::string(format!("rpc.metadata.{}", key), value_str));
            }
        }

        Self {
            trace_id,
            span_id,
            parent_span_id,
            name,
            start_time_ns,
            attributes,
        }
    }

    /// Finish the span and convert to an OTLP Span.
    pub fn finish(mut self, outcome: &MethodOutcome<'_>) -> Span {
        let end_time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let status = match outcome {
            MethodOutcome::Ok(_) => {
                self.attributes.push(KeyValue::bool("rpc.success", true));
                Status::ok()
            }
            MethodOutcome::Err(_) => {
                self.attributes.push(KeyValue::bool("rpc.success", false));
                self.attributes
                    .push(KeyValue::string("rpc.error_type", "user_error"));
                Status::error("user error")
            }
            MethodOutcome::Rejected => {
                self.attributes.push(KeyValue::bool("rpc.success", false));
                self.attributes
                    .push(KeyValue::string("rpc.error_type", "rejected"));
                Status::error("rejected by middleware")
            }
        };

        Span {
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            name: self.name,
            kind: SpanKind::Server.as_u32(),
            start_time_unix_nano: self.start_time_ns.to_string(),
            end_time_unix_nano: end_time_ns.to_string(),
            attributes: self.attributes,
            status,
        }
    }

    /// Get the traceparent header value for propagating to downstream calls.
    ///
    /// Use this when making outgoing RPC calls to propagate the trace.
    pub fn traceparent(&self) -> String {
        format!("00-{}-{}-01", self.trace_id, self.span_id)
    }
}

/// Telemetry middleware that creates spans for RPC requests.
///
/// # Trace Context Propagation
///
/// If the incoming request has a `traceparent` metadata key, the middleware
/// extracts the trace ID and creates a child span. Otherwise, it starts a
/// new trace.
///
/// The [`PendingSpan`] is stored in `ctx.extensions` and can be used to
/// propagate context to downstream calls via `pending_span.traceparent()`.
///
/// # Example
///
/// ```ignore
/// use roam_telemetry::{TelemetryMiddleware, OtlpExporter};
///
/// let exporter = OtlpExporter::new(
///     "http://tempo:4318/v1/traces",
///     "my-service"
/// );
/// let telemetry = TelemetryMiddleware::new(exporter);
///
/// let dispatcher = MyServiceDispatcher::new(handler)
///     .with_middleware(telemetry);
/// ```
#[derive(Clone)]
pub struct TelemetryMiddleware<E> {
    exporter: E,
}

impl<E: SpanExporter> TelemetryMiddleware<E> {
    /// Create a new telemetry middleware with the given exporter.
    pub fn new(exporter: E) -> Self {
        Self { exporter }
    }
}

impl<E: SpanExporter> Middleware for TelemetryMiddleware<E> {
    fn pre<'a>(
        &'a self,
        ctx: &'a mut Context,
        args: SendPeek<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>> {
        Box::pin(async move {
            // Start a span and store it in extensions
            let mut span = PendingSpan::start(ctx);

            // Add per-argument span attributes using arg names from context
            let printer = PrettyPrinter::new()
                .with_colors(facet_pretty::ColorMode::Never)
                .with_max_content_len(128);

            let arg_names = ctx.arg_names();
            let peek = args.peek();

            // Args is a tuple - iterate through its fields
            if let Ok(tuple) = peek.into_struct() {
                for (i, name) in arg_names.iter().enumerate() {
                    if let Ok(field) = tuple.field(i) {
                        let value_str = printer.format_peek(field);
                        span.attributes
                            .push(KeyValue::string(format!("rpc.args.{}", name), value_str));
                    }
                }
            }

            // Also insert CurrentTrace so that TracingCaller can propagate
            // the trace context to downstream calls
            let current_trace = CurrentTrace {
                trace_id: span.trace_id.clone(),
                span_id: span.span_id.clone(),
                flags: 0x01, // sampled
            };
            ctx.extensions.insert(current_trace);

            ctx.extensions.insert(span);
            Ok(())
        })
    }

    fn post<'a>(
        &'a self,
        ctx: &'a Context,
        outcome: MethodOutcome<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let exporter = self.exporter.clone();
        Box::pin(async move {
            // Get the pending span and finish it
            if let Some(pending) = ctx.extensions.get::<PendingSpan>() {
                let span = pending.clone().finish(&outcome);
                exporter.send(span);
            }
        })
    }
}
