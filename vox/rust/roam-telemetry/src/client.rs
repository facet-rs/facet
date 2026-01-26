//! Client-side telemetry for outgoing RPC calls.
//!
//! Provides a `TracingCaller` wrapper that:
//! - Creates CLIENT spans for outgoing calls
//! - Injects `traceparent` into request metadata for distributed tracing
//! - Records call success/failure in the span

use std::time::{SystemTime, UNIX_EPOCH};

use facet::Facet;
use roam_session::{Caller, ResponseData, SendPtr, TransportError};
use roam_wire::MetadataValue;

use crate::exporter::OtlpExporter;
use crate::otlp::{KeyValue, Span, SpanKind, Status, generate_span_id, generate_trace_id};

/// The current trace information for context propagation.
///
/// This is stored in `Context::extensions` by `TelemetryMiddleware`, and
/// read from `CURRENT_EXTENSIONS` task-local by `TracingCaller`.
#[derive(Debug, Clone)]
pub struct CurrentTrace {
    /// The trace ID (32 hex chars).
    pub trace_id: String,
    /// The current span ID (16 hex chars) - becomes parent of child spans.
    pub span_id: String,
    /// Trace flags.
    pub flags: u8,
}

impl CurrentTrace {
    /// Format as a traceparent header value.
    pub fn traceparent(&self) -> String {
        format!("00-{}-{}-{:02x}", self.trace_id, self.span_id, self.flags)
    }
}

/// A `Caller` wrapper that adds distributed tracing to outgoing calls.
///
/// For each call:
/// 1. Creates a CLIENT span
/// 2. Injects `traceparent` into metadata (propagating the current trace)
/// 3. Makes the call
/// 4. Records success/failure and exports the span
///
/// # Trace Propagation
///
/// If a [`CurrentTrace`] is found in the task-local `CURRENT_EXTENSIONS`
/// (set by the generated dispatch code), the span will be a child of that trace.
/// Otherwise, a new trace is started.
///
/// The server-side `TelemetryMiddleware` inserts `CurrentTrace` into context
/// extensions, and the generated dispatch code makes extensions available via
/// the `CURRENT_EXTENSIONS` task-local. This allows nested calls to be part
/// of the same trace.
///
/// # Example
///
/// ```ignore
/// use roam_telemetry::{TracingCaller, OtlpExporter};
///
/// let exporter = OtlpExporter::new("http://tempo:4318/v1/traces", "my-service");
/// let caller = TracingCaller::new(connection_handle, exporter);
/// let client = MyServiceClient::new(caller);
///
/// // Calls will now create spans and propagate trace context
/// client.some_method(args).await?;
/// ```
#[derive(Clone)]
pub struct TracingCaller<C> {
    inner: C,
    exporter: OtlpExporter,
}

impl<C> TracingCaller<C> {
    /// Create a new tracing caller wrapping the given caller.
    pub fn new(inner: C, exporter: OtlpExporter) -> Self {
        Self { inner, exporter }
    }

    /// Get the inner caller.
    pub fn inner(&self) -> &C {
        &self.inner
    }
}

impl<C: Caller> Caller for TracingCaller<C> {
    #[cfg(not(target_arch = "wasm32"))]
    async fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        mut metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        // Get trace context from CURRENT_EXTENSIONS (set by generated dispatch code)
        // or create a new trace if not in a request context
        let (trace_id, parent_span_id) = roam_session::CURRENT_EXTENSIONS
            .try_with(|ext| {
                ext.get::<CurrentTrace>()
                    .map(|tc| (tc.trace_id.clone(), Some(tc.span_id.clone())))
            })
            .ok()
            .flatten()
            .unwrap_or_else(|| (generate_trace_id(), None));

        let span_id = generate_span_id();
        let flags: u8 = 0x01; // sampled

        // Inject traceparent into metadata
        let traceparent = format!("00-{}-{}-{:02x}", trace_id, span_id, flags);
        metadata.push((
            "traceparent".to_string(),
            MetadataValue::String(traceparent),
        ));

