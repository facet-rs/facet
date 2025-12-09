//! Tracing subscriber that forwards spans/events over rapace RPC.
//!
//! This crate enables plugins to use `tracing` normally while having all
//! spans and events collected in the host process via rapace RPC.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                             PLUGIN PROCESS                              │
//! │                                                                         │
//! │   tracing::info!("hello") ──► RapaceTracingLayer ──► TracingSinkClient ─┤
//! │                                                                         │
//! └────────────────────────────────────────────────────────────────────────┬┘
//!                                                                          │
//!                              rapace transport (TCP/Unix/SHM)             │
//!                                                                          │
//! ┌────────────────────────────────────────────────────────────────────────┴┐
//! │                              HOST PROCESS                               │
//! │                                                                         │
//! │   TracingSinkServer ──► HostTracingSink ──► tracing_subscriber / logs  │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! // Plugin side: install the layer
//! let layer = RapaceTracingLayer::new(sink_client);
//! tracing_subscriber::registry().with(layer).init();
//!
//! // Now all tracing calls are forwarded to the host
//! tracing::info!("hello from plugin");
//! ```

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use rapace_core::{Frame, RpcError, Transport};
use rapace_testkit::RpcSession;
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

// Required by the macro
#[allow(unused)]
use rapace_registry;

// ============================================================================
// Facet Types (transport-agnostic)
// ============================================================================

/// A field value captured from tracing.
#[derive(Debug, Clone, facet::Facet)]
pub struct Field {
    /// Field name
    pub name: String,
    /// Field value (stringified for v1)
    pub value: String,
}

/// Metadata about a span.
#[derive(Debug, Clone, facet::Facet)]
pub struct SpanMeta {
    /// Span name
    pub name: String,
    /// Target (module path)
    pub target: String,
    /// Level as string ("TRACE", "DEBUG", "INFO", "WARN", "ERROR")
    pub level: String,
    /// Source file, if available
    pub file: Option<String>,
    /// Line number, if available
    pub line: Option<u32>,
    /// Fields recorded at span creation
    pub fields: Vec<Field>,
}

/// Metadata about an event.
#[derive(Debug, Clone, facet::Facet)]
pub struct EventMeta {
    /// Event message (from the `message` field if present)
    pub message: String,
    /// Target (module path)
    pub target: String,
    /// Level as string
    pub level: String,
    /// Source file, if available
    pub file: Option<String>,
    /// Line number, if available
    pub line: Option<u32>,
    /// All fields including message
    pub fields: Vec<Field>,
    /// Parent span ID if inside a span
    pub parent_span_id: Option<u64>,
}

// ============================================================================
// TracingSink Service
// ============================================================================

/// Service for receiving tracing data from plugins.
///
/// The host implements this, the plugin calls it via RPC.
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
pub trait TracingSink {
    /// Called when a new span is created.
    /// Returns a span ID that the plugin should use for subsequent calls.
    async fn new_span(&self, span: crate::SpanMeta) -> u64;

    /// Called when fields are recorded on an existing span.
    async fn record(&self, span_id: u64, fields: Vec<crate::Field>);

    /// Called when an event is emitted.
    async fn event(&self, event: crate::EventMeta);

    /// Called when a span is entered.
    async fn enter(&self, span_id: u64);

    /// Called when a span is exited.
    async fn exit(&self, span_id: u64);

    /// Called when a span is dropped/closed.
    async fn drop_span(&self, span_id: u64);
}

// ============================================================================
// Plugin Side: RapaceTracingLayer
// ============================================================================

/// A tracing Layer that forwards spans/events to a TracingSink via RPC.
///
/// Install this layer in the plugin's tracing_subscriber registry to have
/// all tracing data forwarded to the host process.
pub struct RapaceTracingLayer<T: Transport + Send + Sync + 'static> {
    session: Arc<RpcSession<T>>,
    /// Maps local tracing span IDs to our u64 IDs used in RPC
    span_ids: Mutex<HashMap<u64, u64>>,
    /// Counter for generating local span IDs
    next_span_id: AtomicU64,
    /// Runtime handle for spawning async tasks
    rt: tokio::runtime::Handle,
}

