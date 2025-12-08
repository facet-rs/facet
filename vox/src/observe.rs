// src/observe.rs

use std::sync::atomic::{AtomicU64, Ordering};

/// Message descriptor - cold path (observability)
///
/// Stored in a parallel array or separate telemetry ring.
/// Can be disabled per-channel or globally for maximum performance.
#[repr(C, align(64))]
pub struct MsgDescCold {
    /// Correlates with hot descriptor
    pub msg_id: u64,
    /// Distributed tracing ID
    pub trace_id: u64,
    /// Span within trace
    pub span_id: u64,
    /// Parent span for hierarchy
    pub parent_span_id: u64,
    /// When enqueued (nanoseconds)
    pub timestamp_ns: u64,
    /// Debug level for this message
    pub debug_level: u32,
    /// Reserved for future use
    _reserved: u32,
}

/// Telemetry event types
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventType {
    Send = 0,
    Recv = 1,
    Cancel = 2,
    Timeout = 3,
    Error = 4,
}

/// Debug levels (per-channel configurable)
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DebugLevel {
    /// Metrics only (counters), no cold descriptors
    MetricsOnly = 0,
    /// Metadata only (cold desc without payload)
    Metadata = 1,
    /// Full payload mirroring to telemetry ring
    FullPayload = 2,
    /// Plus fault injection rules active
    FaultInjection = 3,
}

/// Telemetry event for observability ring
///
/// A separate ring for telemetry events, readable by observer processes.
/// When the telemetry ring is full, events may be dropped (oldest or newest).
/// Telemetry overflow must not backpressure the data path.
#[repr(C)]
pub struct TelemetryEvent {
    pub timestamp_ns: u64,
    pub trace_id: u64,
    pub span_id: u64,
    pub channel_id: u32,
    pub event_type: u32,
    pub payload_len: u32,
    /// For RECV: time since SEND
    pub latency_ns: u32,
}

/// Per-channel metrics (atomic counters)
#[repr(C)]
pub struct ChannelMetrics {
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub messages_sent: AtomicU64,
    pub messages_received: AtomicU64,
    pub flow_control_stalls: AtomicU64,
    pub errors: AtomicU64,
}

/// Global metrics across all channels
#[repr(C)]
pub struct GlobalMetrics {
    pub ring_high_watermark: AtomicU64,
    pub total_allocations: AtomicU64,
    pub allocation_failures: AtomicU64,
}

/// Non-atomic snapshot of channel metrics
#[derive(Clone, Copy, Debug, Default)]
pub struct ChannelMetricsSnapshot {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub flow_control_stalls: u64,
    pub errors: u64,
}

/// Trace context for distributed tracing
#[derive(Clone, Copy, Debug, Default)]
pub struct TraceContext {
    pub trace_id: u64,
    pub span_id: u64,
    pub parent_span_id: u64,
}

// Compile-time size assertions for repr(C) types
const _: () = {
    assert!(std::mem::size_of::<MsgDescCold>() == 64);
    assert!(std::mem::align_of::<MsgDescCold>() == 64);
    assert!(std::mem::size_of::<TelemetryEvent>() == 40);
};

impl MsgDescCold {
    /// Create a new cold descriptor with trace context
    pub fn new(
        msg_id: u64,
        trace_ctx: TraceContext,
        timestamp_ns: u64,
        debug_level: DebugLevel,
    ) -> Self {
        Self {
            msg_id,
            trace_id: trace_ctx.trace_id,
            span_id: trace_ctx.span_id,
            parent_span_id: trace_ctx.parent_span_id,
            timestamp_ns,
            debug_level: debug_level as u32,
            _reserved: 0,
        }
    }

    /// Get the debug level
    pub fn debug_level(&self) -> Option<DebugLevel> {
        match self.debug_level {
            0 => Some(DebugLevel::MetricsOnly),
            1 => Some(DebugLevel::Metadata),
            2 => Some(DebugLevel::FullPayload),
            3 => Some(DebugLevel::FaultInjection),
            _ => None,
        }
    }

    /// Get trace context
    pub fn trace_context(&self) -> TraceContext {
        TraceContext {
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
        }
    }
}

impl Default for MsgDescCold {
    fn default() -> Self {
        Self {
            msg_id: 0,
            trace_id: 0,
            span_id: 0,
            parent_span_id: 0,
            timestamp_ns: 0,
            debug_level: DebugLevel::MetricsOnly as u32,
            _reserved: 0,
        }
    }
}

impl Clone for MsgDescCold {
    fn clone(&self) -> Self {
        Self {
            msg_id: self.msg_id,
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            timestamp_ns: self.timestamp_ns,
            debug_level: self.debug_level,
            _reserved: self._reserved,
        }
    }
}