        let start_time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // Get method name for span (best effort)
        let method_name = roam_session::diagnostic::get_method_name(method_id)
            .unwrap_or("unknown")
            .to_string();

        // Make the actual call
        let result = self
            .inner
            .call_with_metadata(method_id, args, metadata)
            .await;

        let end_time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // Build span attributes
        let mut attributes = vec![
            KeyValue::string("rpc.system", "roam"),
            KeyValue::string("rpc.method", &method_name),
            KeyValue::string("rpc.service", self.exporter.service_name()),
        ];

        let status = match &result {
            Ok(_) => {
                attributes.push(KeyValue::bool("rpc.success", true));
                Status::ok()
            }
            Err(e) => {
                attributes.push(KeyValue::bool("rpc.success", false));
                attributes.push(KeyValue::string("rpc.error", format!("{:?}", e)));
                Status::error(format!("{:?}", e))
            }
        };

        // Export the span
        let span = Span {
            trace_id,
            span_id,
            parent_span_id,
            name: method_name,
            kind: SpanKind::Client.as_u32(),
            start_time_unix_nano: start_time_ns.to_string(),
            end_time_unix_nano: end_time_ns.to_string(),
            attributes,
            status,
        };
        self.exporter.send(span);

        result
    }

    #[cfg(target_arch = "wasm32")]
    async fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        mut metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        // WASM version - uses same CURRENT_EXTENSIONS task-local
        let (trace_id, parent_span_id) = roam_session::CURRENT_EXTENSIONS
            .try_with(|ext| {
                ext.get::<CurrentTrace>()
                    .map(|tc| (tc.trace_id.clone(), Some(tc.span_id.clone())))
            })
            .ok()
            .flatten()
            .unwrap_or_else(|| (generate_trace_id(), None));

        let span_id = generate_span_id();
        let flags: u8 = 0x01;

        let traceparent = format!("00-{}-{}-{:02x}", trace_id, span_id, flags);
        metadata.push((
            "traceparent".to_string(),
            MetadataValue::String(traceparent),
        ));

        let start_time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let method_name = roam_session::diagnostic::get_method_name(method_id)
            .unwrap_or("unknown")
            .to_string();

        let result = self
            .inner
            .call_with_metadata(method_id, args, metadata)
            .await;

        let end_time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let mut attributes = vec![
            KeyValue::string("rpc.system", "roam"),
            KeyValue::string("rpc.method", &method_name),
            KeyValue::string("rpc.service", self.exporter.service_name()),
        ];

        let status = match &result {
            Ok(_) => {
                attributes.push(KeyValue::bool("rpc.success", true));
                Status::ok()
            }
            Err(e) => {
                attributes.push(KeyValue::bool("rpc.success", false));
                attributes.push(KeyValue::string("rpc.error", format!("{:?}", e)));
                Status::error(format!("{:?}", e))
            }
        };

        let span = Span {
            trace_id,
            span_id,
            parent_span_id,
            name: method_name,
            kind: SpanKind::Client.as_u32(),
            start_time_unix_nano: start_time_ns.to_string(),
            end_time_unix_nano: end_time_ns.to_string(),
            attributes,
            status,
        };
        self.exporter.send(span);

        result
    }

    fn bind_response_streams<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]) {
        self.inner.bind_response_streams(response, channels)
    }

    #[allow(unsafe_code)]
    fn call_with_metadata_by_shape(
        &self,
        method_id: u64,
        args_ptr: SendPtr,
        args_shape: &'static facet::Shape,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send {
        // TracingCaller just delegates to inner - tracing happens at the generic call level
        self.inner
            .call_with_metadata_by_shape(method_id, args_ptr, args_shape, metadata)
    }

    #[allow(unsafe_code)]
    unsafe fn bind_response_streams_by_shape(
        &self,
        response_ptr: *mut (),
        response_shape: &'static facet::Shape,
        channels: &[u64],
    ) {
        // SAFETY: Caller guarantees response_ptr is valid and initialized
        unsafe {
            self.inner
                .bind_response_streams_by_shape(response_ptr, response_shape, channels)
        }
    }
}
