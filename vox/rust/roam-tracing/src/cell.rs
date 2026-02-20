//! Cell-side tracing layer and service implementation.
//!
//! Provides a `tracing_subscriber::Layer` that captures events and spans,
//! buffers them, and forwards to the host via RPC calls.

use moire::sync::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use roam::session::ConnectionHandle;
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use crate::buffer::LossyBuffer;
use crate::record::{FieldValue, Level, SpanId, TracingRecord};
use crate::service::{CellTracing, ConfigResult, HostTracingClient, TracingConfig};

/// Extension stored in span extensions to track our span ID.
struct SpanIdExt(SpanId);

/// Parsed filter state derived from TracingConfig.
pub(crate) struct FilterState {
    /// Parsed target filter for level/target checking.
    targets: Targets,
    /// Whether to include span enter/exit events.
    include_span_events: bool,
}

impl FilterState {
    fn from_config(config: &TracingConfig) -> Self {
        // Parse the filter directives. If parsing fails, default to "info".
        let targets = config
            .filter_directives
            .parse::<Targets>()
            .unwrap_or_else(|_| "info".parse().unwrap());
        Self {
            targets,
            include_span_events: config.include_span_events,
        }
    }
}

/// Cell-side tracing layer that forwards events to the host.
///
/// This layer captures tracing events and spans, converts them to
/// `TracingRecord` values, and pushes them to a bounded buffer.
/// A separate async task drains the buffer to the host via RPC.
pub struct CellTracingLayer {
    /// Bounded buffer for outgoing records.
    pub(crate) buffer: Arc<LossyBuffer<TracingRecord>>,
    /// Span ID allocator (cell-local).
    next_span_id: AtomicU64,
    /// Parsed filter state (derived from TracingConfig).
    pub(crate) filter: Arc<Mutex<FilterState>>,
    /// Start time for monotonic timestamps.
    start: Instant,
}

impl CellTracingLayer {
    /// Create a new cell tracing layer.
    ///
    /// `buffer_size` is the maximum number of records to buffer.
    /// When full, oldest records are dropped (lossy).
    ///
    /// **Important**: The layer starts with a maximally permissive filter ("trace")
    /// to avoid losing events. Call [`CellTracingService::start`] immediately after
    /// establishing the connection to query the host's config and start draining.
    pub fn new(buffer_size: usize) -> Self {
        // Start maximally permissive - don't drop anything until host config arrives.
        let initial_filter = FilterState::from_config(&TracingConfig::with_filter("trace"));
        Self {
            buffer: Arc::new(LossyBuffer::new(buffer_size)),
            next_span_id: AtomicU64::new(1),
            filter: Arc::new(Mutex::new("CellTracingLayer.filter", initial_filter)),
            start: Instant::now(),
        }
    }

    /// Get a service handle for implementing `CellTracing`.
    ///
    /// The returned service should be registered with the cell's dispatcher.
    pub fn service_handle(&self) -> CellTracingService {
        CellTracingService {
            buffer: self.buffer.clone(),
            filter: self.filter.clone(),
        }
    }

    fn alloc_span_id(&self) -> SpanId {
        self.next_span_id.fetch_add(1, Ordering::Relaxed)
    }

    fn should_emit(&self, level: Level, target: &str) -> bool {
        // Never forward roam crate events - doing so would cause infinite recursion
        // since emit_tracing() itself goes through roam RPC
        if target.starts_with("roam") {
            return false;
        }
        let filter = self.filter.lock();
        filter.targets.would_enable(target, &level.to_tracing())
    }

    fn now_ns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }

    /// Update the filter configuration.
    ///
    /// This is primarily for testing; in production, config is pushed via RPC.
    pub fn set_config(&self, config: &TracingConfig) {
        *self.filter.lock() = FilterState::from_config(config);
    }
}

