//! Cell-side tracing layer and service implementation.
//!
//! Provides a `tracing_subscriber::Layer` that captures events and spans,
//! buffers them, and forwards to the host via roam RPC.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use roam::session::Tx;
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use crate::buffer::LossyBuffer;
use crate::record::{FieldValue, Level, SpanId, TracingRecord};
use crate::service::{CellTracing, ConfigResult, TracingConfig};

/// Extension stored in span extensions to track our span ID.
struct SpanIdExt(SpanId);

/// Cell-side tracing layer that forwards events to the host.
///
/// This layer captures tracing events and spans, converts them to
/// `TracingRecord` values, and pushes them to a bounded buffer.
/// A separate async task drains the buffer to the host via `Tx`.
pub struct CellTracingLayer {
    /// Bounded buffer for outgoing records.
    pub(crate) buffer: Arc<LossyBuffer<TracingRecord>>,
    /// Span ID allocator (cell-local).
    next_span_id: AtomicU64,
    /// Current configuration.
    pub(crate) config: Arc<RwLock<TracingConfig>>,
    /// Start time for monotonic timestamps.
    start: Instant,
}

impl CellTracingLayer {
    /// Create a new cell tracing layer.
    ///
    /// `buffer_size` is the maximum number of records to buffer.
    /// When full, oldest records are dropped (lossy).
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: Arc::new(LossyBuffer::new(buffer_size)),
            next_span_id: AtomicU64::new(1),
            config: Arc::new(RwLock::new(TracingConfig::default())),
            start: Instant::now(),
        }
    }

    /// Get a service handle for implementing `CellTracing`.
    ///
    /// The returned service should be registered with the cell's dispatcher.
    pub fn service_handle(&self) -> CellTracingService {
        CellTracingService {
            buffer: self.buffer.clone(),
            config: self.config.clone(),
        }
    }

    fn alloc_span_id(&self) -> SpanId {
        self.next_span_id.fetch_add(1, Ordering::Relaxed)
    }

    fn should_emit(&self, level: Level, _target: &str) -> bool {
        let config = self.config.read().unwrap();
        if level < config.min_level {
            return false;
        }
        // TODO: Apply target filters from config.filters
        true
    }

    fn now_ns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
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
        let config = self.config.read().unwrap();
        if !config.include_span_events {
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
        let config = self.config.read().unwrap();
        if !config.include_span_events {
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
        let config = self.config.read().unwrap();
        if !config.include_span_events {
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
        let config = self.config.read().unwrap();
        if !config.include_span_events {
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
/// Implements the `CellTracing` service trait.
#[derive(Clone)]
pub struct CellTracingService {
    buffer: Arc<LossyBuffer<TracingRecord>>,
    config: Arc<RwLock<TracingConfig>>,
}

impl CellTracing for CellTracingService {
    async fn configure(&self, config: TracingConfig) -> ConfigResult {
        *self.config.write().unwrap() = config;
        ConfigResult::Ok
    }

    async fn subscribe(&self, sink: Tx<TracingRecord>) {
        // Spawn background task to drain buffer to sink
        let buffer = self.buffer.clone();
        tokio::spawn(async move {
            loop {
                if let Some(record) = buffer.pop().await
                    && sink.send(&record).await.is_err()
                {
                    break; // Sink closed
                }
            }
        });
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
