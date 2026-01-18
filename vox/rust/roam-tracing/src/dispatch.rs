//! Host-side dispatch of cell tracing records through the local tracing system.
//!
//! This module provides the machinery to convert `TracingRecord` from cells
//! into proper `tracing` events that flow through the host's subscriber.
//!
//! The challenge is that `tracing` requires compile-time field names, but
//! cell records have dynamic fields. We solve this by:
//!
//! 1. Creating static callsites (one per log level) with known field names
//! 2. Serializing dynamic fields into a "fields" string field
//! 3. Using `Event::dispatch()` to emit events through the subscriber

use std::sync::OnceLock;

use tracing_core::callsite::Identifier;
use tracing_core::field::{Field, FieldSet, Value};
use tracing_core::metadata::Kind;
use tracing_core::{Event, Metadata};

use crate::{FieldValue, Level, TaggedRecord, TracingRecord};

// ============================================================================
// Field Names
// ============================================================================

/// Known field names for cell events.
/// - `message`: The log message
/// - `cell`: The cell/peer name
/// - `target`: The tracing target (module path)
/// - `fields`: Serialized key=value pairs from dynamic fields
static FIELD_NAMES: &[&str] = &["message", "cell", "target", "fields"];

// ============================================================================
// Static Callsites (one per level)
// ============================================================================

macro_rules! define_callsite {
    ($level:expr, $cs_type:ident, $cs_static:ident, $meta:ident, $fields:ident) => {
        struct $cs_type;
        static $cs_static: $cs_type = $cs_type;

        static $meta: Metadata<'static> = Metadata::new(
            "cell event",
            "roam_tracing::cell",
            $level,
            Some(file!()),
            Some(line!()),
            Some(module_path!()),
            FieldSet::new(FIELD_NAMES, Identifier(&$cs_static)),
            Kind::EVENT,
        );

        impl tracing_core::callsite::Callsite for $cs_type {
            fn set_interest(&self, _: tracing_core::subscriber::Interest) {}
            fn metadata(&self) -> &'static Metadata<'static> {
                &$meta
            }
        }

        static $fields: OnceLock<Fields> = OnceLock::new();
    };
}

define_callsite!(
    tracing_core::Level::ERROR,
    CsError,
    CS_ERROR,
    META_ERROR,
    FIELDS_ERROR
);
define_callsite!(
    tracing_core::Level::WARN,
    CsWarn,
    CS_WARN,
    META_WARN,
    FIELDS_WARN
);
define_callsite!(
    tracing_core::Level::INFO,
    CsInfo,
    CS_INFO,
    META_INFO,
    FIELDS_INFO
);
define_callsite!(
    tracing_core::Level::DEBUG,
    CsDebug,
    CS_DEBUG,
    META_DEBUG,
    FIELDS_DEBUG
);
define_callsite!(
    tracing_core::Level::TRACE,
    CsTrace,
    CS_TRACE,
    META_TRACE,
    FIELDS_TRACE
);

/// Cached field references for a callsite.
struct Fields {
    message: Field,
    cell: Field,
    target: Field,
    fields: Field,
}

impl Fields {
    fn new(meta: &'static Metadata<'static>) -> Self {
        let fs = meta.fields();
        Self {
            message: fs.field("message").expect("message field"),
            cell: fs.field("cell").expect("cell field"),
            target: fs.field("target").expect("target field"),
            fields: fs.field("fields").expect("fields field"),
        }
    }
}

fn get_level_components(
    level: Level,
) -> (
    &'static Metadata<'static>,
    &'static OnceLock<Fields>,
    &'static dyn tracing_core::callsite::Callsite,
) {
    match level {
        Level::Error => (&META_ERROR, &FIELDS_ERROR, &CS_ERROR),
        Level::Warn => (&META_WARN, &FIELDS_WARN, &CS_WARN),
        Level::Info => (&META_INFO, &FIELDS_INFO, &CS_INFO),
        Level::Debug => (&META_DEBUG, &FIELDS_DEBUG, &CS_DEBUG),
        Level::Trace => (&META_TRACE, &FIELDS_TRACE, &CS_TRACE),
    }
}

// ============================================================================
// Field Value Formatting
// ============================================================================