impl TelemetryEvent {
    /// Create a new telemetry event
    pub fn new(
        timestamp_ns: u64,
        trace_ctx: TraceContext,
        channel_id: u32,
        event_type: EventType,
        payload_len: u32,
        latency_ns: u32,
    ) -> Self {
        Self {
            timestamp_ns,
            trace_id: trace_ctx.trace_id,
            span_id: trace_ctx.span_id,
            channel_id,
            event_type: event_type as u32,
            payload_len,
            latency_ns,
        }
    }

    /// Get the event type
    pub fn event_type(&self) -> Option<EventType> {
        match self.event_type {
            0 => Some(EventType::Send),
            1 => Some(EventType::Recv),
            2 => Some(EventType::Cancel),
            3 => Some(EventType::Timeout),
            4 => Some(EventType::Error),
            _ => None,
        }
    }
}

impl Default for TelemetryEvent {
    fn default() -> Self {
        Self {
            timestamp_ns: 0,
            trace_id: 0,
            span_id: 0,
            channel_id: 0,
            event_type: EventType::Send as u32,
            payload_len: 0,
            latency_ns: 0,
        }
    }
}

impl ChannelMetrics {
    /// Create new zeroed channel metrics
    pub fn new() -> Self {
        Self {
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            flow_control_stalls: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }

    /// Record a message send
    pub fn record_send(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message receive
    pub fn record_recv(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a flow control stall
    pub fn record_flow_control_stall(&self) {
        self.flow_control_stalls.fetch_add(1, Ordering::Relaxed);
    }

    /// Take a non-atomic snapshot of metrics
    pub fn snapshot(&self) -> ChannelMetricsSnapshot {
        ChannelMetricsSnapshot {
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            flow_control_stalls: self.flow_control_stalls.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }

    /// Reset all metrics to zero
    pub fn reset(&self) {
        self.bytes_sent.store(0, Ordering::Relaxed);
        self.bytes_received.store(0, Ordering::Relaxed);
        self.messages_sent.store(0, Ordering::Relaxed);
        self.messages_received.store(0, Ordering::Relaxed);
        self.flow_control_stalls.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
    }
}

impl Default for ChannelMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalMetrics {
    /// Create new zeroed global metrics
    pub fn new() -> Self {
        Self {
            ring_high_watermark: AtomicU64::new(0),
            total_allocations: AtomicU64::new(0),
            allocation_failures: AtomicU64::new(0),
        }
    }

    /// Update ring high watermark if current value is higher
    pub fn update_ring_high_watermark(&self, current: u64) {
        self.ring_high_watermark.fetch_max(current, Ordering::Relaxed);
    }

    /// Record an allocation
    pub fn record_allocation(&self) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an allocation failure
    pub fn record_allocation_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get ring high watermark
    pub fn ring_high_watermark(&self) -> u64 {
        self.ring_high_watermark.load(Ordering::Relaxed)
    }

    /// Get total allocations
    pub fn total_allocations(&self) -> u64 {
        self.total_allocations.load(Ordering::Relaxed)
    }

    /// Get allocation failures
    pub fn allocation_failures(&self) -> u64 {
        self.allocation_failures.load(Ordering::Relaxed)
    }
}

impl Default for GlobalMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl TraceContext {
    /// Create a new trace context
    pub fn new(trace_id: u64, span_id: u64, parent_span_id: u64) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id,
        }
    }

    /// Create a root trace context (no parent)
    pub fn root(trace_id: u64, span_id: u64) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id: 0,
        }
    }

    /// Create a child span with this context as parent
    pub fn child(&self, child_span_id: u64) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: child_span_id,
            parent_span_id: self.span_id,
        }
    }

    /// Check if this is a root span (no parent)
    pub fn is_root(&self) -> bool {
        self.parent_span_id == 0
    }
}

impl DebugLevel {
    /// Check if this level includes cold descriptors
    pub fn has_cold_descriptors(&self) -> bool {
        *self >= DebugLevel::Metadata
    }

    /// Check if this level includes payload mirroring
    pub fn has_payload_mirroring(&self) -> bool {
        *self >= DebugLevel::FullPayload
    }

    /// Check if this level includes fault injection
    pub fn has_fault_injection(&self) -> bool {
        *self >= DebugLevel::FaultInjection
    }
}

impl From<u32> for DebugLevel {
    fn from(value: u32) -> Self {
        match value {
            0 => DebugLevel::MetricsOnly,
            1 => DebugLevel::Metadata,
            2 => DebugLevel::FullPayload,
            _ => DebugLevel::FaultInjection,
        }
    }
}

impl EventType {
    /// Check if this is a receive event
    pub fn is_recv(&self) -> bool {
        *self == EventType::Recv
    }

