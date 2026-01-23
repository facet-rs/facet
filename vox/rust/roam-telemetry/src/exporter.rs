//! Span exporters for roam-telemetry.
//!
//! Provides:
//! - [`OtlpExporter`] - batches and sends spans to an OTLP HTTP endpoint
//! - [`LoggingExporter`] - prints spans to the console (for development/debugging)

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::otlp::{
    ExportTraceServiceRequest, InstrumentationScope, KeyValue, Resource, ResourceSpans, ScopeSpans,
    Span,
};

// ============================================================================
// Exporter Trait
// ============================================================================

/// Trait for span exporters.
///
/// Implement this to create custom exporters (e.g., for testing or custom backends).
pub trait SpanExporter: Clone + Send + Sync + 'static {
    /// Queue a span for export.
    fn send(&self, span: Span);

    /// Get the service name.
    fn service_name(&self) -> &str;
}

// ============================================================================
// Logging Exporter (for development/debugging)
// ============================================================================

/// A simple exporter that logs spans to the console.
///
/// Useful for development and debugging when you don't have an OTLP collector.
///
/// # Example
///
/// ```ignore
/// use roam_telemetry::{LoggingExporter, TelemetryMiddleware};
///
/// let exporter = LoggingExporter::new("my-service");
/// let telemetry = TelemetryMiddleware::new(exporter);
/// ```
#[derive(Clone)]
pub struct LoggingExporter {
    service_name: String,
}

impl LoggingExporter {
    /// Create a new logging exporter with the given service name.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }

    /// Queue a span for logging.
    ///
    /// The span is logged immediately using the `tracing` crate.
    pub fn send(&self, span: Span) {
        // Format attributes as key=value pairs
        let attrs: Vec<String> = span
            .attributes
            .iter()
            .map(|kv| {
                let value = kv
                    .value
                    .string_value
                    .as_ref()
                    .cloned()
                    .or_else(|| kv.value.int_value.as_ref().cloned())
                    .or_else(|| kv.value.bool_value.map(|b| b.to_string()))
                    .unwrap_or_else(|| "?".to_string());
                format!("{}={}", kv.key, value)
            })
            .collect();

        let duration_ns = span
            .end_time_unix_nano
            .parse::<u64>()
            .unwrap_or(0)
            .saturating_sub(span.start_time_unix_nano.parse::<u64>().unwrap_or(0));
        let duration_ms = duration_ns as f64 / 1_000_000.0;

        let parent = span
            .parent_span_id
            .as_ref()
            .map(|p| format!(" parent={}", p))
            .unwrap_or_default();

        tracing::info!(
            target: "roam_telemetry",
            "[{}] {} trace={} span={}{} {:.2}ms [{}]",
            self.service_name,
            span.name,
            &span.trace_id[..8],
            &span.span_id[..8],
            parent,
            duration_ms,
            attrs.join(" "),
        );
    }

    /// Get the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

impl SpanExporter for LoggingExporter {
    fn send(&self, span: Span) {
        self.send(span)
    }

    fn service_name(&self) -> &str {
        self.service_name()
    }
}

// ============================================================================
// OTLP HTTP Exporter
// ============================================================================

/// Configuration for the OTLP exporter.
#[derive(Debug, Clone)]
pub struct ExporterConfig {
    /// OTLP HTTP endpoint (e.g., "http://tempo:4318/v1/traces").
    pub endpoint: String,
    /// Service name for resource attributes.
    pub service_name: String,
    /// Additional resource attributes.
    pub resource_attributes: Vec<KeyValue>,
    /// Maximum batch size before sending.
    pub max_batch_size: usize,
    /// Maximum time to wait before sending a batch.
    pub max_batch_delay: Duration,
    /// HTTP timeout for export requests.
    pub timeout: Duration,
}

impl Default for ExporterConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4318/v1/traces".to_string(),
            service_name: "unknown".to_string(),
            resource_attributes: Vec::new(),
            max_batch_size: 512,
            max_batch_delay: Duration::from_secs(5),
            timeout: Duration::from_secs(10),
        }
    }
}

