#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]
#![allow(clippy::type_complexity)]

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use rapace::{BufferPool, Frame, RpcError, RpcSession};
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

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
// TracingSink Service (plugin calls host)
// ============================================================================

/// Service for receiving tracing data from plugins.
///
/// The host implements this, the plugin calls it via RPC.
#[allow(async_fn_in_trait)]
#[rapace::service]
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
// TracingConfig Service (host calls plugin)
// ============================================================================

/// Service for configuring tracing in plugins.
///
/// The plugin implements this, the host calls it via RPC to push filter config.
/// This allows the host to be the single source of truth for log filtering.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait TracingConfig {
    /// Set the tracing filter.
    ///
    /// The filter string uses the same format as RUST_LOG (e.g., "info,mymodule=debug").
    /// The plugin should apply this filter to all subsequent tracing calls.
    async fn set_filter(&self, filter: String);
}

// ============================================================================
// Plugin Side: Shared Filter State
// ============================================================================

use tracing_subscriber::EnvFilter;

/// Shared filter state that can be updated by the host.
///
/// This is wrapped in Arc and shared between `RapaceTracingLayer` and `TracingConfigImpl`.
#[derive(Clone)]
pub struct SharedFilter {
    inner: Arc<parking_lot::RwLock<EnvFilter>>,
}

impl SharedFilter {
    /// Create a new shared filter with default (allow all) settings.
    pub fn new() -> Self {
        // Default to a conservative filter to avoid flooding the transport during startup.
        // Hosts that want more verbosity can push a filter update via TracingConfig.
        let default_filter = std::env::var("RAPACE_TRACING_DEFAULT_FILTER")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "warn".to_string());

        let filter = EnvFilter::new(default_filter);
        Self {
            inner: Arc::new(parking_lot::RwLock::new(filter)),
        }
    }

    /// Update the filter from a filter string (RUST_LOG format).
    pub fn set_filter(&self, filter_str: &str) {
        match EnvFilter::builder().parse(filter_str) {
            Ok(filter) => {
                *self.inner.write() = filter;
            }
            Err(e) => {
                // Log the error but don't crash - keep existing filter
                eprintln!("rapace-tracing: invalid filter '{}': {}", filter_str, e);
            }
        }
    }

    /// Check if a given level is enabled (max level check).
    ///
    /// This is a quick check based on the filter's max level hint.
    /// Target-specific filtering happens through the Layer's `enabled` method.
    pub fn max_level_enabled(&self, level: tracing::level_filters::LevelFilter) -> bool {
        let filter = self.inner.read();
        if let Some(max) = filter.max_level_hint() {
            level <= max
        } else {
            true
        }
    }

    /// Get the current max level hint.
    pub fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        self.inner.read().max_level_hint()
    }
}

impl Default for SharedFilter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Plugin Side: TracingConfig Implementation
// ============================================================================

/// Plugin-side implementation of TracingConfig.
///
/// Host calls this to push filter updates to the plugin.
#[derive(Clone)]
pub struct TracingConfigImpl {
    filter: SharedFilter,
}

impl TracingConfigImpl {
    /// Create a new TracingConfig implementation with the given shared filter.
    pub fn new(filter: SharedFilter) -> Self {
        Self { filter }
    }
}

impl TracingConfig for TracingConfigImpl {
    async fn set_filter(&self, filter: String) {
        self.filter.set_filter(&filter);
    }
}

/// Create a dispatcher for TracingConfig service (plugin side).
pub fn create_tracing_config_dispatcher(
    config: TracingConfigImpl,
    buffer_pool: BufferPool,
) -> impl Fn(Frame) -> Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
+ Send
+ Sync
+ 'static {
    move |request: Frame| {
        let config = config.clone();
        let buffer_pool = buffer_pool.clone();
        Box::pin(async move {
            let server = TracingConfigServer::new(config);
            let mut response = server
                .dispatch(request.desc.method_id, &request, &buffer_pool)
                .await?;
            response.desc.channel_id = request.desc.channel_id;
            response.desc.msg_id = request.desc.msg_id;
            Ok(response)
        })
    }
}

// ============================================================================
// Plugin Side: RapaceTracingLayer
// ============================================================================

/// A tracing Layer that forwards spans/events to a TracingSink via RPC.
///
/// Install this layer in the plugin's tracing_subscriber registry to have
/// all tracing data forwarded to the host process.
///
/// The layer uses a `SharedFilter` to apply host-controlled filtering locally,
/// avoiding unnecessary RPC calls for filtered events.
///
/// # Generic Transport
///
/// `RapaceTracingLayer` is generic over the transport type `T`, mirroring `RpcSession<T>`.
pub struct RapaceTracingLayer<T: rapace::Transport> {
    session: Arc<RpcSession<T>>,
    /// Maps local tracing span IDs to our u64 IDs used in RPC
    span_ids: Mutex<HashMap<u64, u64>>,
    /// Counter for generating local span IDs
    next_span_id: AtomicU64,
    /// Runtime handle for spawning async tasks
    rt: tokio::runtime::Handle,
    /// Shared filter state (updated by host via TracingConfig)
    filter: SharedFilter,
}