/// Formats dynamic fields as a string: `key1=value1 key2=value2`
fn format_fields(fields: &[(String, FieldValue)]) -> String {
    if fields.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    for (i, (key, value)) in fields.iter().enumerate() {
        if i > 0 {
            result.push(' ');
        }
        result.push_str(key);
        result.push('=');
        match value {
            FieldValue::Bool(v) => result.push_str(&v.to_string()),
            FieldValue::I64(v) => result.push_str(&v.to_string()),
            FieldValue::U64(v) => result.push_str(&v.to_string()),
            FieldValue::Str(v) => {
                // Quote strings that contain spaces
                if v.contains(' ') {
                    result.push('"');
                    result.push_str(v);
                    result.push('"');
                } else {
                    result.push_str(v);
                }
            }
        }
    }
    result
}

// ============================================================================
// Public API
// ============================================================================

/// Dispatch a tagged tracing record through the host's tracing subscriber.
///
/// This converts the `TaggedRecord` into a proper `tracing::Event` and
/// dispatches it through the current subscriber, preserving:
/// - Log level
/// - Message
/// - Cell/peer name
/// - Target (module path)
/// - Dynamic fields (serialized as key=value pairs)
///
/// # Example
///
/// ```ignore
/// use roam_tracing::dispatch_record;
///
/// // In your tracing consumer task:
/// while let Some(tagged) = tracing_rx.recv().await {
///     dispatch_record(&tagged);
/// }
/// ```
pub fn dispatch_record(tagged: &TaggedRecord) {
    match &tagged.record {
        TracingRecord::Event {
            level,
            target,
            message,
            fields,
            ..
        } => {
            dispatch_event(
                *level,
                target,
                message.as_deref().unwrap_or(""),
                tagged
                    .peer_name
                    .as_deref()
                    .unwrap_or(&format!("peer-{}", tagged.peer_id)),
                fields,
            );
        }
        TracingRecord::SpanEnter { name, level, .. } => {
            // Emit span enter as a debug event
            if *level >= Level::Debug {
                let msg = format!("-> {}", name);
                dispatch_event(
                    Level::Debug,
                    "roam_tracing::span",
                    &msg,
                    tagged
                        .peer_name
                        .as_deref()
                        .unwrap_or(&format!("peer-{}", tagged.peer_id)),
                    &[],
                );
            }
        }
        TracingRecord::SpanExit { .. } | TracingRecord::SpanClose { .. } => {
            // Usually too verbose to emit
        }
    }
}

/// Dispatch an event with the given parameters through the tracing system.
fn dispatch_event(
    level: Level,
    target: &str,
    message: &str,
    cell: &str,
    fields: &[(String, FieldValue)],
) {
    // Register callsites on first use
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        tracing_core::callsite::register(&CS_ERROR);
        tracing_core::callsite::register(&CS_WARN);
        tracing_core::callsite::register(&CS_INFO);
        tracing_core::callsite::register(&CS_DEBUG);
        tracing_core::callsite::register(&CS_TRACE);
    });

    let (meta, fields_lock, _cs) = get_level_components(level);
    let keys = fields_lock.get_or_init(|| Fields::new(meta));

    // Format dynamic fields
    let fields_str = format_fields(fields);

    // Build value set - use references to &str which implement Value
    let values: [(&Field, Option<&dyn Value>); 4] = [
        (&keys.message, Some(&message as &dyn Value)),
        (&keys.cell, Some(&cell as &dyn Value)),
        (&keys.target, Some(&target as &dyn Value)),
        (&keys.fields, Some(&fields_str.as_str() as &dyn Value)),
    ];

    let value_set = meta.fields().value_set(&values);
    Event::dispatch(meta, &value_set);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_fields_empty() {
        assert_eq!(format_fields(&[]), "");
    }

    #[test]
    fn test_format_fields_simple() {
        let fields = vec![
            ("key1".to_string(), FieldValue::I64(42)),
            ("key2".to_string(), FieldValue::Str("value".to_string())),
        ];
        assert_eq!(format_fields(&fields), "key1=42 key2=value");
    }

    #[test]
    fn test_format_fields_with_spaces() {
        let fields = vec![(
            "msg".to_string(),
            FieldValue::Str("hello world".to_string()),
        )];
        assert_eq!(format_fields(&fields), "msg=\"hello world\"");
    }
}
