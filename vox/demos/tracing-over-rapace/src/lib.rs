//! Tracing over Rapace - Shared Library
//!
//! This module contains utilities shared between the main demo binary
//! and the cross-process test helper.

// Re-export from rapace-tracing for convenience
pub use rapace_tracing::{
    create_tracing_sink_dispatcher, EventMeta, Field, HostTracingSink, RapaceTracingLayer,
    SpanMeta, TraceRecord, TracingSink, TracingSinkClient, TracingSinkServer,
};
