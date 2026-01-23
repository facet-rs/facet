//! OTLP JSON types for trace export.
//!
//! These match the OpenTelemetry Protocol JSON format:
//! <https://opentelemetry.io/docs/specs/otlp/#json-protobuf-encoding>

use facet::Facet;

/// Root request for exporting traces.
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ExportTraceServiceRequest {
    pub resource_spans: Vec<ResourceSpans>,
}

/// Spans grouped by resource (service).
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ResourceSpans {
    pub resource: Resource,
    pub scope_spans: Vec<ScopeSpans>,
}

/// Resource attributes (service.name, etc).
#[derive(Debug, Clone, Facet)]
pub struct Resource {
    pub attributes: Vec<KeyValue>,
}

/// Spans grouped by instrumentation scope.
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ScopeSpans {
    pub scope: InstrumentationScope,
    pub spans: Vec<Span>,
}

/// Instrumentation scope (library name/version).
#[derive(Debug, Clone, Facet)]
pub struct InstrumentationScope {
    pub name: String,
    #[facet(default)]
    pub version: Option<String>,
}

/// A single span.
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct Span {
    /// 16-byte trace ID as hex string (32 chars).
    pub trace_id: String,
    /// 8-byte span ID as hex string (16 chars).
    pub span_id: String,
    /// Parent span ID (if any).
    #[facet(default)]
    pub parent_span_id: Option<String>,
    /// Span name (e.g., "Testbed.echo").
    pub name: String,
    /// Span kind (CLIENT, SERVER, etc).
    pub kind: u32,
    /// Start time in nanoseconds since Unix epoch.
    pub start_time_unix_nano: String,
    /// End time in nanoseconds since Unix epoch.
    pub end_time_unix_nano: String,
    /// Span attributes.
    #[facet(default)]
    pub attributes: Vec<KeyValue>,
    /// Span status.
    pub status: Status,
}

/// Span kind values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SpanKind {
    Unspecified = 0,
    Internal = 1,
    Server = 2,
    Client = 3,
    Producer = 4,
    Consumer = 5,
}

impl SpanKind {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Key-value attribute.
#[derive(Debug, Clone, Facet)]
pub struct KeyValue {
    pub key: String,
    pub value: AnyValue,
}

impl KeyValue {
    pub fn string(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: AnyValue {
                string_value: Some(value.into()),
                int_value: None,
                bool_value: None,
            },
        }
    }

    pub fn int(key: impl Into<String>, value: i64) -> Self {
        Self {
            key: key.into(),
            value: AnyValue {
                string_value: None,
                int_value: Some(value.to_string()),
                bool_value: None,
            },
        }
    }

    pub fn bool(key: impl Into<String>, value: bool) -> Self {
        Self {
            key: key.into(),
            value: AnyValue {
                string_value: None,
                int_value: None,
                bool_value: Some(value),
            },
        }
    }
}

/// Attribute value (only one field should be set).
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct AnyValue {
    #[facet(default)]
    pub string_value: Option<String>,
    /// OTLP uses string for int64 to avoid JS precision loss.
    #[facet(default)]
    pub int_value: Option<String>,
    #[facet(default)]
    pub bool_value: Option<bool>,
}

/// Span status.
#[derive(Debug, Clone, Facet)]
pub struct Status {
    /// Status code.
    pub code: u32,
    /// Optional message (usually for errors).
    #[facet(default)]
    pub message: Option<String>,
}

/// Status code values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum StatusCode {
    Unset = 0,
    Ok = 1,
    Error = 2,
}

impl StatusCode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: StatusCode::Ok.as_u32(),
            message: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            code: StatusCode::Error.as_u32(),
            message: Some(message.into()),
        }
    }

    pub fn unset() -> Self {
        Self {
            code: StatusCode::Unset.as_u32(),
            message: None,
        }
    }
}

// ============================================================================
// W3C Trace Context
// ============================================================================