impl<S> Layer<S> for CellTracingLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let level = Level::from_tracing(attrs.metadata().level());

        if !self.should_emit(level, attrs.metadata().target()) {
            return;
        }

        let span_id = self.alloc_span_id();

        // Store our span_id in the span's extensions (always, for parent tracking)
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanIdExt(span_id));
        }

        // Only emit SpanEnter record if include_span_events is enabled
        let filter = self.filter.lock();
        if !filter.include_span_events {
            return;
        }

        // Get parent span ID if any
        let parent = attrs
            .parent()
            .and_then(|p| ctx.span(p))
            .and_then(|s| s.extensions().get::<SpanIdExt>().map(|e| e.0))
            .or_else(|| {
                // If no explicit parent, check current span
                ctx.current_span()
                    .id()
                    .and_then(|id| ctx.span(id))
                    .and_then(|s| s.extensions().get::<SpanIdExt>().map(|e| e.0))
            });

        let mut fields = Vec::new();
        let mut visitor = FieldVisitor(&mut fields);
        attrs.record(&mut visitor);

        let record = TracingRecord::SpanEnter {
            id: span_id,
            parent,
            target: attrs.metadata().target().to_string(),
            name: attrs.metadata().name().to_string(),
            level,
            fields,
            timestamp_ns: self.now_ns(),
        };

        self.buffer.push(record);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let level = Level::from_tracing(event.metadata().level());

        if !self.should_emit(level, event.metadata().target()) {
            return;
        }

        // Get parent span ID from current span
        let parent = ctx
            .current_span()
            .id()
            .and_then(|id| ctx.span(id))
            .and_then(|s| s.extensions().get::<SpanIdExt>().map(|e| e.0));

        let mut fields = Vec::new();
        let mut message = None;
        let mut visitor = EventVisitor(&mut fields, &mut message);
        event.record(&mut visitor);

        let record = TracingRecord::Event {
            parent,
            target: event.metadata().target().to_string(),
            level,
            message,
            fields,
            timestamp_ns: self.now_ns(),
        };

        self.buffer.push(record);
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let filter = self.filter.lock();
        if !filter.include_span_events {
            return;
        }

        if let Some(span) = ctx.span(&id)
            && let Some(ext) = span.extensions().get::<SpanIdExt>()
        {
            let record = TracingRecord::SpanClose {
                id: ext.0,
                timestamp_ns: self.now_ns(),
            };
            self.buffer.push(record);
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        // SpanEnter is already recorded on_new_span, but we could emit
        // separate enter events for re-entry if needed
        let filter = self.filter.lock();
        if !filter.include_span_events {
            return;
        }

        if let Some(span) = ctx.span(id)
            && let Some(ext) = span.extensions().get::<SpanIdExt>()
        {
            // Only emit if this is a re-entry (not first enter)
            // For now, we skip this to reduce verbosity
            let _ = ext;
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let filter = self.filter.lock();
        if !filter.include_span_events {
            return;
        }

        if let Some(span) = ctx.span(id)
            && let Some(ext) = span.extensions().get::<SpanIdExt>()
        {
            let record = TracingRecord::SpanExit {
                id: ext.0,
                timestamp_ns: self.now_ns(),
            };
            self.buffer.push(record);
        }
    }
}

/// Service implementation for cell-side tracing.
///
/// Implements the `CellTracing` service trait (for host-pushed config updates)
/// and provides a method to start the drain task.
#[derive(Clone)]
pub struct CellTracingService {
    buffer: Arc<LossyBuffer<TracingRecord>>,
    filter: Arc<Mutex<FilterState>>,
}

/// Guard returned by [`init_cell_tracing`](crate::init_cell_tracing).
///
/// You **must** call [`start()`](Self::start) on this guard after establishing
/// the connection to the host. If dropped without calling `start()`, it will panic.
///
/// This ensures you don't forget to query the host's tracing config.
#[must_use = "you must call .start(handle).await to initialize tracing"]
pub struct CellTracingGuard {
    service: CellTracingService,
    started: Arc<std::sync::atomic::AtomicBool>,
}