impl<T: Transport + Send + Sync + 'static> RapaceTracingLayer<T> {
    /// Create a new layer that forwards to the given RPC session.
    ///
    /// The session should be connected to a host that implements TracingSink.
    pub fn new(session: Arc<RpcSession<T>>, rt: tokio::runtime::Handle) -> Self {
        Self {
            session,
            span_ids: Mutex::new(HashMap::new()),
            next_span_id: AtomicU64::new(1),
            rt,
        }
    }

    /// Call TracingSink.new_span via RPC (blocking from sync context).
    fn call_new_span(&self, meta: SpanMeta) -> u64 {
        let session = self.session.clone();
        let local_id = self.next_span_id.fetch_add(1, Ordering::Relaxed);

        // We need to call the async RPC from a sync context.
        // Use spawn_blocking + block_on pattern.
        self.rt.spawn(async move {
            let channel_id = session.next_channel_id();
            let payload = facet_postcard::to_vec(&meta).unwrap();
            // method_id 1 = new_span
            let _ = session.call(channel_id, 1, payload).await;
        });

        local_id
    }

    /// Call TracingSink.record via RPC.
    fn call_record(&self, span_id: u64, fields: Vec<Field>) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let channel_id = session.next_channel_id();
            let payload = facet_postcard::to_vec(&(span_id, fields)).unwrap();
            // method_id 2 = record
            let _ = session.call(channel_id, 2, payload).await;
        });
    }

    /// Call TracingSink.event via RPC.
    fn call_event(&self, event: EventMeta) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let channel_id = session.next_channel_id();
            let payload = facet_postcard::to_vec(&event).unwrap();
            // method_id 3 = event
            let _ = session.call(channel_id, 3, payload).await;
        });
    }

    /// Call TracingSink.enter via RPC.
    fn call_enter(&self, span_id: u64) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let channel_id = session.next_channel_id();
            let payload = facet_postcard::to_vec(&span_id).unwrap();
            // method_id 4 = enter
            let _ = session.call(channel_id, 4, payload).await;
        });
    }

    /// Call TracingSink.exit via RPC.
    fn call_exit(&self, span_id: u64) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let channel_id = session.next_channel_id();
            let payload = facet_postcard::to_vec(&span_id).unwrap();
            // method_id 5 = exit
            let _ = session.call(channel_id, 5, payload).await;
        });
    }

    /// Call TracingSink.drop_span via RPC.
    fn call_drop_span(&self, span_id: u64) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let channel_id = session.next_channel_id();
            let payload = facet_postcard::to_vec(&span_id).unwrap();
            // method_id 6 = drop_span
            let _ = session.call(channel_id, 6, payload).await;
        });
    }
}

impl<S, T> Layer<S> for RapaceTracingLayer<T>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    T: Transport + Send + Sync + 'static,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, _ctx: Context<'_, S>) {
        let meta = attrs.metadata();

        // Collect fields
        let mut visitor = FieldVisitor::new();
        attrs.record(&mut visitor);

        let span_meta = SpanMeta {
            name: meta.name().to_string(),
            target: meta.target().to_string(),
            level: meta.level().to_string(),
            file: meta.file().map(|s| s.to_string()),
            line: meta.line(),
            fields: visitor.fields,
        };

        let local_id = self.call_new_span(span_meta);

        // Store mapping from tracing's Id to our local ID
        self.span_ids.lock().insert(id.into_u64(), local_id);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        let span_id = match self.span_ids.lock().get(&id.into_u64()) {
            Some(&id) => id,
            None => return,
        };

        let mut visitor = FieldVisitor::new();
        values.record(&mut visitor);

        if !visitor.fields.is_empty() {
            self.call_record(span_id, visitor.fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();

        // Collect fields
        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);

        // Extract message from fields
        let message = visitor
            .fields
            .iter()
            .find(|f| f.name == "message")
            .map(|f| f.value.clone())
            .unwrap_or_default();

        // Get parent span ID if any
        let parent_span_id = ctx
            .current_span()
            .id()
            .and_then(|id| self.span_ids.lock().get(&id.into_u64()).copied());

        let event_meta = EventMeta {
            message,
            target: meta.target().to_string(),
            level: meta.level().to_string(),
            file: meta.file().map(|s| s.to_string()),
            line: meta.line(),
            fields: visitor.fields,
            parent_span_id,
        };

        self.call_event(event_meta);
    }