/// W3C Trace Context extracted from or injected into metadata.
///
/// Format: `traceparent: 00-{trace_id}-{parent_span_id}-{flags}`
/// - version: 2 hex chars (always "00")
/// - trace_id: 32 hex chars (16 bytes)
/// - parent_span_id: 16 hex chars (8 bytes)
/// - flags: 2 hex chars (01 = sampled)
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// 16-byte trace ID as 32 hex chars.
    pub trace_id: String,
    /// 8-byte parent span ID as 16 hex chars.
    pub parent_span_id: String,
    /// Trace flags (01 = sampled).
    pub flags: u8,
}

impl TraceContext {
    /// The metadata key for W3C traceparent.
    pub const TRACEPARENT_KEY: &'static str = "traceparent";

    /// Parse from a traceparent header value.
    ///
    /// Format: `00-{trace_id}-{span_id}-{flags}`
    pub fn parse(traceparent: &str) -> Option<Self> {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        let version = parts[0];
        let trace_id = parts[1];
        let parent_span_id = parts[2];
        let flags_str = parts[3];

        // Version must be "00"
        if version != "00" {
            return None;
        }

        // Validate lengths
        if trace_id.len() != 32 || parent_span_id.len() != 16 || flags_str.len() != 2 {
            return None;
        }

        // Validate hex
        if !trace_id.chars().all(|c| c.is_ascii_hexdigit())
            || !parent_span_id.chars().all(|c| c.is_ascii_hexdigit())
            || !flags_str.chars().all(|c| c.is_ascii_hexdigit())
        {
            return None;
        }

        let flags = u8::from_str_radix(flags_str, 16).ok()?;

        Some(Self {
            trace_id: trace_id.to_lowercase(),
            parent_span_id: parent_span_id.to_lowercase(),
            flags,
        })
    }

    /// Format as a traceparent header value.
    pub fn to_traceparent(&self, span_id: &str) -> String {
        format!("00-{}-{}-{:02x}", self.trace_id, span_id, self.flags)
    }

    /// Create a new trace context with a fresh trace ID.
    pub fn new_root() -> Self {
        Self {
            trace_id: generate_trace_id(),
            parent_span_id: String::new(), // No parent for root
            flags: 0x01,                   // Sampled
        }
    }

    /// Is this trace sampled?
    pub fn is_sampled(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

/// Generate a random 16-byte trace ID as 32 hex chars.
pub fn generate_trace_id() -> String {
    let bytes: [u8; 16] = rand::random();
    hex_encode(&bytes)
}

/// Generate a random 8-byte span ID as 16 hex chars.
pub fn generate_span_id() -> String {
    let bytes: [u8; 8] = rand::random();
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_traceparent() {
        let tp = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = TraceContext::parse(tp).unwrap();
        assert_eq!(ctx.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.parent_span_id, "b7ad6b7169203331");
        assert_eq!(ctx.flags, 0x01);
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_parse_traceparent_not_sampled() {
        let tp = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-00";
        let ctx = TraceContext::parse(tp).unwrap();
        assert!(!ctx.is_sampled());
    }

    #[test]
    fn test_parse_invalid() {
        assert!(TraceContext::parse("invalid").is_none());
        assert!(TraceContext::parse("01-abc-def-00").is_none()); // wrong version
        assert!(TraceContext::parse("00-short-span-00").is_none()); // wrong length
    }

    #[test]
    fn test_to_traceparent() {
        let ctx = TraceContext {
            trace_id: "0af7651916cd43dd8448eb211c80319c".to_string(),
            parent_span_id: "b7ad6b7169203331".to_string(),
            flags: 0x01,
        };
        let span_id = "00f067aa0ba902b7";
        assert_eq!(
            ctx.to_traceparent(span_id),
            "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01"
        );
    }

    #[test]
    fn test_generate_ids() {
        let trace_id = generate_trace_id();
        assert_eq!(trace_id.len(), 32);
        assert!(trace_id.chars().all(|c| c.is_ascii_hexdigit()));

        let span_id = generate_span_id();
        assert_eq!(span_id.len(), 16);
        assert!(span_id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
