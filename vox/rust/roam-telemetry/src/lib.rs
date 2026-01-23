//! Lightweight OTLP telemetry for roam RPC.
//!
//! This crate provides OpenTelemetry-compatible tracing without the heavy
//! `opentelemetry` crate dependency. It sends traces directly to an OTLP
//! HTTP endpoint (like Grafana Tempo) using reqwest and facet-json.
//!
//! # Example
//!
//! ```ignore
//! use roam_telemetry::{TelemetryMiddleware, OtlpExporter};
//!
//! // Create exporter pointing to Tempo
//! let exporter = OtlpExporter::new("http://tempo:4318/v1/traces", "my-service");
//!
//! // Create middleware
//! let telemetry = TelemetryMiddleware::new(exporter);
//!
//! // Add to dispatcher
//! let dispatcher = MyServiceDispatcher::new(handler)
//!     .with_middleware(telemetry);
//! ```

mod client;
mod exporter;
mod middleware;
mod otlp;

pub use client::{CurrentTrace, TracingCaller};
pub use exporter::{ExporterConfig, LoggingExporter, OtlpExporter, SpanExporter};
pub use middleware::{PendingSpan, TelemetryMiddleware};
pub use otlp::{KeyValue, Span, SpanKind, StatusCode, TraceContext};
