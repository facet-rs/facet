//! Tracing/logging infrastructure for dodeca
//!
//! Provides:
//! - TUI layer that routes log events to the Activity panel
//! - Dynamic filtering with salsa debug toggle
//! - Standard env filter for non-TUI mode

use crate::tui::{LogEvent, LogLevel};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::{
    Layer,
    filter::EnvFilter,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
};

/// A tracing layer that sends formatted events to a channel (for TUI Activity panel)
pub struct TuiLayer {
    tx: Sender<LogEvent>,
    salsa_debug: Arc<AtomicBool>,
}

impl TuiLayer {
    pub fn new(tx: Sender<LogEvent>) -> Self {
        Self {
            tx,
            salsa_debug: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a handle to update the filter dynamically
    pub fn filter_handle(&self) -> FilterHandle {
        FilterHandle {
            salsa_debug: self.salsa_debug.clone(),
        }
    }
}

impl<S> Layer<S> for TuiLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = *metadata.level();
        let target = metadata.target();

        // Filter salsa events (they use INFO and DEBUG levels)
        if target.starts_with("salsa") {
            if !self.salsa_debug.load(Ordering::Relaxed) {
                return;
            }
        } else {
            // Only show ERROR, WARN, and INFO - filter out DEBUG and TRACE
            match level {
                Level::ERROR | Level::WARN | Level::INFO => {}
                Level::DEBUG | Level::TRACE => return,
            }
        }

        // Format the event
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let msg = if let Some(message) = visitor.message {
            message
        } else {
            format!("{}: {}", target, metadata.name())
        };

        // Convert tracing Level to our LogLevel
        let log_level = match level {
            Level::ERROR => LogLevel::Error,
            Level::WARN => LogLevel::Warn,
            Level::INFO => LogLevel::Info,
            Level::DEBUG => LogLevel::Debug,
            Level::TRACE => LogLevel::Trace,
        };

        let _ = self.tx.send(LogEvent {
            level: log_level,
            message: msg,
        });
    }
}

/// Handle for dynamically updating the log filter
#[derive(Clone)]
pub struct FilterHandle {
    salsa_debug: Arc<AtomicBool>,
}

impl FilterHandle {
    /// Toggle salsa debug logging, returns new state
    pub fn toggle_salsa_debug(&self) -> bool {
        // Toggle and return the new value
        !self.salsa_debug.fetch_xor(true, Ordering::Relaxed)
    }

    /// Check if salsa debug is currently enabled
    pub fn is_salsa_debug_enabled(&self) -> bool {
        self.salsa_debug.load(Ordering::Relaxed)
    }
}

/// Visitor to extract the message field from an event
#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }
}

/// Initialize tracing for TUI mode
/// Returns a FilterHandle for dynamic filter updates
/// Starts with salsa debug disabled - use 'd' key to toggle
pub fn init_tui_tracing(event_tx: Sender<LogEvent>) -> FilterHandle {
    let tui_layer = TuiLayer::new(event_tx);
    let handle = tui_layer.filter_handle();

    tracing_subscriber::registry().with(tui_layer).init();

    handle
}

/// Initialize tracing for non-TUI mode (uses RUST_LOG env var)
pub fn init_standard_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_filter(filter),
        )
        .init();
}