/// OTLP HTTP exporter.
///
/// Collects spans via an async channel and batches them for export.
/// The exporter runs a background task that:
/// - Collects spans until batch is full or timeout expires
/// - Serializes batch to JSON using facet-json
/// - POSTs to the OTLP endpoint
#[derive(Clone)]
pub struct OtlpExporter {
    tx: mpsc::Sender<Span>,
    config: Arc<ExporterConfig>,
}

impl OtlpExporter {
    /// Create a new exporter with the given endpoint and service name.
    ///
    /// This starts a background task that batches and exports spans.
    /// The task runs until the exporter is dropped.
    pub fn new(endpoint: impl Into<String>, service_name: impl Into<String>) -> Self {
        Self::with_config(ExporterConfig {
            endpoint: endpoint.into(),
            service_name: service_name.into(),
            ..Default::default()
        })
    }

    /// Create a new exporter with full configuration.
    pub fn with_config(config: ExporterConfig) -> Self {
        let (tx, rx) = mpsc::channel(4096);
        let config = Arc::new(config);

        // Spawn the background export task
        let config_clone = config.clone();
        tokio::spawn(async move {
            export_loop(rx, config_clone).await;
        });

        Self { tx, config }
    }

    /// Queue a span for export.
    ///
    /// This is non-blocking. If the channel is full, the span is dropped.
    pub fn send(&self, span: Span) {
        // Use try_send to avoid blocking - if buffer is full, drop the span
        let _ = self.tx.try_send(span);
    }

    /// Get the service name.
    pub fn service_name(&self) -> &str {
        &self.config.service_name
    }
}

impl SpanExporter for OtlpExporter {
    fn send(&self, span: Span) {
        self.send(span)
    }

    fn service_name(&self) -> &str {
        self.service_name()
    }
}

async fn export_loop(mut rx: mpsc::Receiver<Span>, config: Arc<ExporterConfig>) {
    let client = reqwest::Client::builder()
        .timeout(config.timeout)
        .build()
        .expect("failed to create HTTP client");

    let mut batch = Vec::with_capacity(config.max_batch_size);
    let mut interval = tokio::time::interval(config.max_batch_delay);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Receive spans
            span = rx.recv() => {
                match span {
                    Some(span) => {
                        batch.push(span);
                        if batch.len() >= config.max_batch_size {
                            export_batch(&client, &config, &mut batch).await;
                        }
                    }
                    None => {
                        // Channel closed, export remaining and exit
                        if !batch.is_empty() {
                            export_batch(&client, &config, &mut batch).await;
                        }
                        break;
                    }
                }
            }
            // Periodic flush
            _ = interval.tick() => {
                if !batch.is_empty() {
                    export_batch(&client, &config, &mut batch).await;
                }
            }
        }
    }
}

async fn export_batch(client: &reqwest::Client, config: &ExporterConfig, batch: &mut Vec<Span>) {
    if batch.is_empty() {
        return;
    }

    // Build resource attributes
    let mut attributes = vec![KeyValue::string("service.name", &config.service_name)];
    attributes.extend(config.resource_attributes.iter().cloned());

    // Build the export request
    let request = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Resource { attributes },
            scope_spans: vec![ScopeSpans {
                scope: InstrumentationScope {
                    name: "roam-telemetry".to_string(),
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                },
                spans: std::mem::take(batch),
            }],
        }],
    };

    // Serialize to JSON
    let json = match facet_json::to_string(&request) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("failed to serialize spans: {}", e);
            return;
        }
    };

    // Send to endpoint
    match client
        .post(&config.endpoint)
        .header("Content-Type", "application/json")
        .body(json)
        .send()
        .await
    {
        Ok(response) => {
            if !response.status().is_success() {
                tracing::warn!(
                    "OTLP export failed: {} {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
            }
        }
        Err(e) => {
            tracing::warn!("OTLP export error: {}", e);
        }
    }
}