    fn on_enter(&self, id: &Id, _ctx: Context<'_, S>) {
        if let Some(&span_id) = self.span_ids.lock().get(&id.into_u64()) {
            self.call_enter(span_id);
        }
    }

    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        if let Some(&span_id) = self.span_ids.lock().get(&id.into_u64()) {
            self.call_exit(span_id);
        }
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        if let Some(span_id) = self.span_ids.lock().remove(&id.into_u64()) {
            self.call_drop_span(span_id);
        }
    }
}

/// Visitor for collecting tracing fields into our Field type.
struct FieldVisitor {
    fields: Vec<Field>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self { fields: Vec::new() }
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.push(Field {
            name: field.name().to_string(),
            value: format!("{:?}", value),
        });
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.push(Field {
            name: field.name().to_string(),
            value: value.to_string(),
        });
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.push(Field {
            name: field.name().to_string(),
            value: value.to_string(),
        });
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.push(Field {
            name: field.name().to_string(),
            value: value.to_string(),
        });
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.push(Field {
            name: field.name().to_string(),
            value: value.to_string(),
        });
    }
}

// ============================================================================
// Host Side: TracingSink Implementation
// ============================================================================

/// Collected trace data for inspection/testing.
#[derive(Debug, Clone)]
pub enum TraceRecord {
    NewSpan { id: u64, meta: SpanMeta },
    Record { span_id: u64, fields: Vec<Field> },
    Event(EventMeta),
    Enter { span_id: u64 },
    Exit { span_id: u64 },
    DropSpan { span_id: u64 },
}

/// Host-side implementation of TracingSink.
///
/// Collects all trace data into a buffer for inspection/testing.
/// In a real application, you might forward to a real tracing subscriber.
#[derive(Clone)]
pub struct HostTracingSink {
    records: Arc<Mutex<Vec<TraceRecord>>>,
    next_span_id: Arc<AtomicU64>,
}

impl HostTracingSink {
    /// Create a new sink that collects trace data.
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            next_span_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Get all collected trace records.
    pub fn records(&self) -> Vec<TraceRecord> {
        self.records.lock().clone()
    }

    /// Clear all collected records.
    pub fn clear(&self) {
        self.records.lock().clear();
    }
}

impl Default for HostTracingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl TracingSink for HostTracingSink {
    async fn new_span(&self, span: SpanMeta) -> u64 {
        let id = self.next_span_id.fetch_add(1, Ordering::Relaxed);
        self.records.lock().push(TraceRecord::NewSpan {
            id,
            meta: span,
        });
        id
    }

    async fn record(&self, span_id: u64, fields: Vec<Field>) {
        self.records.lock().push(TraceRecord::Record { span_id, fields });
    }

    async fn event(&self, event: EventMeta) {
        self.records.lock().push(TraceRecord::Event(event));
    }

    async fn enter(&self, span_id: u64) {
        self.records.lock().push(TraceRecord::Enter { span_id });
    }

    async fn exit(&self, span_id: u64) {
        self.records.lock().push(TraceRecord::Exit { span_id });
    }

    async fn drop_span(&self, span_id: u64) {
        self.records.lock().push(TraceRecord::DropSpan { span_id });
    }
}

// ============================================================================
// Dispatcher Helper
// ============================================================================

/// Create a dispatcher for TracingSink service.
pub fn create_tracing_sink_dispatcher(
    sink: HostTracingSink,
) -> impl Fn(u32, u32, Vec<u8>) -> Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
       + Send
       + Sync
       + 'static {
    move |_channel_id, method_id, payload| {
        let sink = sink.clone();
        Box::pin(async move {
            let server = TracingSinkServer::new(sink);
            server.dispatch(method_id, &payload).await
        })
    }
}

// TracingSinkClient is generated by the rapace_macros::service attribute.
// Use TracingSinkClient::new(session) to create a client.