impl<T: rapace::Transport> RapaceTracingLayer<T> {
    /// Create a new layer that forwards to the given RPC session.
    ///
    /// The session should be connected to a host that implements TracingSink.
    /// Use the returned `SharedFilter` to create a `TracingConfigImpl` for the
    /// host to push filter updates.
    pub fn new(session: Arc<RpcSession<T>>, rt: tokio::runtime::Handle) -> (Self, SharedFilter) {
        let filter = SharedFilter::new();
        let layer = Self {
            session,
            span_ids: Mutex::new(HashMap::new()),
            next_span_id: AtomicU64::new(1),
            rt,
            filter: filter.clone(),
        };
        (layer, filter)
    }

    /// Create a new layer with an existing shared filter.
    pub fn with_filter(
        session: Arc<RpcSession<T>>,
        rt: tokio::runtime::Handle,
        filter: SharedFilter,
    ) -> Self {
        Self {
            session,
            span_ids: Mutex::new(HashMap::new()),
            next_span_id: AtomicU64::new(1),
            rt,
            filter,
        }
    }

    /// Call TracingSink.new_span via RPC (fire-and-forget from sync context).
    fn call_new_span(&self, meta: SpanMeta) -> u64 {
        let local_id = self.next_span_id.fetch_add(1, Ordering::Relaxed);
        let session = self.session.clone();

        self.rt.spawn(async move {
            let request_bytes: Vec<u8> = rapace::postcard_to_vec(&meta);
            let _ = session
                .notify(TRACING_SINK_METHOD_ID_NEW_SPAN, request_bytes)
                .await;
        });

        local_id
    }

    /// Call TracingSink.record via RPC.
    fn call_record(&self, span_id: u64, fields: Vec<Field>) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let request_bytes: Vec<u8> = rapace::postcard_to_vec(&(span_id, fields));
            let _ = session
                .notify(TRACING_SINK_METHOD_ID_RECORD, request_bytes)
                .await;
        });
    }

    /// Call TracingSink.event via RPC.
    fn call_event(&self, event: EventMeta) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let request_bytes: Vec<u8> = rapace::postcard_to_vec(&event);
            let _ = session
                .notify(TRACING_SINK_METHOD_ID_EVENT, request_bytes)
                .await;
        });
    }

    /// Call TracingSink.enter via RPC.
    fn call_enter(&self, span_id: u64) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let request_bytes: Vec<u8> = rapace::postcard_to_vec(&span_id);
            let _ = session
                .notify(TRACING_SINK_METHOD_ID_ENTER, request_bytes)
                .await;
        });
    }

    /// Call TracingSink.exit via RPC.
    fn call_exit(&self, span_id: u64) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let request_bytes: Vec<u8> = rapace::postcard_to_vec(&span_id);
            let _ = session
                .notify(TRACING_SINK_METHOD_ID_EXIT, request_bytes)
                .await;
        });
    }

    /// Call TracingSink.drop_span via RPC.
    fn call_drop_span(&self, span_id: u64) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let request_bytes: Vec<u8> = rapace::postcard_to_vec(&span_id);
            let _ = session
                .notify(TRACING_SINK_METHOD_ID_DROP_SPAN, request_bytes)
                .await;
        });
    }
}

impl<S, T> Layer<S> for RapaceTracingLayer<T>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    T: rapace::Transport,
{
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        // Avoid infinite recursion: don't forward events about rapace_tracing itself
        // or rapace_core (which handles the RPC session internals)
        let target = metadata.target();
        if target.starts_with("rapace_tracing")
            || target.starts_with("rapace_core")
            || target.starts_with("rapace_transport_shm")
        {
            return false;
        }

        // Check against the host-controlled filter
        let level = match *metadata.level() {
            tracing::Level::ERROR => tracing::level_filters::LevelFilter::ERROR,
            tracing::Level::WARN => tracing::level_filters::LevelFilter::WARN,
            tracing::Level::INFO => tracing::level_filters::LevelFilter::INFO,
            tracing::Level::DEBUG => tracing::level_filters::LevelFilter::DEBUG,
            tracing::Level::TRACE => tracing::level_filters::LevelFilter::TRACE,
        };
        self.filter.max_level_enabled(level)
    }

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
        self.records
            .lock()
            .push(TraceRecord::NewSpan { id, meta: span });
        id
    }

    async fn record(&self, span_id: u64, fields: Vec<Field>) {
        self.records
            .lock()
            .push(TraceRecord::Record { span_id, fields });
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
    buffer_pool: BufferPool,
) -> impl Fn(Frame) -> Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
+ Send
+ Sync
+ 'static {
    move |request: Frame| {
        let sink = sink.clone();
        let buffer_pool = buffer_pool.clone();
        Box::pin(async move {
            let server = TracingSinkServer::new(sink);
            let mut response = server
                .dispatch(request.desc.method_id, &request, &buffer_pool)
                .await?;
            response.desc.channel_id = request.desc.channel_id;
            response.desc.msg_id = request.desc.msg_id;
            Ok(response)
        })
    }
}

// TracingSinkClient is generated by the rapace::service attribute.
// Use TracingSinkClient::new(session) to create a client.