impl CellTracingGuard {
    pub(crate) fn new(service: CellTracingService) -> Self {
        Self {
            service,
            started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start the tracing service: query host config, then spawn the drain task.
    ///
    /// **Call this immediately after `establish_guest()` returns**, before doing
    /// any real work. This ensures the tracing filter matches the host's `RUST_LOG`.
    ///
    /// Consumes the guard to prevent double-start.
    pub async fn start(self, handle: ConnectionHandle) {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.service.start(handle).await;
    }

    /// Start with custom batch size and flush interval.
    pub async fn start_with_options(
        self,
        handle: ConnectionHandle,
        batch_size: usize,
        flush_interval: Duration,
    ) {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.service
            .start_with_options(handle, batch_size, flush_interval)
            .await;
    }

    /// Get a clone of the underlying service for registering with dispatchers.
    ///
    /// The service implements `CellTracing` for receiving config updates from host.
    pub fn service(&self) -> CellTracingService {
        self.service.clone()
    }
}

impl Drop for CellTracingGuard {
    fn drop(&mut self) {
        if !self.started.load(std::sync::atomic::Ordering::SeqCst) {
            panic!(
                "CellTracingGuard dropped without calling start()! \
                 You must call guard.start(handle).await after establish_guest() \
                 to initialize tracing with the host's RUST_LOG config."
            );
        }
    }
}

impl CellTracingGuard {
    /// Defuse the guard without starting (won't panic on drop).
    ///
    /// **For testing only.** In production, always call `.start()` instead.
    /// This is useful for unit tests that don't have a host to connect to.
    #[doc(hidden)]
    pub fn defuse(self) -> CellTracingService {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.service.clone()
    }
}

impl CellTracingService {
    /// Start the tracing service: query host config, then spawn the drain task.
    ///
    /// **Call this immediately after `establish_guest()` returns**, before doing
    /// any real work. This ensures the tracing filter matches the host's `RUST_LOG`
    /// before any events are emitted.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (layer, service) = init_cell_tracing(1024);
    /// tracing_subscriber::registry().with(layer).init();
    ///
    /// let handle = establish_guest(transport, dispatcher);
    ///
    /// // Query host config and start draining - do this FIRST
    /// service.start(handle.clone()).await;
    ///
    /// // Now tracing is properly configured
    /// tracing::info!("cell started");
    /// ```
    pub async fn start(&self, handle: ConnectionHandle) {
        self.start_with_options(handle, 64, Duration::from_millis(50))
            .await;
    }

    /// Start with custom batch size and flush interval.
    ///
    /// See [`start`](Self::start) for details.
    pub async fn start_with_options(
        &self,
        handle: ConnectionHandle,
        batch_size: usize,
        flush_interval: Duration,
    ) {
        // Query config from host FIRST, before spawning drain task
        let client = HostTracingClient::new(handle.clone());
        match client.get_tracing_config().await {
            Ok(host_config) => {
                *self.filter.lock() = FilterState::from_config(&host_config);
            }
            Err(_) => {
                // Use default config if query fails (keeps "trace" level)
            }
        }

        // Now spawn the drain task
        let buffer = self.buffer.clone();
        moire::task::spawn(async move {
            loop {
                // Collect batch from buffer
                let mut batch = Vec::with_capacity(batch_size);
                while batch.len() < batch_size {
                    if let Some(record) = buffer.try_pop() {
                        batch.push(record);
                    } else {
                        break;
                    }
                }

                // Send batch if non-empty
                if !batch.is_empty() {
                    let _ = client.emit_tracing(batch).await;
                }

                moire::time::sleep(flush_interval).await;
            }
        });
    }

    /// Spawn the drain task without querying config first.
    ///
    /// **Deprecated**: Use [`start`](Self::start) instead, which queries the host
    /// config before spawning. This method exists for backwards compatibility.
    #[deprecated(
        since = "0.7.0",
        note = "use `start()` instead which queries config first"
    )]
    pub fn spawn_drain(&self, handle: ConnectionHandle) {
        let buffer = self.buffer.clone();
        let filter = self.filter.clone();

        moire::task::spawn(async move {
            let client = HostTracingClient::new(handle);

            // Query config (but we're already racing with events)
            if let Ok(host_config) = client.get_tracing_config().await {
                *filter.lock() = FilterState::from_config(&host_config);
            }

            loop {
                let mut batch = Vec::with_capacity(64);
                while batch.len() < 64 {
                    if let Some(record) = buffer.try_pop() {
                        batch.push(record);
                    } else {
                        break;
                    }
                }

                if !batch.is_empty() {
                    let _ = client.emit_tracing(batch).await;
                }

                moire::time::sleep(Duration::from_millis(50)).await;
            }
        });
    }
}

impl CellTracing for CellTracingService {
    async fn configure(&self, _cx: &roam::session::Context, config: TracingConfig) -> ConfigResult {
        *self.filter.lock() = FilterState::from_config(&config);
        ConfigResult::Ok
    }
}

/// Field visitor for span attributes.
struct FieldVisitor<'a>(&'a mut Vec<(String, FieldValue)>);

impl tracing::field::Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push((
            field.name().to_string(),
            FieldValue::Str(format!("{value:?}")),
        ));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        // Store as string since we don't have f64 variant
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0
            .push((field.name().to_string(), FieldValue::I64(value)));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0
            .push((field.name().to_string(), FieldValue::U64(value)));
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        // Store as string since it may overflow i64
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        // Store as string since it may overflow u64
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0
            .push((field.name().to_string(), FieldValue::Bool(value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }
}

/// Field visitor for events that extracts the "message" field separately.
struct EventVisitor<'a>(&'a mut Vec<(String, FieldValue)>, &'a mut Option<String>);

impl tracing::field::Visit for EventVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.1 = Some(format!("{value:?}"));
        } else {
            self.0.push((
                field.name().to_string(),
                FieldValue::Str(format!("{value:?}")),
            ));
        }
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0
            .push((field.name().to_string(), FieldValue::I64(value)));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0
            .push((field.name().to_string(), FieldValue::U64(value)));
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.0
            .push((field.name().to_string(), FieldValue::Str(value.to_string())));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0
            .push((field.name().to_string(), FieldValue::Bool(value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            *self.1 = Some(value.to_string());
        } else {
            self.0
                .push((field.name().to_string(), FieldValue::Str(value.to_string())));
        }
    }
}