    /// Check if this is an error-related event
    pub fn is_error(&self) -> bool {
        matches!(self, EventType::Error | EventType::Timeout | EventType::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_desc_cold_size() {
        assert_eq!(std::mem::size_of::<MsgDescCold>(), 64);
        assert_eq!(std::mem::align_of::<MsgDescCold>(), 64);
    }

    #[test]
    fn telemetry_event_size() {
        assert_eq!(std::mem::size_of::<TelemetryEvent>(), 40);
    }

    #[test]
    fn debug_level_conversion() {
        assert_eq!(DebugLevel::from(0), DebugLevel::MetricsOnly);
        assert_eq!(DebugLevel::from(1), DebugLevel::Metadata);
        assert_eq!(DebugLevel::from(2), DebugLevel::FullPayload);
        assert_eq!(DebugLevel::from(3), DebugLevel::FaultInjection);
        assert_eq!(DebugLevel::from(99), DebugLevel::FaultInjection);
    }

    #[test]
    fn debug_level_features() {
        assert!(!DebugLevel::MetricsOnly.has_cold_descriptors());
        assert!(DebugLevel::Metadata.has_cold_descriptors());
        assert!(!DebugLevel::Metadata.has_payload_mirroring());
        assert!(DebugLevel::FullPayload.has_payload_mirroring());
        assert!(DebugLevel::FaultInjection.has_fault_injection());
    }

    #[test]
    fn trace_context_hierarchy() {
        let root = TraceContext::root(1, 10);
        assert!(root.is_root());
        assert_eq!(root.trace_id, 1);
        assert_eq!(root.span_id, 10);

        let child = root.child(20);
        assert!(!child.is_root());
        assert_eq!(child.trace_id, 1);
        assert_eq!(child.span_id, 20);
        assert_eq!(child.parent_span_id, 10);
    }

    #[test]
    fn channel_metrics_operations() {
        let metrics = ChannelMetrics::new();

        metrics.record_send(100);
        metrics.record_send(200);
        metrics.record_recv(150);
        metrics.record_error();
        metrics.record_flow_control_stall();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.bytes_sent, 300);
        assert_eq!(snapshot.messages_sent, 2);
        assert_eq!(snapshot.bytes_received, 150);
        assert_eq!(snapshot.messages_received, 1);
        assert_eq!(snapshot.errors, 1);
        assert_eq!(snapshot.flow_control_stalls, 1);

        metrics.reset();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.bytes_sent, 0);
        assert_eq!(snapshot.messages_sent, 0);
    }

    #[test]
    fn global_metrics_operations() {
        let metrics = GlobalMetrics::new();

        metrics.record_allocation();
        metrics.record_allocation();
        metrics.record_allocation_failure();
        metrics.update_ring_high_watermark(42);
        metrics.update_ring_high_watermark(30); // Should not decrease

        assert_eq!(metrics.total_allocations(), 2);
        assert_eq!(metrics.allocation_failures(), 1);
        assert_eq!(metrics.ring_high_watermark(), 42);
    }

    #[test]
    fn msg_desc_cold_roundtrip() {
        let trace_ctx = TraceContext::new(123, 456, 789);
        let cold = MsgDescCold::new(1, trace_ctx, 1000, DebugLevel::Metadata);

        assert_eq!(cold.msg_id, 1);
        assert_eq!(cold.trace_id, 123);
        assert_eq!(cold.span_id, 456);
        assert_eq!(cold.parent_span_id, 789);
        assert_eq!(cold.timestamp_ns, 1000);
        assert_eq!(cold.debug_level().unwrap(), DebugLevel::Metadata);

        let retrieved = cold.trace_context();
        assert_eq!(retrieved.trace_id, 123);
        assert_eq!(retrieved.span_id, 456);
        assert_eq!(retrieved.parent_span_id, 789);
    }

    #[test]
    fn telemetry_event_roundtrip() {
        let trace_ctx = TraceContext::root(555, 666);
        let event = TelemetryEvent::new(
            2000,
            trace_ctx,
            42,
            EventType::Recv,
            1024,
            150,
        );

        assert_eq!(event.timestamp_ns, 2000);
        assert_eq!(event.trace_id, 555);
        assert_eq!(event.span_id, 666);
        assert_eq!(event.channel_id, 42);
        assert_eq!(event.event_type().unwrap(), EventType::Recv);
        assert_eq!(event.payload_len, 1024);
        assert_eq!(event.latency_ns, 150);
    }

    #[test]
    fn event_type_checks() {
        assert!(EventType::Recv.is_recv());
        assert!(!EventType::Send.is_recv());
        assert!(EventType::Error.is_error());
        assert!(EventType::Timeout.is_error());
        assert!(EventType::Cancel.is_error());
        assert!(!EventType::Send.is_error());
    }
}
