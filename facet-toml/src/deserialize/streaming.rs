//! Streaming TOML deserializer using toml_parser's push-based events.
//!
//! This module implements the architecture described in [`crate::design`]:
//! - Always uses deferred materialization (TOML keys can come in any order)
//! - Buffers events during flatten disambiguation
//! - Uses facet-solver for variant resolution

use alloc::{
    borrow::{Cow, ToOwned},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::ops::Range;
use facet_core::{
    Def, DynDateTimeKind, Facet, FieldError, Shape, StructKind, StructType, Type, UserType, Variant,
};
use facet_reflect::{Partial, ReflectError, Resolution, ScalarType, VariantSelection};
use facet_solver::{KeyResult, Schema, Solver, VariantsByFormat};
use log::trace;
use toml_parser::{
    ErrorSink, Raw, Source,
    decoder::ScalarKind,
    parser::{Event, EventKind, RecursionGuard, parse_document},
};

use super::TomlDeError;
use super::TomlDeErrorKind;

// ============================================================================
// Error collection for parsing
// ============================================================================

/// Collects parse errors from the TOML parser
struct ParseErrorCollector {
    error: Option<String>,
}

impl ParseErrorCollector {
    fn new() -> Self {
        Self { error: None }
    }

    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

impl ErrorSink for ParseErrorCollector {
    fn report_error(&mut self, error: toml_parser::ParseError) {
        if self.error.is_none() {
            self.error = Some(error.description().to_string());
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a TOML datetime string into components.
///
/// Returns `(year, month, day, hour, minute, second, nanos, offset_minutes, kind)`.
/// - For LocalTime, year/month/day are 0.
/// - For LocalDate, hour/minute/second/nanos are 0.
/// - offset_minutes is Some only for Offset datetimes.
#[derive(Debug)]
enum ParsedDateTime {
    Offset {
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
        offset_minutes: i16,
    },
    LocalDateTime {
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
    },
    LocalDate {
        year: i32,
        month: u8,
        day: u8,
    },
    LocalTime {
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
    },
}

fn parse_toml_datetime(s: &str) -> Option<ParsedDateTime> {
    // Check if this is a local time (starts with HH:MM)
    if s.len() >= 5 && s.as_bytes()[2] == b':' && !s.contains('-') {
        // Local time: HH:MM:SS[.fractional]
        return parse_local_time(s);
    }

    // Must start with a date (YYYY-MM-DD)
    if s.len() < 10 {
        return None;
    }

    let year = s[0..4].parse::<i32>().ok()?;
    if s.as_bytes()[4] != b'-' {
        return None;
    }
    let month = s[5..7].parse::<u8>().ok()?;
    if s.as_bytes()[7] != b'-' {
        return None;
    }
    let day = s[8..10].parse::<u8>().ok()?;

    // If that's all, it's a local date
    if s.len() == 10 {
        return Some(ParsedDateTime::LocalDate { year, month, day });
    }

    // Must have 'T' or ' ' separator for datetime
    let sep = s.as_bytes()[10];
    if sep != b'T' && sep != b't' && sep != b' ' {
        return None;
    }

    // Parse time part - minimum is HH:MM (5 chars)
    let time_part = &s[11..];
    if time_part.len() < 5 {
        return None;
    }

    let hour = time_part[0..2].parse::<u8>().ok()?;
    if time_part.as_bytes()[2] != b':' {
        return None;
    }
    let minute = time_part[3..5].parse::<u8>().ok()?;

    // Seconds are optional in TOML
    let (second, rest) = if time_part.len() > 5 && time_part.as_bytes()[5] == b':' {
        if time_part.len() < 8 {
            return None;
        }
        let sec = time_part[6..8].parse::<u8>().ok()?;
        (sec, &time_part[8..])
    } else {
        (0u8, &time_part[5..])
    };

    // Parse optional fractional seconds and offset
    let (nanos, offset_rest) = parse_fractional_and_offset(rest);

    match offset_rest {
        Some(offset_minutes) => Some(ParsedDateTime::Offset {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanos,
            offset_minutes,
        }),
        None => Some(ParsedDateTime::LocalDateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanos,
        }),
    }
}

fn parse_local_time(s: &str) -> Option<ParsedDateTime> {
    // Minimum is HH:MM (5 chars)
    if s.len() < 5 {
        return None;
    }

    let hour = s[0..2].parse::<u8>().ok()?;
    if s.as_bytes()[2] != b':' {
        return None;
    }
    let minute = s[3..5].parse::<u8>().ok()?;

    // Seconds are optional in TOML
    let (second, rest) = if s.len() > 5 && s.as_bytes()[5] == b':' {
        if s.len() < 8 {
            return None;
        }
        let sec = s[6..8].parse::<u8>().ok()?;
        (sec, &s[8..])
    } else {
        (0, &s[5..])
    };

    // Parse optional fractional seconds
    let nanos = if let Some(stripped) = rest.strip_prefix('.') {
        parse_nanos(stripped)
    } else {
        0
    };

    Some(ParsedDateTime::LocalTime {
        hour,
        minute,
        second,
        nanos,
    })
}

fn parse_fractional_and_offset(s: &str) -> (u32, Option<i16>) {
    if s.is_empty() {
        return (0, None);
    }

    let (nanos, rest) = if let Some(stripped) = s.strip_prefix('.') {
        // Find where fractional part ends
        let frac_end = stripped
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(stripped.len());
        let nanos = parse_nanos(&stripped[..frac_end]);
        (nanos, &stripped[frac_end..])
    } else {
        (0, s)
    };

    // Parse offset
    let offset = if rest.is_empty() {
        None
    } else if rest == "Z" || rest == "z" {
        Some(0i16)
    } else if rest.starts_with('+') || rest.starts_with('-') {
        parse_offset(rest)
    } else {
        None
    };

    (nanos, offset)
}

fn parse_nanos(s: &str) -> u32 {
    // Take up to 9 digits, pad with zeros
    let digits: String = s.chars().take(9).filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return 0;
    }
    let padded = format!("{digits:0<9}");
    padded.parse().unwrap_or(0)
}

fn parse_offset(s: &str) -> Option<i16> {
    // Format: +HH:MM or -HH:MM or +HH or -HH
    if s.len() < 3 {
        return None;
    }

    let sign: i16 = if s.starts_with('+') { 1 } else { -1 };
    let rest = &s[1..];

    let (hours, minutes) = if rest.len() >= 5 && rest.as_bytes()[2] == b':' {
        let h = rest[0..2].parse::<i16>().ok()?;
        let m = rest[3..5].parse::<i16>().ok()?;
        (h, m)
    } else if rest.len() >= 2 {
        let h = rest[0..2].parse::<i16>().ok()?;
        (h, 0)
    } else {
        return None;
    };

    Some(sign * (hours * 60 + minutes))
}

/// Check if a shape represents `Spanned<T>`.
///
/// Returns `true` if the shape is a struct with exactly two fields:
/// - `value` (the inner value)
/// - `span` (for storing source location)
fn is_spanned_shape(shape: &Shape) -> bool {
    if let Type::User(UserType::Struct(struct_def)) = &shape.ty
        && struct_def.fields.len() == 2
    {
        let has_value = struct_def.fields.iter().any(|f| f.name == "value");
        let has_span = struct_def.fields.iter().any(|f| f.name == "span");
        return has_value && has_span;
    }
    false
}

// ============================================================================
// Field lookup for flattened structs/enums
// ============================================================================

/// Result of finding where a field belongs
#[derive(Debug)]
enum FieldLocation<'a> {
    /// Field is directly on the current struct
    Direct { field_name: &'static str },
    /// Field is inside a flattened struct
    FlattenedStruct {
        /// The flattened field name on the parent struct
        flatten_field_name: &'static str,
        /// The property field name inside the flattened struct
        inner_field_name: &'static str,
    },
    /// Field is inside a flattened enum variant
    FlattenedEnum {
        /// The flattened field name on the parent struct
        flatten_field_name: &'static str,
        /// The variant to select
        variant: &'a VariantSelection,
        /// The property field name inside the variant
        inner_field_name: &'static str,
    },
}

/// Find where a field with the given name belongs.
///
/// This searches:
/// 1. Direct fields on the struct
/// 2. Fields inside flattened structs
/// 3. Fields inside flattened enum variants (using resolution to know which variant)
fn find_field_location<'a>(
    shape: &'static Shape,
    field_name: &str,
    resolution: &'a Resolution,
) -> Option<FieldLocation<'a>> {
    let fields = match shape.ty {
        Type::User(UserType::Struct(StructType { fields, .. })) => fields,
        _ => return None,
    };

    // First check direct fields
    for field in fields {
        if field.name == field_name {
            return Some(FieldLocation::Direct {
                field_name: field.name,
            });
        }
    }

    // Then check flattened fields
    for field in fields {
        if !field.is_flattened() {
            continue;
        }

        let field_shape = field.shape();

        match field_shape.ty {
            // Flattened struct - search inside it
            Type::User(UserType::Struct(StructType {
                fields: inner_fields,
                ..
            })) => {
                for inner_field in inner_fields {
                    if inner_field.name == field_name {
                        return Some(FieldLocation::FlattenedStruct {
                            flatten_field_name: field.name,
                            inner_field_name: inner_field.name,
                        });
                    }
                }
            }
            // Flattened enum - need to find which variant contains this field
            Type::User(UserType::Enum(enum_type)) => {
                // Find the variant selection for this flattened field
                let variant_selection = resolution.variant_selections().iter().find(|vs| {
                    // Match based on the field name in the path
                    vs.path
                        .segments()
                        .first()
                        .is_some_and(|seg| matches!(seg, facet_reflect::PathSegment::Field(name) if *name == field.name))
                });

                if let Some(vs) = variant_selection {
                    // Find the variant by name
                    if let Some(variant) = enum_type
                        .variants
                        .iter()
                        .find(|v| v.name == vs.variant_name)
                    {
                        // Check if the variant has this field
                        // Handle both named fields and newtype variants
                        if variant.data.fields.len() == 1 && variant.data.fields[0].name == "0" {
                            // Newtype variant - look inside the inner type
                            let inner_shape = variant.data.fields[0].shape();
                            if let Type::User(UserType::Struct(StructType {
                                fields: inner_fields,
                                ..
                            })) = inner_shape.ty
                            {
                                for inner_field in inner_fields {
                                    if inner_field.name == field_name {
                                        return Some(FieldLocation::FlattenedEnum {
                                            flatten_field_name: field.name,
                                            variant: vs,
                                            inner_field_name: inner_field.name,
                                        });
                                    }
                                }
                            }
                        } else {
                            // Named fields on the variant
                            for variant_field in variant.data.fields {
                                if variant_field.name == field_name {
                                    return Some(FieldLocation::FlattenedEnum {
                                        flatten_field_name: field.name,
                                        variant: vs,
                                        inner_field_name: variant_field.name,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Iterator over collected events for replay.
///
/// This iterator automatically skips whitespace, comment, and newline events
/// since they are not needed for deserialization.
pub struct EventIter<'a> {
    events: &'a [Event],
    pos: usize,
}

impl<'a> EventIter<'a> {
    /// Create a new event iterator
    pub fn new(events: &'a [Event]) -> Self {
        Self { events, pos: 0 }
    }

    /// Check if an event should be skipped (whitespace, comment, newline)
    #[inline]
    fn should_skip(event: &Event) -> bool {
        matches!(
            event.kind(),
            EventKind::Whitespace | EventKind::Comment | EventKind::Newline
        )
    }

    /// Advance to the next non-skipped position
    fn advance_to_next_valid(&mut self) {
        while self.pos < self.events.len() && Self::should_skip(&self.events[self.pos]) {
            self.pos += 1;
        }
    }

    /// Peek at the next event without consuming it (skips whitespace/comments/newlines)
    pub fn peek(&self) -> Option<&'a Event> {
        let mut pos = self.pos;
        while pos < self.events.len() {
            let event = &self.events[pos];
            if !Self::should_skip(event) {
                return Some(event);
            }
            pos += 1;
        }
        None
    }

    /// Get the current position (for rewinding)
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Rewind to a previous position
    pub fn rewind(&mut self, pos: usize) {
        self.pos = pos;
    }
}

impl<'a> Iterator for EventIter<'a> {
    type Item = &'a Event;

    fn next(&mut self) -> Option<Self::Item> {
        self.advance_to_next_valid();
        if self.pos >= self.events.len() {
            return None;
        }
        let event = &self.events[self.pos];
        self.pos += 1;
        Some(event)
    }
}

// ============================================================================
// Streaming Deserializer
// ============================================================================

/// Default maximum recursion depth for inline tables and arrays.
///
/// This limits how deeply nested inline constructs like `{ a = { b = { c = 1 } } }`
/// can be. Standard table headers `[a.b.c]` are not affected by this limit.
pub const DEFAULT_MAX_RECURSION_DEPTH: u32 = 128;

/// Deserialize TOML using the streaming event-based parser.
///
/// This is the new implementation that:
/// - Uses toml_parser's push-based event stream
/// - Always uses deferred materialization (TOML keys can come in any order)
/// - Handles interleaved dotted keys correctly
/// - Uses facet-solver for flatten disambiguation
pub fn from_str<'input, 'facet, T: Facet<'facet>>(
    toml: &'input str,
) -> Result<T, TomlDeError<'input>> {
    from_str_with_options(toml, DEFAULT_MAX_RECURSION_DEPTH)
}

/// Deserialize TOML with configurable options.
///
/// # Arguments
/// * `toml` - The TOML source string
/// * `max_recursion_depth` - Maximum nesting depth for inline tables/arrays
pub fn from_str_with_options<'input, 'facet, T: Facet<'facet>>(
    toml: &'input str,
    max_recursion_depth: u32,
) -> Result<T, TomlDeError<'input>> {
    trace!("Parsing TOML (streaming)");

    // Parse TOML into events using Vec<Event> directly with RecursionGuard
    let source = Source::new(toml);
    let tokens: Vec<_> = source.lex().collect();
    let mut events: Vec<Event> = Vec::new();
    let mut guarded = RecursionGuard::new(&mut events, max_recursion_depth);
    let mut error_collector = ParseErrorCollector::new();
    parse_document(&tokens, &mut guarded, &mut error_collector);

    // Check for parse errors
    if let Some(error_msg) = error_collector.take_error() {
        return Err(TomlDeError::new(
            toml,
            TomlDeErrorKind::GenericTomlError(error_msg),
            None,
            "$".to_string(),
        ));
    }

    // Allocate the type
    let mut partial = Partial::alloc::<T>().map_err(|e| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::GenericReflect(e),
            None,
            "$".to_string(),
        )
    })?;

    // Build schema for flatten disambiguation
    // Use flattened representation: the solver disambiguates by looking at the variant's
    // inner fields. For example, if Backend::Local has a "cache" field and Backend::Remote
    // has a "url" field, seeing "cache" in the TOML resolves to Local variant.
    let schema = Schema::build(partial.shape()).map_err(|e| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::GenericTomlError(format!("Schema build error: {e}")),
            None,
            "$".to_string(),
        )
    })?;

    trace!(
        "Built schema with {} resolutions",
        schema.resolutions().len()
    );

    // Determine the resolution to use for deferred mode
    let resolution = if schema.resolutions().len() == 1 {
        // Only one resolution - use it directly
        trace!("Single resolution, using directly");
        schema.resolutions()[0].clone()
    } else {
        // Multiple resolutions - use solver to disambiguate
        trace!(
            "Multiple resolutions ({}), using solver",
            schema.resolutions().len()
        );

        // Pre-scan events to collect top-level keys for the solver
        let mut solver = Solver::new(&schema);
        let mut resolved: Option<Resolution> = None;

        // Scan top-level keys from events
        let keys = collect_top_level_keys(toml, &events);
        trace!("Top-level keys: {keys:?}");

        for key in &keys {
            let result = solver.see_key(*key);
            trace!("Solver result for '{key}': {result:?}");

            match result {
                KeyResult::Solved(handle) => {
                    let res = handle.resolution();
                    trace!("Solved to resolution: {}", res.describe());
                    resolved = Some(res.clone());
                    break;
                }
                KeyResult::Unambiguous { .. } | KeyResult::Ambiguous { .. } => {
                    // Continue processing keys
                }
                KeyResult::Unknown => {
                    // For externally-tagged enums at the root level, the key might be
                    // the variant name. Check if any resolution describes a variant
                    // matching this key (e.g., key "A" matches resolution "Root::A").
                    for res in schema.resolutions() {
                        let desc = res.describe();
                        // Check if desc ends with "::key" (variant name match)
                        if desc.ends_with(&format!("::{key}")) {
                            trace!("Key '{key}' matches variant in resolution: {desc}");
                            resolved = Some(res.clone());
                            break;
                        }
                    }
                    if resolved.is_some() {
                        break;
                    }
                    // Otherwise, unknown key - continue (might be an error later)
                }
            }
        }

        // If not solved by keys, try finish()
        if resolved.is_none() {
            match solver.finish() {
                Ok(handle) => {
                    let res = handle.resolution();
                    trace!("Solver finished with resolution: {}", res.describe());
                    resolved = Some(res.clone());
                }
                Err(e) => {
                    return Err(TomlDeError::new(
                        toml,
                        TomlDeErrorKind::GenericTomlError(format!("Solver error: {e}")),
                        None,
                        "$".to_string(),
                    ));
                }
            }
        }

        resolved.unwrap_or_default()
    };

    // Store shape before moving the resolution
    let root_shape = partial.shape();

    // Always use deferred mode for TOML (keys can come in any order)
    let path = partial.path().to_owned();
    partial = partial
        .begin_deferred(resolution.clone())
        .map_err(|e| TomlDeError::new(toml, TomlDeErrorKind::GenericReflect(e), None, path))?;

    trace!("Starting streaming deserialization");

    // Create deserializer and process events
    let mut deser = StreamingDeserializer::new(toml, &events, root_shape, &resolution);
    let mut partial = deser.deserialize_document(partial)?;

    // Finish deferred mode - this fills defaults for unset fields and validates
    let path = partial.path().to_owned();
    partial = partial.finish_deferred().map_err(|e| {
        trace!("finish_deferred error: {e:?}");
        // Convert UninitializedField errors to ExpectedFieldWithName
        let kind = match &e {
            ReflectError::UninitializedField { field_name, .. } => {
                TomlDeErrorKind::ExpectedFieldWithName(field_name)
            }
            _ => TomlDeErrorKind::GenericReflect(e),
        };
        TomlDeError::new(toml, kind, None, path)
    })?;

    // Build the result
    let heap_value = partial.build().map_err(|e| {
        // Convert UninitializedField errors to ExpectedFieldWithName
        trace!("Build error: {e:?}");
        let kind = match &e {
            ReflectError::UninitializedField { field_name, .. } => {
                TomlDeErrorKind::ExpectedFieldWithName(field_name)
            }
            _ => TomlDeErrorKind::GenericReflect(e),
        };
        TomlDeError::new(toml, kind, None, "$".to_string())
    })?;

    trace!("Finished streaming deserialization");

    // Materialize the HeapValue into the concrete type T
    heap_value.materialize::<T>().map_err(|e| {
        TomlDeError::new(
            toml,
            TomlDeErrorKind::GenericReflect(e),
            None,
            "$".to_string(),
        )
    })
}

/// Collect top-level keys from TOML events for solver disambiguation.
///
/// This scans the events and extracts:
/// - Keys from top-level key-value pairs (e.g., `key = value`)
/// - First segments of table headers (e.g., `[foo]` -> "foo")
fn collect_top_level_keys<'input>(source: &'input str, events: &[Event]) -> Vec<&'input str> {
    let mut keys = Vec::new();
    let mut iter = EventIter::new(events);
    let mut depth: usize = 0; // Track nesting level (0 = top-level)

    while let Some(event) = iter.next() {
        match event.kind() {
            // Table headers reset depth and provide a key
            EventKind::StdTableOpen | EventKind::ArrayTableOpen => {
                depth = 0;
                // The next SimpleKey is part of the table path
                // For [foo.bar], the first key "foo" is a top-level key
                if let Some(next) = iter.peek()
                    && next.kind() == EventKind::SimpleKey
                {
                    let span = next.span();
                    let key_str = &source[span.start()..span.end()];
                    // Decode if it's a quoted string
                    let key = decode_simple_key(source, next);
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                    _ = key_str;
                }
            }
            EventKind::StdTableClose | EventKind::ArrayTableClose => {
                // After table header, keys are within that table (depth = 1 for path length 1)
                // This is simplified - actual depth depends on table path length
                depth = 1;
            }
            // Key-value pairs at depth 0 have top-level keys
            EventKind::SimpleKey if depth == 0 => {
                // Check if this is a key-value pair (followed by = or .) vs table header
                // Table header keys are handled above
                // Here we handle dotted keys like foo.bar = x
                let saved_pos = iter.position();

                // Look ahead to determine context
                let mut is_kv_key = false;
                let decoded_first = decode_simple_key(source, event);

                for next in iter.by_ref() {
                    match next.kind() {
                        EventKind::KeySep => {
                            // Dot separator - continue to next key segment
                        }
                        EventKind::SimpleKey => {
                            // Another key segment
                        }
                        EventKind::KeyValSep => {
                            // = found - this is a key-value pair
                            is_kv_key = true;
                            break;
                        }
                        _ => break,
                    }
                }

                iter.rewind(saved_pos);

                if is_kv_key && !keys.contains(&decoded_first) {
                    keys.push(decoded_first);
                }
            }
            // Track nesting
            EventKind::InlineTableOpen | EventKind::ArrayOpen => {
                depth += 1;
            }
            EventKind::InlineTableClose | EventKind::ArrayClose => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    keys
}

/// Decode a simple key from an event, handling quoted strings
fn decode_simple_key<'input>(source: &'input str, event: &Event) -> &'input str {
    let span = event.span();
    let raw_str = &source[span.start()..span.end()];
    // If it starts with a quote, it's a quoted string - for now just strip quotes
    // A more complete implementation would use toml_parser's decoder
    if raw_str.starts_with('"') || raw_str.starts_with('\'') {
        // Strip quotes
        &raw_str[1..raw_str.len() - 1]
    } else {
        raw_str
    }
}

/// Streaming deserializer state
struct StreamingDeserializer<'input, 'events, 'res> {
    /// The TOML source string
    source: &'input str,
    /// Event iterator (automatically filters whitespace/comments/newlines)
    iter: EventIter<'events>,
    /// Current key path being built (for dotted keys like foo.bar.baz)
    current_keys: Vec<Cow<'input, str>>,
    /// Number of nested frames we've opened (for cleanup)
    open_frames: usize,
    /// The root shape (for finding flattened fields)
    root_shape: &'static Shape,
    /// The resolution (for variant selections)
    resolution: &'res Resolution,
    /// Currently open flattened field (and variant if applicable)
    open_flatten: Option<OpenFlatten>,
    /// Tracks array table keys for DynamicValue types, mapping key -> frame count at array level.
    /// This allows subsequent `[[key]]` headers to add to the existing array instead of replacing.
    dynamic_array_tables: micromap::Map<String, usize, 16>,
    /// Tracks ALL active array table keys (both typed `Vec<T>` and DynamicValue),
    /// mapping key -> frame count at the list item level (one level deeper than dynamic_array_tables).
    /// This allows table headers like `[array.field]` to navigate into the current array item.
    active_array_tables: micromap::Map<String, usize, 16>,
    /// When true, we're skipping an unknown table section (ignoring all key-value pairs until next table header)
    skipping_unknown_table: bool,
}

/// Tracks which flattened field is currently open
struct OpenFlatten {
    /// The flattened field name
    field_name: &'static str,
    /// The selected variant name (if the flattened type is an enum)
    variant_name: Option<&'static str>,
}

impl<'input, 'events, 'res> StreamingDeserializer<'input, 'events, 'res> {
    fn new(
        source: &'input str,
        events: &'events [Event],
        root_shape: &'static Shape,
        resolution: &'res Resolution,
    ) -> Self {
        Self {
            source,
            iter: EventIter::new(events),
            current_keys: Vec::new(),
            open_frames: 0,
            root_shape,
            resolution,
            open_flatten: None,
            dynamic_array_tables: Default::default(),
            active_array_tables: Default::default(),
            skipping_unknown_table: false,
        }
    }

    // ========================================================================
    // Error helpers - reduce duplication
    // ========================================================================

    /// Convert a ReflectError to the appropriate TomlDeErrorKind
    #[allow(dead_code)]
    fn reflect_error_to_kind(e: ReflectError, _partial: &Partial<'_>) -> TomlDeErrorKind {
        match e {
            // For NoSuchField errors, check if the struct has a single field
            // and return ExpectedFieldWithName if so
            ReflectError::FieldError {
                shape,
                field_error: FieldError::NoSuchField,
            } => {
                // Check if this is a struct with a single field
                if let Type::User(UserType::Struct(st)) = shape.ty
                    && st.fields.len() == 1
                {
                    return TomlDeErrorKind::ExpectedFieldWithName(st.fields[0].name);
                }
                TomlDeErrorKind::GenericReflect(ReflectError::FieldError {
                    shape,
                    field_error: FieldError::NoSuchField,
                })
            }
            // When trying to navigate into a primitive as if it were a table
            ReflectError::OperationFailed { operation, .. }
                if operation.contains("cannot select a field from") =>
            {
                TomlDeErrorKind::ExpectedType {
                    expected: "value",
                    got: "table",
                }
            }
            // All other errors are passed through
            other => TomlDeErrorKind::GenericReflect(other),
        }
    }

    /// Create an error from a ReflectError
    #[allow(dead_code)]
    fn reflect_err<'facet>(
        &self,
        e: ReflectError,
        partial: &Partial<'facet>,
    ) -> TomlDeError<'input> {
        TomlDeError::new(
            self.source,
            Self::reflect_error_to_kind(e, partial),
            self.current_span(),
            partial.path(),
        )
    }

    /// Create an error from a ReflectError with a specific span
    #[allow(dead_code)]
    fn reflect_err_at<'facet>(
        &self,
        e: ReflectError,
        span: Range<usize>,
        partial: &Partial<'facet>,
    ) -> TomlDeError<'input> {
        TomlDeError::new(
            self.source,
            Self::reflect_error_to_kind(e, partial),
            Some(span),
            partial.path(),
        )
    }

    /// Create an error from a ReflectError with a specific span and pre-computed path
    /// (for use when partial has been moved by ownership-taking methods)
    fn reflect_err_at_path(
        &self,
        e: ReflectError,
        span: Range<usize>,
        path: String,
    ) -> TomlDeError<'input> {
        TomlDeError::new(
            self.source,
            Self::reflect_error_to_kind_simple(e),
            Some(span),
            path,
        )
    }

    /// Convert a ReflectError to the appropriate TomlDeErrorKind (without partial reference)
    fn reflect_error_to_kind_simple(e: ReflectError) -> TomlDeErrorKind {
        match e {
            ReflectError::FieldError {
                shape,
                field_error: FieldError::NoSuchField,
            } => {
                if let Type::User(UserType::Struct(st)) = shape.ty
                    && st.fields.len() == 1
                {
                    return TomlDeErrorKind::ExpectedFieldWithName(st.fields[0].name);
                }
                TomlDeErrorKind::GenericReflect(ReflectError::FieldError {
                    shape,
                    field_error: FieldError::NoSuchField,
                })
            }
            ReflectError::OperationFailed { operation, .. }
                if operation.contains("cannot select a field from") =>
            {
                TomlDeErrorKind::ExpectedType {
                    expected: "value",
                    got: "table",
                }
            }
            other => TomlDeErrorKind::GenericReflect(other),
        }
    }

    // ========================================================================
    // Partial helpers - reduce duplication
    // ========================================================================

    /// Check if the partial is at an enum that needs a variant to be selected.
    /// Returns false for untagged enums (they need type-based dispatch, not name-based).
    fn needs_variant_selection(partial: &Partial<'_>) -> bool {
        matches!(partial.shape().ty, Type::User(UserType::Enum(_)))
            && partial.selected_variant().is_none()
            && !partial.shape().is_untagged()
    }

    /// Check if the partial is at an untagged enum that needs variant selection.
    fn is_untagged_enum_needing_selection(partial: &Partial<'_>) -> bool {
        matches!(partial.shape().ty, Type::User(UserType::Enum(_)))
            && partial.selected_variant().is_none()
            && partial.shape().is_untagged()
    }

    /// Call partial.end() and convert any error
    fn end_frame<'facet>(
        &self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let path = partial.path().to_owned();
        partial.end().map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path,
            )
        })
    }

    /// Call partial.begin_field() and convert any error
    fn begin_field<'facet>(
        &self,
        partial: Partial<'facet>,
        key: &str,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let path = partial.path().to_owned();
        partial.begin_field(key).map_err(|e| {
            TomlDeError::new(
                self.source,
                Self::reflect_error_to_kind_simple(e),
                self.current_span(),
                path,
            )
        })
    }

    /// Call partial.select_variant_named() and convert any error
    fn select_variant<'facet>(
        &self,
        partial: Partial<'facet>,
        variant_name: &str,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let path = partial.path().to_owned();
        partial.select_variant_named(variant_name).map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path,
            )
        })
    }

    /// Call partial.begin_list_item() and convert any error
    fn begin_list_item<'facet>(
        &self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let path = partial.path().to_owned();
        partial.begin_list_item().map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path,
            )
        })
    }

    // ========================================================================
    // Map helpers
    // ========================================================================

    /// Call partial.begin_map() and convert any error
    fn begin_map<'facet>(
        &self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let path = partial.path().to_owned();
        partial.begin_map().map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path,
            )
        })
    }

    /// Call partial.begin_key() and convert any error
    fn begin_key<'facet>(
        &self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let path = partial.path().to_owned();
        partial.begin_key().map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path,
            )
        })
    }

    /// Call partial.begin_value() and convert any error
    fn begin_value<'facet>(
        &self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let path = partial.path().to_owned();
        partial.begin_value().map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path,
            )
        })
    }

    // ========================================================================
    // Spanned helpers
    // ========================================================================

    /// Deserialize into a `Spanned<T>` wrapper.
    /// This records the source span while deserializing the inner value.
    fn deserialize_spanned<'facet>(
        &mut self,
        partial: Partial<'facet>,
        value_span: Range<usize>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("deserialize_spanned: span={value_span:?}");

        let mut partial = partial;

        // Deserialize the inner value into the `value` field
        partial = self.begin_field(partial, "value")?;
        partial = self.deserialize_value(partial)?;
        partial = self.end_frame(partial)?;

        // Set the span field with offset and len
        partial = self.begin_field(partial, "span")?;
        let path = partial.path().to_owned();
        partial = partial.set_field("offset", value_span.start).map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path.clone(),
            )
        })?;
        partial = partial.set_field("len", value_span.len()).map_err(|e| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericReflect(e),
                self.current_span(),
                path,
            )
        })?;
        partial = self.end_frame(partial)?;

        Ok(partial)
    }

    // ========================================================================
    // Frame management with tracking
    // ========================================================================

    /// Push a field frame and track it
    fn push_field<'facet>(
        &mut self,
        partial: Partial<'facet>,
        key: &str,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let partial = self.begin_field(partial, key)?;
        self.open_frames += 1;
        Ok(partial)
    }

    /// Pop a tracked frame
    fn pop_frame<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        debug_assert!(self.open_frames > 0, "pop_frame called with no open frames");
        let partial = self.end_frame(partial)?;
        self.open_frames -= 1;
        Ok(partial)
    }

    /// Get the span for error reporting
    fn current_span(&self) -> Option<Range<usize>> {
        self.iter.peek().map(|e| {
            let span = e.span();
            span.start()..span.end()
        })
    }

    /// Create a Raw slice for decoding from an event
    fn raw_from_event(&self, event: &Event) -> Raw<'input> {
        let span = event.span();
        Raw::new_unchecked(
            &self.source[span.start()..span.end()],
            event.encoding(),
            span,
        )
    }

    /// Get a span as `Range<usize>` from an event
    fn span_range(event: &Event) -> Range<usize> {
        let span = event.span();
        span.start()..span.end()
    }

    /// Decode a key from an event
    fn decode_key(&self, event: &Event) -> Cow<'input, str> {
        let raw = self.raw_from_event(event);
        let mut output: Cow<'input, str> = Cow::Borrowed("");
        raw.decode_key(&mut output, &mut ());
        output
    }

    /// Skip a key-value pair by consuming events until we reach the value end
    fn skip_key_value_pair(&mut self) {
        let mut depth = 0;
        while let Some(event) = self.iter.next() {
            match event.kind() {
                EventKind::SimpleKey | EventKind::KeySep | EventKind::KeyValSep => {
                    // Continue consuming key parts and separators
                }
                EventKind::InlineTableOpen | EventKind::ArrayOpen => {
                    depth += 1;
                }
                EventKind::InlineTableClose | EventKind::ArrayClose => {
                    depth -= 1;
                    if depth == 0 {
                        // We've finished the value
                        break;
                    }
                }
                EventKind::Scalar => {
                    if depth == 0 {
                        // Simple scalar value - we're done
                        break;
                    }
                }
                EventKind::StdTableOpen | EventKind::ArrayTableOpen => {
                    // Hit next table header - don't consume it, let main loop handle it
                    // Need to rewind one event
                    self.iter.rewind(self.iter.position() - 1);
                    break;
                }
                _ => {}
            }
        }
    }

    /// Deserialize the entire document
    fn deserialize_document<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("deserialize_document");

        let mut partial = partial;

        // Process all events
        while let Some(event) = self.iter.peek() {
            match event.kind() {
                // Standard table header: [foo.bar]
                EventKind::StdTableOpen => {
                    self.iter.next(); // consume
                    // Clear skip mode when encountering a new table header
                    self.skipping_unknown_table = false;
                    partial = self.process_table_header(partial, false)?;
                }
                // Array table header: [[foo.bar]]
                EventKind::ArrayTableOpen => {
                    self.iter.next(); // consume
                    // Clear skip mode when encountering a new table header
                    self.skipping_unknown_table = false;
                    partial = self.process_table_header(partial, true)?;
                }
                // Key in a key-value pair
                EventKind::SimpleKey => {
                    if self.skipping_unknown_table {
                        // Skip this key-value pair
                        trace!("Skipping key-value pair in unknown table");
                        self.skip_key_value_pair();
                    } else {
                        partial = self.process_key_value(partial)?;
                    }
                }
                // Skip structural tokens at document level
                EventKind::StdTableClose | EventKind::ArrayTableClose => {
                    self.iter.next();
                }
                // Error events
                EventKind::Error => {
                    let event = self.iter.next().unwrap();
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError("Parse error".to_string()),
                        Some(Self::span_range(event)),
                        partial.path(),
                    ));
                }
                _ => {
                    // Skip other events at document level
                    self.iter.next();
                }
            }
        }

        // Close any open flattened field
        partial = self.close_open_flatten(partial)?;

        // Close any remaining open frames
        partial = self.close_all_frames(partial)?;

        Ok(partial)
    }

    /// Process a table header like [foo.bar] or [[foo.bar]]
    fn process_table_header<'facet>(
        &mut self,
        partial: Partial<'facet>,
        is_array_table: bool,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("process_table_header (array={is_array_table})");

        let mut partial = partial;

        // Close any open flattened field first
        partial = self.close_open_flatten(partial)?;

        // Collect the dotted key path first (before closing frames)
        let mut path: Vec<String> = Vec::new();
        while let Some(event) = self.iter.peek() {
            match event.kind() {
                EventKind::SimpleKey => {
                    let key = self.decode_key(event);
                    path.push(key.into_owned());
                    self.iter.next();
                }
                EventKind::KeySep => {
                    self.iter.next(); // skip the dot
                }
                EventKind::StdTableClose | EventKind::ArrayTableClose => {
                    self.iter.next();
                    break;
                }
                _ => break,
            }
        }

        trace!("Table path: {path:?}");

        // For DynamicValue array tables, check if this is a continuation
        // If so, we only close the list item frame and add a new item
        if is_array_table {
            // Use the full dotted path as the key (e.g., "pkg.rust.target.x86.components")
            let full_key = path.join(".");
            if let Some(&array_frame_count) = self.dynamic_array_tables.get(full_key.as_str()) {
                // This is a continuation of an existing array table
                // Close frames down to the array level (not including the array itself)
                while self.open_frames > array_frame_count {
                    partial = self.pop_frame(partial)?;
                }
                // Add a new list item
                partial = self.begin_list_item(partial)?;
                self.open_frames += 1;
                // Initialize the list item as an object
                partial = self.begin_map(partial)?;
                return Ok(partial);
            }
        }

        // Check if this table header navigates INTO an active array table item
        // E.g., [[items]] followed by [items.features] should navigate to the 'features' field
        // of the current items array element, not close everything and start fresh
        // This also applies to nested array tables like [[datasets]] followed by [[datasets.tests.queries]]
        if path.len() > 1 {
            let first_key = &path[0];
            if let Some(&item_frame_count) = self.active_array_tables.get(first_key.as_str()) {
                trace!(
                    "Navigating into active array table item: {first_key}, remaining path: {:?}, is_array_table: {}",
                    &path[1..],
                    is_array_table
                );
                // Close frames back to the list item level
                while self.open_frames > item_frame_count {
                    partial = self.pop_frame(partial)?;
                }
                // Navigate the remaining path (everything after the array table name)
                // For the last element, pass is_array_table to handle [[datasets.tests.queries]] properly
                for (i, key) in path.iter().enumerate().skip(1) {
                    let is_last = i == path.len() - 1;
                    let is_array_for_this_key = is_last && is_array_table;
                    partial = self.begin_field_or_list_item(
                        partial,
                        key,
                        is_array_for_this_key,
                        is_last,
                    )?;
                }
                return Ok(partial);
            }
        }

        // Close any previously open frames
        partial = self.close_all_frames(partial)?;

        // Navigate into the path
        for (i, key) in path.iter().enumerate() {
            let is_last = i == path.len() - 1;

            if is_last && is_array_table {
                // For [[array]] tables, we need to begin a list item
                partial = self.begin_field_or_list_item(partial, key, true, is_last)?;
            } else {
                partial = self.begin_field_or_list_item(partial, key, false, is_last)?;
            }
        }

        // For array tables with DynamicValue, store the full path for continuation detection
        // This needs to happen AFTER navigation, when we know the target is DynamicValue
        if is_array_table && matches!(partial.shape().def, Def::DynamicValue(_)) {
            let full_key = path.join(".");
            // Only store if not already stored (first occurrence)
            if !self.dynamic_array_tables.contains_key(full_key.as_str()) {
                self.dynamic_array_tables
                    .insert(full_key, self.open_frames - 1); // -1 because we're inside the list item
            }
        }

        Ok(partial)
    }

    /// Navigate into a key, handling Option unwrapping and enum variant selection.
    /// For dotted paths like value.sub where value is `Option<T>`, this unwraps into Some.
    /// For table paths like [foo.VariantName.bar], this properly selects the variant.
    /// Returns (partial, frame_pushed) where frame_pushed is true if a frame was pushed.
    fn navigate_into_key<'facet>(
        &mut self,
        partial: Partial<'facet>,
        key: &str,
    ) -> Result<(Partial<'facet>, bool), TomlDeError<'input>> {
        // Check for Option<T> - if we're navigating into it, unwrap into Some first
        // Option has Def::Option but may also be Type::User(UserType::Enum) due to NPO,
        // so we must check Def::Option BEFORE checking for enum variant selection
        if matches!(partial.shape().def, Def::Option(_)) {
            trace!("Unwrapping Option into Some, then navigating to: {key}");
            let path = partial.path().to_owned();
            let partial = partial.begin_some().map_err(|e| {
                TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericReflect(e),
                    self.current_span(),
                    path,
                )
            })?;
            self.open_frames += 1;
            // Now we're inside the Some variant, continue to navigate to the key
            return self.navigate_into_key(partial, key);
        }

        // Check for untagged enum - need to select struct variant first
        // When navigating with dotted keys like `edition.workspace = true`,
        // we're creating a nested structure, so select the struct-accepting variant
        if Self::is_untagged_enum_needing_selection(&partial) {
            trace!("Dotted key navigation into untagged enum - selecting struct variant");
            let shape = partial.shape();
            let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
                TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericTomlError(format!(
                        "Expected enum shape for untagged deserialization: {}",
                        shape.type_identifier
                    )),
                    self.current_span(),
                    partial.path(),
                )
            })?;

            if variants_by_format.struct_variants.is_empty() {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericTomlError(format!(
                        "No struct-accepting variants in untagged enum {} for dotted key navigation",
                        shape.type_identifier
                    )),
                    self.current_span(),
                    partial.path(),
                ));
            }

            // Try to find a variant that has this field name
            // For dotted keys like `edition.workspace`, we know we need a variant
            // with a "workspace" field (or wrapping a struct with that field)
            let mut selected_variant = None;
            for &variant in &variants_by_format.struct_variants {
                // Check if this is a newtype variant
                let is_newtype =
                    variant.data.fields.len() == 1 && variant.data.fields[0].name == "0";

                if is_newtype {
                    // For newtype variants, we need to check the inner struct's fields
                    // Get the inner type
                    let inner_field = &variant.data.fields[0];
                    let inner_shape = inner_field.shape();
                    // Check if the inner type has our key as a field
                    if let Type::User(UserType::Struct(struct_type)) = inner_shape.ty
                        && struct_type.fields.iter().any(|f| f.name == key)
                    {
                        selected_variant = Some(variant);
                        break;
                    }
                } else {
                    // For regular struct variants, check if they have this field directly
                    if variant.data.fields.iter().any(|f| f.name == key) {
                        selected_variant = Some(variant);
                        break;
                    }
                }
            }

            // If no variant matched the key, peek ahead at what fields will be set
            // This handles cases like [dependencies.foo] where "foo" is a HashMap key
            // and we need to look at the fields inside the table (like `path`, `version`, etc.)
            let variant = if let Some(v) = selected_variant {
                v
            } else {
                // Peek ahead to see what fields will be in this table
                let table_fields = self.peek_table_fields();
                if !table_fields.is_empty() {
                    self.select_best_matching_variant(&variants_by_format, &table_fields)
                        .unwrap_or(variants_by_format.struct_variants[0])
                } else {
                    variants_by_format.struct_variants[0]
                }
            };
            trace!(
                "Selected struct variant {} for untagged enum with dotted key '{}'",
                variant.name, key
            );
            let mut partial = self.select_variant(partial, variant.name)?;

            // Check if this is a newtype variant wrapping a struct
            let is_newtype = variant.data.fields.len() == 1 && variant.data.fields[0].name == "0";

            if is_newtype {
                // Enter field "0" to deserialize the inner struct, then navigate to the actual key
                partial = self.begin_field(partial, "0")?;
                self.open_frames += 1;
                // Now navigate to the actual key in the wrapped struct
                let (new_partial, pushed) = self.navigate_into_key(partial, key)?;
                return Ok((new_partial, pushed));
            } else {
                // Navigate directly to the key in the struct variant
                let (new_partial, pushed) = self.navigate_into_key(partial, key)?;
                return Ok((new_partial, pushed));
            }
        }

        // Check if this is variant selection or field navigation
        if Self::needs_variant_selection(&partial) {
            trace!("Selecting enum variant: {key}");
            let partial = self.select_variant(partial, key)?;
            Ok((partial, false)) // No frame pushed
        } else {
            trace!("Begin field: {key}");
            let partial = self.push_field(partial, key)?;
            Ok((partial, true)) // Frame pushed
        }
    }

    /// Begin a field, handling potential list items for array tables.
    /// Used for table header navigation like `[foo.bar]` or `[[items]]`.
    fn begin_field_or_list_item<'facet>(
        &mut self,
        partial: Partial<'facet>,
        key: &str,
        is_array_item: bool,
        is_last_in_path: bool,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!(
            "begin_field_or_list_item: {key} (array_item={is_array_item}, is_last={is_last_in_path})"
        );

        let mut partial = partial;

        // Check if we're at a map - need special handling
        if let Def::Map(map_def) = &partial.shape().def {
            // For maps, the key is a map entry key, not a struct field
            let value_shape = map_def.v();

            // Check if the value type can act as a table (have key-value pairs)
            // Only scalars definitively cannot be tables. For Def::Undefined types,
            // we allow them through since they might be structs - the actual navigation
            // will fail later if they're not compatible.
            // However, this check should only apply when this is the LAST element in the path.
            // For nested paths like [target.x86_64.dependencies], when processing "x86_64"
            // (middle element), we don't check at all since we'll navigate deeper.
            if is_last_in_path && matches!(value_shape.def, Def::Scalar) {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::ExpectedType {
                        expected: "value",
                        got: "table",
                    },
                    self.current_span(),
                    partial.path(),
                ));
            }

            // Initialize the map
            partial = self.begin_map(partial)?;

            // Start a map entry
            partial = self.begin_key(partial)?;
            let path = partial.path().to_owned();
            partial = partial.set(key.to_string()).map_err(|e| {
                TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericReflect(e),
                    self.current_span(),
                    path,
                )
            })?;
            partial = self.end_frame(partial)?;

            // Begin the value
            partial = self.begin_value(partial)?;
            self.open_frames += 1; // Track that we opened a value frame

            // Handle array item within map value
            if is_array_item {
                let path = partial.path().to_owned();
                partial = partial.begin_list().map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        self.current_span(),
                        path,
                    )
                })?;
                partial = self.begin_list_item(partial)?;
                self.open_frames += 1;
            }

            // After beginning the value, check if it's an untagged enum that needs variant selection
            // For table headers like [dependencies.backtrace] where the value is an untagged enum,
            // we need to select the struct-accepting variant since a table header implies struct content
            if is_last_in_path && Self::is_untagged_enum_needing_selection(&partial) {
                trace!(
                    "Table header points to untagged enum in HashMap - selecting struct variant"
                );
                let shape = partial.shape();
                let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!(
                            "Expected enum shape for untagged deserialization: {}",
                            shape.type_identifier
                        )),
                        self.current_span(),
                        partial.path(),
                    )
                })?;

                if variants_by_format.struct_variants.is_empty() {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!(
                            "No struct-accepting variants in untagged enum {} for table header",
                            shape.type_identifier
                        )),
                        self.current_span(),
                        partial.path(),
                    ));
                }

                let variant = variants_by_format.struct_variants[0];
                trace!(
                    "Selected struct variant {} for untagged enum in HashMap",
                    variant.name
                );
                partial = self.select_variant(partial, variant.name)?;

                // Check if this is a newtype variant wrapping a struct
                let is_newtype =
                    variant.data.fields.len() == 1 && variant.data.fields[0].name == "0";

                if is_newtype {
                    // Enter field "0" to deserialize the inner struct
                    partial = self.begin_field(partial, "0")?;
                    self.open_frames += 1;
                }
            }

            return Ok(partial);
        }

        // Check if we're at a DynamicValue - treat it like a map/object
        if matches!(partial.shape().def, Def::DynamicValue(_)) {
            // Initialize root as object
            partial = self.begin_map(partial)?;

            if is_array_item {
                // Array table [[key]] - create entry with array value
                // Note: continuations of existing array tables are handled in process_table_header
                let path = partial.path().to_owned();
                partial = partial.begin_object_entry(key).map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        self.current_span(),
                        path,
                    )
                })?;
                self.open_frames += 1;

                // Initialize as array
                let path = partial.path().to_owned();
                partial = partial.begin_list().map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        self.current_span(),
                        path,
                    )
                })?;

                // NOTE: Array table tracking is now done in process_table_header
                // where we have access to the full dotted path

                // Add the first list item
                partial = self.begin_list_item(partial)?;
                self.open_frames += 1;

                // Initialize the list item as an object
                partial = self.begin_map(partial)?;
            } else {
                // Regular table [key] - create object entry
                let path = partial.path().to_owned();
                partial = partial.begin_object_entry(key).map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        self.current_span(),
                        path,
                    )
                })?;
                self.open_frames += 1;

                // Initialize the entry's value as an object
                partial = self.begin_map(partial)?;
            }

            return Ok(partial);
        }

        // Check if the field exists before trying to navigate
        // This allows us to skip unknown table keys instead of failing
        // NOTE: This only applies to table headers, not dotted key-value pairs
        // TODO: Add support for #[facet(deny_unknown_fields)] attribute
        if !Self::needs_variant_selection(&partial) && partial.field_index(key).is_none() {
            // Skip mode: ignore this unknown table section
            trace!("Field '{key}' not found in table header - entering skip mode");
            self.skipping_unknown_table = true;
            return Ok(partial);
        }

        let (new_partial, _frame_pushed) = self.navigate_into_key(partial, key)?;
        partial = new_partial;

        // After navigating into the field, if we ended up at an Option, unwrap it
        // This handles cases like Option<Vec<T>> for array tables [[field]]
        if matches!(partial.shape().def, Def::Option(_)) {
            trace!("Unwrapping Option after field navigation");
            let path = partial.path().to_owned();
            partial = partial.begin_some().map_err(|e| {
                TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericReflect(e),
                    self.current_span(),
                    path,
                )
            })?;
            self.open_frames += 1;
        }

        // After navigating into the field, check if we're at an untagged enum
        // For table headers like [dep] where dep is an untagged enum, we need to
        // select the struct-accepting variant since a table header implies struct content
        if is_last_in_path && Self::is_untagged_enum_needing_selection(&partial) {
            trace!("Table header points to untagged enum - selecting struct variant");
            let shape = partial.shape();
            let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
                TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericTomlError(format!(
                        "Expected enum shape for untagged deserialization: {}",
                        shape.type_identifier
                    )),
                    self.current_span(),
                    partial.path(),
                )
            })?;

            if variants_by_format.struct_variants.is_empty() {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericTomlError(format!(
                        "No struct-accepting variants in untagged enum {} for table header",
                        shape.type_identifier
                    )),
                    self.current_span(),
                    partial.path(),
                ));
            }

            let variant = variants_by_format.struct_variants[0];
            trace!("Selected struct variant {} for table header", variant.name);
            partial = self.select_variant(partial, variant.name)?;

            // Check if this is a newtype variant wrapping a struct
            let is_newtype = variant.data.fields.len() == 1 && variant.data.fields[0].name == "0";

            if is_newtype {
                // Enter field "0" to deserialize the inner struct
                partial = self.begin_field(partial, "0")?;
                self.open_frames += 1;
            }
        }

        // After navigating into the field, check if it's a scalar type
        // Table headers can't point to scalar types (but structs and other compound types are OK)
        if is_last_in_path && !is_array_item {
            // Only check for Def::Scalar, not Def::Undefined which could be newtype structs
            let target_is_scalar = matches!(partial.shape().def, Def::Scalar);
            if target_is_scalar {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::ExpectedType {
                        expected: "value",
                        got: "table",
                    },
                    self.current_span(),
                    partial.path(),
                ));
            }
        }

        if is_array_item {
            // For array tables [[items]], we need to begin the list first
            // begin_list() is idempotent - returns Ok if already initialized
            let path = partial.path().to_owned();
            partial = partial.begin_list().map_err(|e| {
                TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericReflect(e),
                    self.current_span(),
                    path,
                )
            })?;
            trace!("Ensured list is initialized");

            // Now add the list item
            partial = self.begin_list_item(partial)?;
            self.open_frames += 1;

            // Track this array table so we can recognize [key.field] patterns later
            // Store the frame count at the list item level
            if is_last_in_path {
                self.active_array_tables
                    .insert(key.to_string(), self.open_frames);
                trace!(
                    "Tracking array table: {key} at frame level {}",
                    self.open_frames
                );
            }
        } else if matches!(partial.shape().def, Def::Map(_)) {
            // For maps, initialize the map so subsequent key-value pairs are treated as entries
            trace!("Initializing map");
            partial = self.begin_map(partial)?;
        }

        Ok(partial)
    }

    /// Close all tracked open frames
    fn close_all_frames<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let mut partial = partial;
        while self.open_frames > 0 {
            partial = self.pop_frame(partial)?;
        }
        Ok(partial)
    }

    /// Process a key-value pair like `foo.bar = value`
    fn process_key_value<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        // Collect the dotted key path
        self.current_keys.clear();
        while let Some(event) = self.iter.peek() {
            match event.kind() {
                EventKind::SimpleKey => {
                    let event = self.iter.next().unwrap();
                    let key = self.decode_key(event);
                    self.current_keys.push(key);
                }
                EventKind::KeySep => {
                    self.iter.next(); // skip the dot
                }
                EventKind::KeyValSep => {
                    self.iter.next(); // skip the =
                    break;
                }
                _ => break,
            }
        }

        trace!("Key path: {:?}", self.current_keys);

        let keys_len = self.current_keys.len();
        if keys_len == 0 {
            return Ok(partial);
        }

        let mut partial = partial;

        // For the first key, check if it's a flattened field at the document root
        let first_key = &self.current_keys[0];

        // Check if this key belongs to a flattened field
        if let Some(location) =
            find_field_location(self.root_shape, first_key.as_ref(), self.resolution)
        {
            match location {
                FieldLocation::Direct { field_name } => {
                    // Simple case: navigate directly
                    trace!("Direct field: {field_name}");
                    partial =
                        self.navigate_and_deserialize_direct(partial, &self.current_keys.clone())?;
                }
                FieldLocation::FlattenedStruct {
                    flatten_field_name,
                    inner_field_name,
                } => {
                    trace!("Flattened struct field: {flatten_field_name}.{inner_field_name}");
                    partial = self.navigate_flattened_struct(
                        partial,
                        flatten_field_name,
                        inner_field_name,
                    )?;
                }
                FieldLocation::FlattenedEnum {
                    flatten_field_name,
                    variant,
                    inner_field_name,
                } => {
                    trace!(
                        "Flattened enum field: {}.{}.{}",
                        flatten_field_name, variant.variant_name, inner_field_name
                    );
                    partial = self.navigate_flattened_enum(
                        partial,
                        flatten_field_name,
                        variant.variant_name,
                        inner_field_name,
                    )?;
                }
            }
        } else {
            // Fallback: try direct navigation (may fail for unknown fields)
            partial = self.navigate_and_deserialize_direct(partial, &self.current_keys.clone())?;
        }

        Ok(partial)
    }

    /// Navigate directly to a field path and deserialize.
    ///
    /// For a path like ["foo", "bar", "baz"] = value:
    /// 1. Navigate into "foo" and "bar" (intermediate keys)
    /// 2. Navigate into "baz" (final key)
    /// 3. Deserialize the value
    /// 4. Close "baz"
    /// 5. Close "bar" and "foo"
    ///
    /// Special case for maps: if during navigation we land on a map,
    /// the remaining keys are treated as map entries.
    ///
    /// Important: This manages frames locally and doesn't affect self.open_frames
    /// because all frames opened here are also closed here.
    fn navigate_and_deserialize_direct<'facet>(
        &mut self,
        partial: Partial<'facet>,
        keys: &[Cow<'input, str>],
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        if keys.is_empty() {
            return Ok(partial);
        }

        let mut partial = partial;

        // Track frames we open locally (so we can close them)
        let mut local_frames: Vec<bool> = Vec::with_capacity(keys.len());

        // Navigate into keys, watching for maps
        for (i, key) in keys.iter().enumerate() {
            // Check if we're at a map - if so, treat remaining keys as map entries
            if matches!(partial.shape().def, Def::Map(_)) {
                // Ensure map is initialized
                partial = self.begin_map(partial)?;

                // The current key is the map key, remaining keys (if any) would be
                // nested paths into the value - for TOML, we typically just have one key
                // for simple maps like HashMap<String, i32>
                let map_key = key.as_ref();
                let remaining_keys = &keys[i + 1..];

                // Insert map entry: begin_key, set key, end, begin_value, deserialize, end
                partial = self.begin_key(partial)?;
                let path = partial.path().to_owned();
                partial = partial.set(map_key.to_string()).map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        self.current_span(),
                        path,
                    )
                })?;
                partial = self.end_frame(partial)?;

                partial = self.begin_value(partial)?;
                if remaining_keys.is_empty() {
                    // No more keys, deserialize the value directly
                    partial = self.deserialize_value(partial)?;
                } else {
                    // More keys - navigate into the value (recursively)
                    partial = self.navigate_and_deserialize_direct(partial, remaining_keys)?;
                }
                partial = self.end_frame(partial)?;

                // Close all frames we opened during navigation
                for pushed in local_frames.into_iter().rev() {
                    if pushed {
                        partial = self.pop_frame(partial)?;
                    }
                }
                return Ok(partial);
            }

            // Check if we're at a DynamicValue - treat it like a map/object
            if matches!(partial.shape().def, Def::DynamicValue(_)) {
                // Initialize as object
                partial = self.begin_map(partial)?;

                let entry_key = key.as_ref();
                let remaining_keys = &keys[i + 1..];

                // Start object entry with this key
                let path = partial.path().to_owned();
                partial = partial.begin_object_entry(entry_key).map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        self.current_span(),
                        path,
                    )
                })?;

                if remaining_keys.is_empty() {
                    // No more keys, deserialize the value directly
                    partial = self.deserialize_dynamic_value(partial)?;
                } else {
                    // More keys - navigate into the value (recursively)
                    partial = self.navigate_and_deserialize_direct(partial, remaining_keys)?;
                }
                partial = self.end_frame(partial)?;

                // Close all frames we opened during navigation
                for pushed in local_frames.into_iter().rev() {
                    if pushed {
                        partial = self.pop_frame(partial)?;
                    }
                }
                return Ok(partial);
            }

            let (new_partial, pushed) = self.navigate_into_key(partial, key.as_ref())?;
            partial = new_partial;
            local_frames.push(pushed);
        }

        // Deserialize the value at the final location
        partial = self.deserialize_value(partial)?;

        // Close all frames we opened, in reverse order
        for pushed in local_frames.into_iter().rev() {
            if pushed {
                partial = self.pop_frame(partial)?;
            }
        }

        Ok(partial)
    }

    /// Navigate to a field inside a flattened struct and deserialize
    fn navigate_flattened_struct<'facet>(
        &mut self,
        partial: Partial<'facet>,
        flatten_field_name: &'static str,
        inner_field_name: &'static str,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let mut partial = partial;

        // Check if we need to open/switch flattened field
        let need_open = match &self.open_flatten {
            None => true,
            Some(open) => open.field_name != flatten_field_name,
        };

        if need_open {
            // Close current flattened field if open
            if self.open_flatten.is_some() {
                partial = self.end_frame(partial)?;
            }

            // Open the new flattened field
            partial = self.begin_field(partial, flatten_field_name)?;

            self.open_flatten = Some(OpenFlatten {
                field_name: flatten_field_name,
                variant_name: None,
            });
        }

        // Navigate to the inner field, deserialize, then close
        partial = self.begin_field(partial, inner_field_name)?;
        partial = self.deserialize_value(partial)?;
        partial = self.end_frame(partial)?;

        Ok(partial)
    }

    /// Navigate to a field inside a flattened enum variant and deserialize
    fn navigate_flattened_enum<'facet>(
        &mut self,
        partial: Partial<'facet>,
        flatten_field_name: &'static str,
        variant_name: &'static str,
        inner_field_name: &'static str,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let mut partial = partial;

        // Check if we need to open/switch flattened field or variant
        let need_open = match &self.open_flatten {
            None => true,
            Some(open) => {
                open.field_name != flatten_field_name || open.variant_name != Some(variant_name)
            }
        };

        if need_open {
            // Close current flattened field if open (including inner variant)
            if let Some(open) = &self.open_flatten {
                if open.variant_name.is_some() {
                    // Close the variant's inner type (the "0" field for newtype)
                    partial = self.end_frame(partial)?;
                }
                // Close the flattened field
                partial = self.end_frame(partial)?;
            }

            // Open the flattened field
            partial = self.begin_field(partial, flatten_field_name)?;

            // Select the variant
            partial = self.select_variant(partial, variant_name)?;

            // Open the variant's inner type (field "0" for newtype variants)
            partial = self.begin_field(partial, "0")?;

            self.open_flatten = Some(OpenFlatten {
                field_name: flatten_field_name,
                variant_name: Some(variant_name),
            });
        }

        // Navigate to the inner field, deserialize, then close
        partial = self.begin_field(partial, inner_field_name)?;
        partial = self.deserialize_value(partial)?;
        partial = self.end_frame(partial)?;

        Ok(partial)
    }

    /// Close any open flattened field
    fn close_open_flatten<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let mut partial = partial;
        if let Some(open) = self.open_flatten.take() {
            if open.variant_name.is_some() {
                // Close the variant's inner type
                partial = self.end_frame(partial)?;
            }
            // Close the flattened field
            partial = self.end_frame(partial)?;
        }
        Ok(partial)
    }

    /// Deserialize a value (scalar, array, or inline table)
    fn deserialize_value<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let Some(event) = self.iter.peek() else {
            return Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError("Unexpected end of input".to_string()),
                None,
                partial.path(),
            ));
        };

        // Handle Spanned<T> - deserialize the inner value and set the span
        if is_spanned_shape(partial.shape()) {
            return self.deserialize_spanned(partial, Self::span_range(event));
        }

        let mut partial = partial;

        // Handle Option<T> - unwrap into Some and deserialize the inner value
        if matches!(partial.shape().def, Def::Option(_)) {
            let span_range = Self::span_range(event);
            let path = partial.path();
            partial = match partial.begin_some() {
                Ok(p) => p,
                Err(e) => return Err(self.reflect_err_at_path(e, span_range, path)),
            };
            // Recursively deserialize the inner value
            partial = self.deserialize_value(partial)?;
            partial = self.end_frame(partial)?;
            return Ok(partial);
        }

        // Handle DynamicValue types (like facet_value::Value) - can hold any TOML value
        if matches!(partial.shape().def, Def::DynamicValue(_)) {
            return self.deserialize_dynamic_value(partial);
        }

        // Handle untagged enums - dispatch based on value type, not variant name
        if Self::is_untagged_enum_needing_selection(&partial) {
            return self.deserialize_untagged_enum(partial);
        }

        // Handle tuple structs (newtype wrappers) - unwrap into field "0"
        // Check if we have a TupleStruct with a single field and a scalar value
        if let Type::User(UserType::Struct(StructType { kind, fields, .. })) = partial.shape().ty
            && matches!(kind, StructKind::TupleStruct)
            && fields.len() == 1
            && matches!(event.kind(), EventKind::Scalar)
        {
            // Unwrap into the single field and deserialize recursively
            // (the inner type might also be a tuple struct, e.g. NestedUnit(Unit(i32)))
            trace!("Unwrapping tuple struct into field 0");
            partial = self.begin_field(partial, "0")?;
            partial = self.deserialize_value(partial)?;
            partial = self.end_frame(partial)?;
            return Ok(partial);
        }

        // Handle enum variants with data after variant selection
        // After select_variant_named, we're still at the enum level but have a selected variant
        if let Some(variant) = partial.selected_variant() {
            // Check if this is a tuple variant (fields named with digits like "0", "1", etc.)
            let is_tuple_variant = variant
                .data
                .fields
                .first()
                .map(|f| f.name.starts_with(|c: char| c.is_ascii_digit()))
                .unwrap_or(false);

            if is_tuple_variant && variant.data.fields.len() == 1 {
                // Single-element tuple variant - enter field "0" and deserialize the value
                trace!(
                    "Entering tuple variant field 0 for variant: {}",
                    variant.name
                );
                partial = self.begin_field(partial, "0")?;
                partial = self.deserialize_value(partial)?;
                partial = self.end_frame(partial)?;
                return Ok(partial);
            }
            // For struct variants or multi-element tuple variants, continue to normal handling
            // (they would be deserialized via inline table or table handlers)
        }

        // Check if the target type is a scalar - if so, we need special handling
        let is_scalar_target = matches!(partial.shape().def, Def::Scalar);

        // Check if target type is a map - maps can't be deserialized from scalars
        let is_map_target = matches!(partial.shape().def, Def::Map(_));

        // Check if target type is a list - lists can't be deserialized from scalars
        let is_list_target = matches!(partial.shape().def, Def::List(_));

        match event.kind() {
            EventKind::Scalar => {
                // Maps require table-like structure, not a scalar
                if is_map_target {
                    // Decode the scalar to determine its type for the error message
                    let raw = self.raw_from_event(event);
                    let mut decoded: Cow<'input, str> = Cow::Borrowed("");
                    let scalar_kind = raw.decode_scalar(&mut decoded, &mut ());
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::ExpectedType {
                            expected: "table like structure",
                            got: match scalar_kind {
                                toml_parser::decoder::ScalarKind::Boolean(_) => "boolean",
                                toml_parser::decoder::ScalarKind::Integer(_) => "integer",
                                toml_parser::decoder::ScalarKind::Float => "float",
                                toml_parser::decoder::ScalarKind::String => "string",
                                toml_parser::decoder::ScalarKind::DateTime => "datetime",
                            },
                        },
                        Some(Self::span_range(event)),
                        partial.path(),
                    ));
                }
                // Lists require array structure, not a scalar
                if is_list_target {
                    // Decode the scalar to determine its type for the error message
                    let raw = self.raw_from_event(event);
                    let mut decoded: Cow<'input, str> = Cow::Borrowed("");
                    let scalar_kind = raw.decode_scalar(&mut decoded, &mut ());
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::ExpectedType {
                            expected: "array",
                            got: match scalar_kind {
                                toml_parser::decoder::ScalarKind::Boolean(_) => "boolean",
                                toml_parser::decoder::ScalarKind::Integer(_) => "integer",
                                toml_parser::decoder::ScalarKind::Float => "float",
                                toml_parser::decoder::ScalarKind::String => "string",
                                toml_parser::decoder::ScalarKind::DateTime => "datetime",
                            },
                        },
                        Some(Self::span_range(event)),
                        partial.path(),
                    ));
                }
                let event = self.iter.next().unwrap();
                partial = self.deserialize_scalar(partial, event)?;
            }
            EventKind::ArrayOpen => {
                // If expecting a scalar but got an array, return type error
                if is_scalar_target {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::ExpectedType {
                            expected: self.expected_type_name(partial.shape()),
                            got: "array",
                        },
                        Some(Self::span_range(event)),
                        partial.path(),
                    ));
                }
                self.iter.next(); // consume [
                partial = self.deserialize_array(partial)?;
            }
            EventKind::InlineTableOpen => {
                // If expecting a scalar but got an inline table, return type error
                if is_scalar_target {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::ExpectedType {
                            expected: self.expected_type_name(partial.shape()),
                            got: "inline table",
                        },
                        Some(Self::span_range(event)),
                        partial.path(),
                    ));
                }
                self.iter.next(); // consume {
                partial = self.deserialize_inline_table(partial)?;
            }
            _ => {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericTomlError(format!(
                        "Expected value, got {:?}",
                        event.kind()
                    )),
                    Some(Self::span_range(event)),
                    partial.path(),
                ));
            }
        }

        Ok(partial)
    }

    /// Get the expected type name for error messages
    fn expected_type_name(&self, shape: &'static Shape) -> &'static str {
        if let Some(scalar_type) = ScalarType::try_from_shape(shape) {
            match scalar_type {
                ScalarType::Bool => "boolean",
                ScalarType::String | ScalarType::CowStr | ScalarType::Char => "string",
                ScalarType::F32 | ScalarType::F64 => "number",
                ScalarType::I8
                | ScalarType::I16
                | ScalarType::I32
                | ScalarType::I64
                | ScalarType::I128
                | ScalarType::ISize
                | ScalarType::U8
                | ScalarType::U16
                | ScalarType::U32
                | ScalarType::U64
                | ScalarType::U128
                | ScalarType::USize => "number",
                _ => "value",
            }
        } else {
            "value"
        }
    }

    /// Deserialize a scalar value
    fn deserialize_scalar<'facet>(
        &mut self,
        partial: Partial<'facet>,
        event: &Event,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let raw = self.raw_from_event(event);
        let event_span = Self::span_range(event);
        let mut decoded: Cow<'input, str> = Cow::Borrowed("");
        let kind = raw.decode_scalar(&mut decoded, &mut ());

        trace!(
            "Scalar: kind={:?}, raw='{}', decoded='{}'",
            kind,
            &self.source[event_span.clone()],
            decoded
        );

        let mut partial = partial;

        // Match the scalar type to what the Partial expects
        let Some(scalar_type) = ScalarType::try_from_shape(partial.shape()) else {
            // Try from_str for other types (like enums, UUIDs, etc)
            if partial.shape().is_from_str() {
                // Capture path/shape before potential move
                let path = partial.path();
                let shape = partial.shape();
                // Determine the TOML type name based on kind
                let toml_type_name = match kind {
                    ScalarKind::Boolean(_) => "boolean",
                    ScalarKind::String => "string",
                    ScalarKind::Integer(_) => "integer",
                    ScalarKind::Float => "float",
                    ScalarKind::DateTime => "datetime",
                };
                partial = match partial.parse_from_str(&decoded) {
                    Ok(p) => p,
                    Err(_e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::FailedTypeConversion {
                                toml_type_name,
                                rust_type: shape,
                                reason: None,
                            },
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
                return Ok(partial);
            }

            // For enums with a string value, try to select the variant by name
            if let (ScalarKind::String, Type::User(UserType::Enum(_))) = (kind, &partial.shape().ty)
            {
                // Capture path before ownership-taking call
                let path = partial.path();
                // Select the variant by name from the string value
                let partial = match partial.select_variant_named(&decoded) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
                return Ok(partial);
            }

            // Check if this is a struct with multiple fields
            if let Type::User(UserType::Struct(st)) = &partial.shape().ty
                && st.fields.len() > 1
            {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::ParseSingleValueAsMultipleFieldStruct,
                    Some(event_span.clone()),
                    partial.path(),
                ));
            }

            return Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::UnrecognizedScalar(partial.shape()),
                Some(event_span.clone()),
                partial.path(),
            ));
        };

        // Capture path once before the match for error handling
        let path = partial.path();

        match (kind, scalar_type) {
            // Strings
            (ScalarKind::String, ScalarType::String) => {
                partial = match partial.set(decoded.into_owned()) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
            }
            (ScalarKind::String, ScalarType::CowStr) => {
                // Create an owned Cow<'static, str>
                let cow: Cow<'static, str> = Cow::Owned(decoded.into_owned());
                partial = match partial.set(cow) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
            }

            // Booleans
            (ScalarKind::Boolean(b), ScalarType::Bool) => {
                partial = match partial.set(b) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
            }

            // Integers
            (ScalarKind::Integer(radix), scalar_type) => {
                partial = self.deserialize_integer(
                    partial,
                    &decoded,
                    radix.value(),
                    scalar_type,
                    event_span.clone(),
                )?;
            }

            // Floats
            (ScalarKind::Float, ScalarType::F32) => {
                let Ok(v) = decoded.parse::<f32>() else {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!("Invalid f32: {decoded}")),
                        Some(event_span.clone()),
                        path.clone(),
                    ));
                };
                partial = match partial.set(v) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
            }
            (ScalarKind::Float, ScalarType::F64) => {
                let Ok(v) = decoded.parse::<f64>() else {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!("Invalid f64: {decoded}")),
                        Some(event_span.clone()),
                        path.clone(),
                    ));
                };
                partial = match partial.set(v) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
            }

            // Float to integer type conversion attempt - this is a failed conversion, not a type mismatch
            (ScalarKind::Float, _) => {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::FailedTypeConversion {
                        toml_type_name: "float",
                        rust_type: partial.shape(),
                        reason: None,
                    },
                    Some(event_span.clone()),
                    path,
                ));
            }

            // Char from single-character string
            (ScalarKind::String, ScalarType::Char) => {
                let mut chars = decoded.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    // Exactly one character
                    partial = match partial.set(c) {
                        Ok(p) => p,
                        Err(e) => {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericReflect(e),
                                Some(event_span.clone()),
                                path,
                            ));
                        }
                    };
                } else {
                    // Zero or more than one character - type mismatch
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::ExpectedType {
                            expected: "char",
                            got: "string",
                        },
                        Some(event_span.clone()),
                        path,
                    ));
                }
            }

            // For string scalars with non-string target types that implement FromStr
            // (e.g., parsing "127.0.0.1" into IpAddr)
            (ScalarKind::String, _) if partial.shape().is_from_str() => {
                let shape = partial.shape();
                partial = match partial.parse_from_str(&decoded) {
                    Ok(p) => p,
                    Err(_e) => {
                        return Err(TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::FailedTypeConversion {
                                toml_type_name: "string",
                                rust_type: shape,
                                reason: None,
                            },
                            Some(event_span.clone()),
                            path,
                        ));
                    }
                };
            }

            _ => {
                // Determine expected type based on scalar_type
                let expected = match scalar_type {
                    ScalarType::Bool => "boolean",
                    ScalarType::String
                    | ScalarType::CowStr
                    | ScalarType::Char
                    | ScalarType::Str => "string",
                    ScalarType::F32
                    | ScalarType::F64
                    | ScalarType::I8
                    | ScalarType::I16
                    | ScalarType::I32
                    | ScalarType::I64
                    | ScalarType::I128
                    | ScalarType::ISize
                    | ScalarType::U8
                    | ScalarType::U16
                    | ScalarType::U32
                    | ScalarType::U64
                    | ScalarType::U128
                    | ScalarType::USize => "number",
                    // Network types (IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr) expect strings
                    // but are gated behind facet-core's "net" feature, so they fall through
                    // to the wildcard which returns "unknown" (still a valid string expectation)
                    _ => "unknown",
                };

                // Determine the actual TOML type
                let got = match kind {
                    ScalarKind::Boolean(_) => "boolean",
                    ScalarKind::String => "string",
                    ScalarKind::Integer(_) => "integer",
                    ScalarKind::Float => "float",
                    ScalarKind::DateTime => "datetime",
                };

                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::ExpectedType { expected, got },
                    Some(event_span.clone()),
                    path,
                ));
            }
        }

        Ok(partial)
    }

    /// Deserialize an integer with the given radix
    fn deserialize_integer<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
        decoded: &str,
        radix: u32,
        scalar_type: ScalarType,
        event_span: Range<usize>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        // Remove underscores from the number (TOML allows them as separators)
        let clean: String = decoded.chars().filter(|&c| c != '_').collect();

        macro_rules! parse_int {
            ($ty:ty) => {{
                let path = partial.path().to_owned();
                let Ok(v) = <$ty>::from_str_radix(&clean, radix) else {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!(
                            "Invalid {}: {}",
                            stringify!($ty),
                            decoded
                        )),
                        Some(event_span.clone()),
                        path,
                    ));
                };
                partial = partial.set(v).map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        Some(event_span.clone()),
                        path,
                    )
                })?;
            }};
        }

        match scalar_type {
            ScalarType::I8 => parse_int!(i8),
            ScalarType::I16 => parse_int!(i16),
            ScalarType::I32 => parse_int!(i32),
            ScalarType::I64 => parse_int!(i64),
            ScalarType::I128 => parse_int!(i128),
            ScalarType::ISize => parse_int!(isize),
            ScalarType::U8 => parse_int!(u8),
            ScalarType::U16 => parse_int!(u16),
            ScalarType::U32 => parse_int!(u32),
            ScalarType::U64 => parse_int!(u64),
            ScalarType::U128 => parse_int!(u128),
            ScalarType::USize => parse_int!(usize),
            // Also handle floats if the scalar was integer-like
            ScalarType::F32 => {
                let path = partial.path().to_owned();
                let Ok(v) = clean.parse::<f32>() else {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!("Invalid f32: {decoded}")),
                        Some(event_span.clone()),
                        path,
                    ));
                };
                partial = partial.set(v).map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        Some(event_span.clone()),
                        path,
                    )
                })?;
            }
            ScalarType::F64 => {
                let path = partial.path().to_owned();
                let Ok(v) = clean.parse::<f64>() else {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!("Invalid f64: {decoded}")),
                        Some(event_span.clone()),
                        path,
                    ));
                };
                partial = partial.set(v).map_err(|e| {
                    TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericReflect(e),
                        Some(event_span.clone()),
                        path,
                    )
                })?;
            }
            _ => {
                // Determine the expected type based on scalar_type
                let expected = match scalar_type {
                    ScalarType::Bool => "boolean",
                    ScalarType::String | ScalarType::CowStr | ScalarType::Char => "string",
                    ScalarType::F32 | ScalarType::F64 => "number",
                    _ => "unknown",
                };
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::ExpectedType {
                        expected,
                        got: "integer",
                    },
                    Some(event_span.clone()),
                    partial.path(),
                ));
            }
        }

        Ok(partial)
    }

    /// Deserialize any TOML value into a DynamicValue type (like facet_value::Value).
    ///
    /// This handles all TOML value types: booleans, integers, floats, strings, arrays, and tables.
    /// Datetime values are not currently supported and will return an error.
    fn deserialize_dynamic_value<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let Some(event) = self.iter.peek() else {
            return Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError("Unexpected end of input".to_string()),
                None,
                partial.path(),
            ));
        };

        let event_span = Self::span_range(event);

        // Capture path once before potentially ownership-transferring operations
        let path = partial.path();

        match event.kind() {
            EventKind::Scalar => {
                let event = self.iter.next().unwrap();
                let raw = self.raw_from_event(event);
                let mut decoded: Cow<'input, str> = Cow::Borrowed("");
                let kind = raw.decode_scalar(&mut decoded, &mut ());

                match kind {
                    ScalarKind::Boolean(b) => {
                        partial = match partial.set(b) {
                            Ok(p) => p,
                            Err(e) => {
                                return Err(TomlDeError::new(
                                    self.source,
                                    TomlDeErrorKind::GenericReflect(e),
                                    Some(event_span),
                                    path,
                                ));
                            }
                        };
                    }
                    ScalarKind::String => {
                        partial = match partial.set(decoded.into_owned()) {
                            Ok(p) => p,
                            Err(e) => {
                                return Err(TomlDeError::new(
                                    self.source,
                                    TomlDeErrorKind::GenericReflect(e),
                                    Some(event_span),
                                    path,
                                ));
                            }
                        };
                    }
                    ScalarKind::Integer(radix) => {
                        // Remove underscores and parse as i64
                        let clean: String = decoded.chars().filter(|&c| c != '_').collect();
                        let Ok(v) = i64::from_str_radix(&clean, radix.value()) else {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericTomlError(format!(
                                    "Invalid integer: {decoded}"
                                )),
                                Some(event_span),
                                path.clone(),
                            ));
                        };
                        partial = match partial.set(v) {
                            Ok(p) => p,
                            Err(e) => {
                                return Err(TomlDeError::new(
                                    self.source,
                                    TomlDeErrorKind::GenericReflect(e),
                                    Some(event_span),
                                    path,
                                ));
                            }
                        };
                    }
                    ScalarKind::Float => {
                        let Ok(v) = decoded.parse::<f64>() else {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericTomlError(format!(
                                    "Invalid float: {decoded}"
                                )),
                                Some(event_span),
                                path.clone(),
                            ));
                        };
                        partial = match partial.set(v) {
                            Ok(p) => p,
                            Err(e) => {
                                return Err(TomlDeError::new(
                                    self.source,
                                    TomlDeErrorKind::GenericReflect(e),
                                    Some(event_span),
                                    path,
                                ));
                            }
                        };
                    }
                    ScalarKind::DateTime => {
                        // Parse the TOML datetime string
                        let Some(parsed) = parse_toml_datetime(&decoded) else {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericTomlError(format!(
                                    "Invalid datetime: {decoded}"
                                )),
                                Some(event_span),
                                path.clone(),
                            ));
                        };

                        // Convert to DynDateTimeKind and call set_datetime
                        partial = match match parsed {
                            ParsedDateTime::Offset {
                                year,
                                month,
                                day,
                                hour,
                                minute,
                                second,
                                nanos,
                                offset_minutes,
                            } => partial.set_datetime(
                                year,
                                month,
                                day,
                                hour,
                                minute,
                                second,
                                nanos,
                                DynDateTimeKind::Offset { offset_minutes },
                            ),
                            ParsedDateTime::LocalDateTime {
                                year,
                                month,
                                day,
                                hour,
                                minute,
                                second,
                                nanos,
                            } => partial.set_datetime(
                                year,
                                month,
                                day,
                                hour,
                                minute,
                                second,
                                nanos,
                                DynDateTimeKind::LocalDateTime,
                            ),
                            ParsedDateTime::LocalDate { year, month, day } => partial.set_datetime(
                                year,
                                month,
                                day,
                                0,
                                0,
                                0,
                                0,
                                DynDateTimeKind::LocalDate,
                            ),
                            ParsedDateTime::LocalTime {
                                hour,
                                minute,
                                second,
                                nanos,
                            } => partial.set_datetime(
                                0,
                                0,
                                0,
                                hour,
                                minute,
                                second,
                                nanos,
                                DynDateTimeKind::LocalTime,
                            ),
                        } {
                            Ok(p) => p,
                            Err(e) => {
                                return Err(TomlDeError::new(
                                    self.source,
                                    TomlDeErrorKind::GenericReflect(e),
                                    Some(event_span),
                                    path,
                                ));
                            }
                        };
                    }
                }
            }
            EventKind::ArrayOpen => {
                self.iter.next(); // consume [
                // Reuse deserialize_dynamic_list - it already handles DynamicValue via begin_list/begin_list_item
                partial = self.deserialize_dynamic_list(partial)?;
            }
            EventKind::InlineTableOpen => {
                self.iter.next(); // consume {
                partial = self.deserialize_dynamic_inline_table(partial)?;
            }
            _ => {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericTomlError(format!(
                        "Expected value, got {:?}",
                        event.kind()
                    )),
                    Some(event_span),
                    path,
                ));
            }
        }

        Ok(partial)
    }

    /// Deserialize an inline table into a DynamicValue object.
    ///
    /// This is separate from `deserialize_inline_table` because that function does
    /// struct field matching, which doesn't apply to DynamicValue objects.
    fn deserialize_dynamic_inline_table<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("deserialize_dynamic_inline_table");

        // Initialize as object
        let path = partial.path();
        partial = match partial.begin_map() {
            Ok(p) => p,
            Err(e) => {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericReflect(e),
                    self.current_span(),
                    path,
                ));
            }
        };

        loop {
            let Some(event) = self.iter.peek() else { break };

            match event.kind() {
                EventKind::InlineTableClose => {
                    self.iter.next();
                    break;
                }
                EventKind::ValueSep => {
                    self.iter.next(); // skip comma
                }
                EventKind::SimpleKey => {
                    let event = self.iter.next().unwrap();
                    let raw = self.raw_from_event(event);

                    // Decode the key
                    let mut key: Cow<'input, str> = Cow::Borrowed("");
                    let _ = raw.decode_scalar(&mut key, &mut ());
                    let key_owned = key.into_owned();

                    // Consume the KeyValSep (=)
                    if let Some(sep_event) = self.iter.peek()
                        && matches!(sep_event.kind(), EventKind::KeyValSep)
                    {
                        self.iter.next();
                    }

                    // Start an object entry with this key
                    let path = partial.path();
                    partial = match partial.begin_object_entry(&key_owned) {
                        Ok(p) => p,
                        Err(e) => {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericReflect(e),
                                self.current_span(),
                                path,
                            ));
                        }
                    };

                    // Recursively deserialize the value
                    partial = self.deserialize_dynamic_value(partial)?;

                    // End the entry
                    let path = partial.path();
                    partial = match partial.end() {
                        Ok(p) => p,
                        Err(e) => {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericReflect(e),
                                self.current_span(),
                                path,
                            ));
                        }
                    };
                }
                _ => {
                    return Err(TomlDeError::new(
                        self.source,
                        TomlDeErrorKind::GenericTomlError(format!(
                            "Expected key or }}, got {:?}",
                            event.kind()
                        )),
                        self.current_span(),
                        partial.path(),
                    ));
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize an inline array
    fn deserialize_array<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("deserialize_array");

        // Check if we're deserializing into a fixed-size array or tuple vs a dynamic list
        let is_fixed_size = matches!(partial.shape().def, Def::Array(_));
        let is_tuple = matches!(partial.shape().ty, Type::User(UserType::Struct(s)) if s.kind == StructKind::Tuple);

        if is_fixed_size || is_tuple {
            // For fixed-size arrays and tuples, use begin_nth_field
            self.deserialize_fixed_array(partial)
        } else {
            // For dynamic lists (Vec, etc.), use begin_list/begin_list_item
            self.deserialize_dynamic_list(partial)
        }
    }

    /// Deserialize into a fixed-size array or tuple using indexed field access
    fn deserialize_fixed_array<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("deserialize_fixed_array");

        let mut index = 0;

        loop {
            let Some(event) = self.iter.peek() else { break };

            match event.kind() {
                EventKind::ArrayClose => {
                    self.iter.next();
                    break;
                }
                EventKind::ValueSep => {
                    self.iter.next(); // skip comma
                }
                _ => {
                    // Start the nth element
                    let path = partial.path();
                    partial = match partial.begin_nth_field(index) {
                        Ok(p) => p,
                        Err(e) => {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericReflect(e),
                                self.current_span(),
                                path,
                            ));
                        }
                    };

                    partial = self.deserialize_value(partial)?;

                    let path = partial.path();
                    partial = match partial.end() {
                        Ok(p) => p,
                        Err(e) => {
                            return Err(TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericReflect(e),
                                self.current_span(),
                                path,
                            ));
                        }
                    };

                    index += 1;
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize into a dynamic list (Vec, etc.) using begin_list/begin_list_item
    fn deserialize_dynamic_list<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("deserialize_dynamic_list");

        let path = partial.path();
        partial = match partial.begin_list() {
            Ok(p) => p,
            Err(e) => {
                return Err(TomlDeError::new(
                    self.source,
                    TomlDeErrorKind::GenericReflect(e),
                    self.current_span(),
                    path,
                ));
            }
        };

        loop {
            let Some(event) = self.iter.peek() else { break };

            match event.kind() {
                EventKind::ArrayClose => {
                    self.iter.next();
                    break;
                }
                EventKind::ValueSep => {
                    self.iter.next(); // skip comma
                }
                _ => {
                    // Start a new list item
                    let path = partial.path().to_owned();
                    partial = partial.begin_list_item().map_err(|e| {
                        TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            self.current_span(),
                            path,
                        )
                    })?;

                    partial = self.deserialize_value(partial)?;

                    let path = partial.path().to_owned();
                    partial = partial.end().map_err(|e| {
                        TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            self.current_span(),
                            path,
                        )
                    })?;
                }
            }
        }

        Ok(partial)
    }

    /// Deserialize an inline table
    fn deserialize_inline_table<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        trace!("deserialize_inline_table");

        // Check if this inline table represents an enum variant like { VariantName = value }
        let is_enum = Self::needs_variant_selection(&partial);

        // Check if this is a map - needs different handling
        let is_map = matches!(partial.shape().def, Def::Map(_));

        // For maps, initialize the map before processing entries
        if is_map {
            partial = self.begin_map(partial)?;
        }

        // Track which struct fields we've seen (for applying defaults later)
        let struct_fields = if let Type::User(UserType::Struct(st)) = partial.shape().ty {
            if !is_enum && !is_map {
                Some((st.fields, vec![false; st.fields.len()]))
            } else {
                None
            }
        } else {
            None
        };
        let mut fields_set = struct_fields;

        loop {
            let Some(event) = self.iter.peek() else { break };

            match event.kind() {
                EventKind::InlineTableClose => {
                    self.iter.next();
                    break;
                }
                EventKind::ValueSep => {
                    self.iter.next(); // skip comma
                }
                EventKind::SimpleKey => {
                    // Process key-value pair within inline table
                    let key_event = self.iter.next().unwrap();
                    let key = self.decode_key(key_event);

                    // Skip KeyValSep (=)
                    if let Some(e) = self.iter.peek()
                        && e.kind() == EventKind::KeyValSep
                    {
                        self.iter.next();
                    }

                    if is_map {
                        // For maps: { key = value, ... }
                        // Each key-value is a map entry
                        partial = self.begin_key(partial)?;
                        let path = partial.path().to_owned();
                        partial = partial.set(key.to_string()).map_err(|e| {
                            TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericReflect(e),
                                self.current_span(),
                                path,
                            )
                        })?;
                        partial = self.end_frame(partial)?;

                        partial = self.begin_value(partial)?;
                        partial = self.deserialize_value(partial)?;
                        partial = self.end_frame(partial)?;
                    } else if is_enum {
                        // For enums: { VariantName = value }
                        // The key is the variant name, value is the payload
                        partial = self.select_variant(partial, &key)?;

                        // Get variant info to determine how to handle the value
                        let variant = partial.selected_variant().ok_or_else(|| {
                            TomlDeError::new(
                                self.source,
                                TomlDeErrorKind::GenericTomlError(
                                    "Failed to get selected variant".to_string(),
                                ),
                                self.current_span(),
                                partial.path(),
                            )
                        })?;

                        // Handle based on variant kind
                        let num_fields = variant.data.fields.len();
                        if num_fields == 0 {
                            // Unit variant - skip the value (should be null or similar)
                            partial = self.deserialize_value(partial)?;
                        } else if num_fields == 1 {
                            // Tuple variant with one field - deserialize directly into field "0"
                            partial = self.begin_field(partial, "0")?;
                            partial = self.deserialize_value(partial)?;
                            partial = self.end_frame(partial)?;
                        } else {
                            // Multi-field tuple or struct variant - the value should be an array or table
                            // For simplicity, assume it's a single value for field "0"
                            partial = self.begin_field(partial, "0")?;
                            partial = self.deserialize_value(partial)?;
                            partial = self.end_frame(partial)?;
                        }
                    } else {
                        // Regular struct field - track which field we're setting
                        if let Some((fields, ref mut set)) = fields_set
                            && let Some(idx) = fields.iter().position(|f| f.name == key)
                        {
                            set[idx] = true;
                        }
                        partial = self.begin_field(partial, &key)?;
                        partial = self.deserialize_value(partial)?;
                        partial = self.end_frame(partial)?;
                    }
                }
                _ => {
                    // Skip unexpected events
                    self.iter.next();
                }
            }
        }

        // Apply defaults for missing struct fields
        if let Some((fields, set)) = fields_set {
            for (idx, field) in fields.iter().enumerate() {
                if set[idx] {
                    continue; // Field was already set
                }

                // Check if field has a default available
                let has_default_fn = field.has_default();
                let has_default_flag = field.has_default();
                let shape_has_default = field
                    .shape()
                    .type_ops
                    .map(|ops| ops.has_default_in_place())
                    .unwrap_or(false);
                let field_is_option = matches!(field.shape().def, Def::Option(_));

                if has_default_fn || has_default_flag || shape_has_default {
                    // Apply default for this field
                    let path = partial.path().to_owned();
                    partial = partial.set_nth_field_to_default(idx).map_err(|e| {
                        TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            self.current_span(),
                            path,
                        )
                    })?;
                } else if field_is_option {
                    // Missing Option<T> should become None even without explicit defaults
                    partial = self.begin_field(partial, field.name)?;
                    let path = partial.path().to_owned();
                    partial = partial.set_default().map_err(|e| {
                        TomlDeError::new(
                            self.source,
                            TomlDeErrorKind::GenericReflect(e),
                            self.current_span(),
                            path,
                        )
                    })?;
                    partial = self.end_frame(partial)?;
                }
                // Note: if no default is available and field is required, the build
                // will fail later with "field not initialized" error
            }
        }

        Ok(partial)
    }

    // ========================================================================
    // Untagged enum handling using facet-solver
    // ========================================================================

    /// Deserialize an untagged enum by examining the TOML value type.
    ///
    /// For untagged enums like:
    /// ```ignore
    /// #[facet(untagged)]
    /// enum Dependency {
    ///     Version(String),      // expects a scalar (string)
    ///     Table(DepTable),      // expects a table/mapping
    /// }
    /// ```
    ///
    /// We use `VariantsByFormat` from facet-solver to classify variants and
    /// match them against the incoming TOML value type.
    fn deserialize_untagged_enum<'facet>(
        &mut self,
        partial: Partial<'facet>,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let shape = partial.shape();
        trace!("deserialize_untagged_enum: {}", shape.type_identifier);

        // Use facet-solver to classify variants by expected format
        let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
            TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError(format!(
                    "Expected enum shape for untagged deserialization: {}",
                    shape.type_identifier
                )),
                self.current_span(),
                partial.path(),
            )
        })?;

        // Peek at the next event to determine value type
        let Some(event) = self.iter.peek() else {
            return Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError("Unexpected end of input".to_string()),
                self.current_span(),
                partial.path(),
            ));
        };

        match event.kind() {
            // Scalar value -> look for scalar-accepting variants (newtype wrapping String, i32, etc.)
            EventKind::Scalar => {
                self.deserialize_untagged_scalar_variant(partial, &variants_by_format)
            }

            // Inline table or table -> look for struct-accepting variants
            EventKind::InlineTableOpen => {
                self.deserialize_untagged_struct_variant(partial, &variants_by_format)
            }

            // Array -> look for tuple/sequence-accepting variants
            EventKind::ArrayOpen => {
                self.deserialize_untagged_tuple_variant(partial, &variants_by_format)
            }

            other => Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError(format!(
                    "Unexpected event {:?} for untagged enum {}",
                    other, shape.type_identifier
                )),
                self.current_span(),
                partial.path(),
            )),
        }
    }

    /// Deserialize an untagged enum from a scalar TOML value.
    ///
    /// Uses facet-solver's VariantsByFormat to find variants that accept scalars.
    fn deserialize_untagged_scalar_variant<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
        variants: &VariantsByFormat,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let shape = partial.shape();

        // First check unit variants - the scalar might be the variant name itself
        let event = self.iter.peek().unwrap();
        let raw = self.raw_from_event(event);
        let mut decoded: Cow<'input, str> = Cow::Borrowed("");
        let _kind = raw.decode_scalar(&mut decoded, &mut ());

        for variant in &variants.unit_variants {
            if variant.name == decoded.as_ref() {
                // This is a unit variant - select it
                self.iter.next(); // consume the scalar
                partial = self.select_variant(partial, variant.name)?;
                return Ok(partial);
            }
        }

        // Not a unit variant - look for scalar-accepting newtype variants
        if variants.scalar_variants.is_empty() {
            return Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError(format!(
                    "No scalar-accepting variants in untagged enum {} for value: {}",
                    shape.type_identifier, decoded
                )),
                self.current_span(),
                partial.path(),
            ));
        }

        // For now, pick the first scalar variant
        // TODO: Could add type-based disambiguation (e.g., if value is "123", prefer i32 over String)
        let (variant, _inner_shape) = &variants.scalar_variants[0];
        trace!("Selected scalar variant {} for untagged enum", variant.name);

        partial = self.select_variant(partial, variant.name)?;

        // Enter the variant's field "0" (newtype) and deserialize
        partial = self.begin_field(partial, "0")?;
        partial = self.deserialize_value(partial)?;
        partial = self.end_frame(partial)?;

        Ok(partial)
    }

    /// Deserialize an untagged enum from an inline table (struct variant).
    ///
    /// Uses facet-solver's VariantsByFormat to find variants that accept structs/mappings.
    fn deserialize_untagged_struct_variant<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
        variants: &VariantsByFormat,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let shape = partial.shape();

        if variants.struct_variants.is_empty() {
            return Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError(format!(
                    "No struct-accepting variants in untagged enum {}",
                    shape.type_identifier
                )),
                self.current_span(),
                partial.path(),
            ));
        }

        // Peek ahead to get the keys in the inline table
        let inline_table_keys = self.peek_inline_table_keys();

        // Try to find the best matching variant based on the keys present
        let variant = if !inline_table_keys.is_empty() {
            self.select_best_matching_variant(variants, &inline_table_keys)
                .unwrap_or(variants.struct_variants[0])
        } else {
            // No keys found or couldn't peek - use first variant as fallback
            variants.struct_variants[0]
        };

        trace!(
            "Selected struct variant {} for untagged enum based on keys {:?}",
            variant.name, inline_table_keys
        );

        partial = self.select_variant(partial, variant.name)?;

        // Check if this is a newtype variant wrapping a struct
        let is_newtype = variant.data.fields.len() == 1 && variant.data.fields[0].name == "0";

        if is_newtype {
            // Enter field "0" and deserialize the inner struct
            partial = self.begin_field(partial, "0")?;
            partial = self.deserialize_value(partial)?;
            partial = self.end_frame(partial)?;
        } else {
            // Struct variant with named fields - deserialize inline table directly
            partial = self.deserialize_value(partial)?;
        }

        Ok(partial)
    }

    /// Deserialize an untagged enum from an array (tuple variant).
    fn deserialize_untagged_tuple_variant<'facet>(
        &mut self,
        mut partial: Partial<'facet>,
        variants: &VariantsByFormat,
    ) -> Result<Partial<'facet>, TomlDeError<'input>> {
        let shape = partial.shape();

        if variants.tuple_variants.is_empty() {
            return Err(TomlDeError::new(
                self.source,
                TomlDeErrorKind::GenericTomlError(format!(
                    "No tuple-accepting variants in untagged enum {}",
                    shape.type_identifier
                )),
                self.current_span(),
                partial.path(),
            ));
        }

        // For now, pick the first tuple variant
        let (variant, _arity) = &variants.tuple_variants[0];
        trace!("Selected tuple variant {} for untagged enum", variant.name);

        partial = self.select_variant(partial, variant.name)?;
        partial = self.deserialize_value(partial)?;

        Ok(partial)
    }

    /// Peek ahead to get the fields that will be set in a table (for table headers)
    fn peek_table_fields(&self) -> Vec<&'input str> {
        let mut fields = Vec::new();
        let mut pos = self.iter.pos;
        let mut depth: usize = 0;

        // Scan through upcoming events to find SimpleKey events at depth 0
        // This works for table headers like [dependencies.foo] where we want to see
        // what fields (path, version, etc.) will be set
        while pos < self.iter.events.len() {
            let event = &self.iter.events[pos];

            match event.kind() {
                EventKind::SimpleKey if depth == 0 => {
                    let key = decode_simple_key(self.source, event);
                    fields.push(key);
                }
                EventKind::StdTableOpen | EventKind::ArrayTableOpen => {
                    // Hit a new table header - stop scanning
                    break;
                }
                EventKind::InlineTableOpen | EventKind::ArrayOpen => {
                    depth += 1;
                }
                EventKind::InlineTableClose | EventKind::ArrayClose => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }

            pos += 1;

            // Stop after collecting a reasonable number of fields
            if fields.len() >= 10 {
                break;
            }
        }

        fields
    }

    /// Peek ahead to get the keys in an inline table without consuming events
    fn peek_inline_table_keys(&self) -> Vec<&'input str> {
        let mut keys = Vec::new();
        let mut depth = 0;
        let mut pos = self.iter.pos;

        // We should currently be positioned before an InlineTableOpen
        // Skip to find the opening brace
        while pos < self.iter.events.len() {
            let event = &self.iter.events[pos];
            if matches!(
                event.kind(),
                EventKind::Whitespace | EventKind::Newline | EventKind::Comment
            ) {
                pos += 1;
                continue;
            }
            if event.kind() == EventKind::InlineTableOpen {
                depth = 1;
                pos += 1;
                break;
            }
            break;
        }

        // Scan through the inline table to collect keys
        while pos < self.iter.events.len() && depth > 0 {
            let event = &self.iter.events[pos];

            match event.kind() {
                EventKind::SimpleKey => {
                    // Extract the key string from the event span
                    let key = decode_simple_key(self.source, event);
                    keys.push(key);
                }
                EventKind::InlineTableOpen | EventKind::ArrayOpen => {
                    depth += 1;
                }
                EventKind::InlineTableClose | EventKind::ArrayClose => {
                    depth -= 1;
                }
                _ => {}
            }

            pos += 1;
        }

        keys
    }

    /// Select the best matching variant based on the keys present in the inline table
    fn select_best_matching_variant(
        &self,
        variants: &VariantsByFormat,
        inline_table_keys: &[&str],
    ) -> Option<&'static Variant> {
        let mut best_match = None;
        let mut best_match_score = 0;

        for &variant in &variants.struct_variants {
            // Check if this is a newtype variant
            let is_newtype = variant.data.fields.len() == 1 && variant.data.fields[0].name == "0";

            let variant_fields: Vec<&str> = if is_newtype {
                // For newtype variants, check the inner struct's fields
                let inner_field = &variant.data.fields[0];
                let inner_shape = inner_field.shape();
                if let Type::User(UserType::Struct(struct_type)) = inner_shape.ty {
                    struct_type.fields.iter().map(|f| f.name).collect()
                } else {
                    continue;
                }
            } else {
                // For regular struct variants, get fields directly
                variant.data.fields.iter().map(|f| f.name).collect()
            };

            // Count how many of the inline table keys match this variant's fields
            let mut match_count = 0;
            let mut has_unknown_fields = false;

            for &key in inline_table_keys {
                if variant_fields.contains(&key) {
                    match_count += 1;
                } else {
                    has_unknown_fields = true;
                }
            }

            // A variant is a good match if:
            // 1. It has all the keys from the inline table (no unknown fields)
            // 2. It has the most matching keys
            if !has_unknown_fields && match_count > best_match_score {
                best_match_score = match_count;
                best_match = Some(variant);
            }
        }

        best_match
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_parser::{Source, parser::parse_document};

    #[test]
    fn test_event_vec_basic() {
        let input = r#"
name = "test"
value = 42
"#;
        let source = Source::new(input);
        let tokens: Vec<_> = source.lex().collect();
        let mut events: Vec<Event> = Vec::new();

        parse_document(&tokens, &mut events, &mut ());

        // Should have events for keys and scalars (includes whitespace/newlines now)
        let structural_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    e.kind(),
                    EventKind::SimpleKey | EventKind::KeyValSep | EventKind::Scalar
                )
            })
            .collect();

        // name = "test" and value = 42
        // Each key-value pair: SimpleKey, KeyValSep, Scalar
        assert_eq!(structural_events.len(), 6);
    }

    #[test]
    fn test_event_vec_nested_keys() {
        let input = r#"
foo.bar.x = 1
foo.baz = 2
foo.bar.y = 3
"#;
        let source = Source::new(input);
        let tokens: Vec<_> = source.lex().collect();
        let mut events: Vec<Event> = Vec::new();

        parse_document(&tokens, &mut events, &mut ());

        // Count key separators (dots) - should reflect the dotted paths
        let key_seps: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind(), EventKind::KeySep))
            .collect();

        // foo.bar.x has 2 dots, foo.baz has 1 dot, foo.bar.y has 2 dots = 5 total
        assert_eq!(key_seps.len(), 5);
    }

    #[test]
    fn test_event_content_extraction() {
        let input = r#"name = "hello""#;
        let source = Source::new(input);
        let tokens: Vec<_> = source.lex().collect();
        let mut events: Vec<Event> = Vec::new();

        parse_document(&tokens, &mut events, &mut ());

        // Find the key event
        let key_event = events
            .iter()
            .find(|e| e.kind() == EventKind::SimpleKey)
            .unwrap();
        let span = key_event.span();
        assert_eq!(&input[span.start()..span.end()], "name");

        // Find the scalar event
        let scalar_event = events
            .iter()
            .find(|e| e.kind() == EventKind::Scalar)
            .unwrap();
        let span = scalar_event.span();
        assert_eq!(&input[span.start()..span.end()], "\"hello\"");
    }

    #[test]
    fn test_event_iter_peek_and_rewind() {
        use toml_parser::Span as TomlSpan;

        // Create events using Event::new_unchecked
        let events = vec![
            Event::new_unchecked(EventKind::SimpleKey, None, TomlSpan::new_unchecked(0, 4)),
            Event::new_unchecked(EventKind::KeyValSep, None, TomlSpan::new_unchecked(5, 6)),
            Event::new_unchecked(EventKind::Scalar, None, TomlSpan::new_unchecked(7, 13)),
        ];

        let mut iter = EventIter::new(&events);

        // Peek doesn't advance
        assert_eq!(iter.peek().unwrap().kind(), EventKind::SimpleKey);
        assert_eq!(iter.peek().unwrap().kind(), EventKind::SimpleKey);

        // Save position
        let pos = iter.position();

        // Consume
        assert_eq!(iter.next().unwrap().kind(), EventKind::SimpleKey);
        assert_eq!(iter.next().unwrap().kind(), EventKind::KeyValSep);

        // Rewind
        iter.rewind(pos);
        assert_eq!(iter.next().unwrap().kind(), EventKind::SimpleKey);
    }

    // Integration tests for the streaming deserializer
    #[test]
    fn test_streaming_simple_struct() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct Simple {
            name: String,
            value: i32,
        }

        let toml = r#"
name = "hello"
value = 42
"#;
        let result: Simple = from_str(toml).unwrap();
        assert_eq!(result.name, "hello");
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_streaming_nested_struct() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            x: i32,
            y: i32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Outer {
            inner: Inner,
            name: String,
        }

        let toml = r#"
name = "test"
[inner]
x = 10
y = 20
"#;
        let result: Outer = from_str(toml).unwrap();
        assert_eq!(result.name, "test");
        assert_eq!(result.inner.x, 10);
        assert_eq!(result.inner.y, 20);
    }

    #[test]
    fn test_streaming_dotted_keys() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct Bar {
            x: i32,
            y: i32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Foo {
            bar: Bar,
            baz: i32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Root {
            foo: Foo,
        }

        // Interleaved dotted keys - the key challenge for TOML
        let toml = r#"
foo.bar.x = 1
foo.baz = 2
foo.bar.y = 3
"#;
        let result: Root = from_str(toml).unwrap();
        assert_eq!(result.foo.bar.x, 1);
        assert_eq!(result.foo.baz, 2);
        assert_eq!(result.foo.bar.y, 3);
    }

    #[test]
    fn test_streaming_inline_array() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct WithArray {
            values: Vec<i32>,
        }

        let toml = r#"values = [1, 2, 3, 4, 5]"#;
        let result: WithArray = from_str(toml).unwrap();
        assert_eq!(result.values, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_streaming_inline_table() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct Point {
            x: i32,
            y: i32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct WithPoint {
            point: Point,
        }

        let toml = r#"point = { x = 10, y = 20 }"#;
        let result: WithPoint = from_str(toml).unwrap();
        assert_eq!(result.point.x, 10);
        assert_eq!(result.point.y, 20);
    }

    #[test]
    fn test_streaming_booleans() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct Flags {
            enabled: bool,
            debug: bool,
        }

        let toml = r#"
enabled = true
debug = false
"#;
        let result: Flags = from_str(toml).unwrap();
        assert!(result.enabled);
        assert!(!result.debug);
    }

    #[test]
    fn test_streaming_floats() {
        use facet::Facet;

        #[derive(Facet, Debug)]
        struct Floats {
            a: f32,
            b: f64,
        }

        let toml = r#"
a = 3.125
b = 2.5
"#;
        let result: Floats = from_str(toml).unwrap();
        assert!((result.a - 3.125).abs() < 0.001);
        assert!((result.b - 2.5).abs() < 0.000001);
    }

    // ========================================================================
    // Flatten with enum tests
    // ========================================================================

    #[test]
    fn test_flatten_enum_simple() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct LocalBackend {
            cache: bool,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct RemoteBackend {
            url: String,
        }

        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum Backend {
            Local(LocalBackend),
            Remote(RemoteBackend),
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Config {
            name: String,
            #[facet(flatten)]
            backend: Backend,
        }

        // Test Local variant - distinguished by "cache" key
        let toml_local = r#"
name = "local-config"
cache = true
"#;
        let result: Config = from_str(toml_local).unwrap();
        assert_eq!(result.name, "local-config");
        assert_eq!(result.backend, Backend::Local(LocalBackend { cache: true }));

        // Test Remote variant - distinguished by "url" key
        let toml_remote = r#"
name = "remote-config"
url = "http://example.com"
"#;
        let result: Config = from_str(toml_remote).unwrap();
        assert_eq!(result.name, "remote-config");
        assert_eq!(
            result.backend,
            Backend::Remote(RemoteBackend {
                url: "http://example.com".to_string()
            })
        );
    }

    #[test]
    fn test_flatten_enum_multiple_fields() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct TextPayload {
            content: String,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct BinaryPayload {
            data: String,
            encoding: String,
        }

        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum MessagePayload {
            Text(TextPayload),
            Binary(BinaryPayload),
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Message {
            id: String,
            #[facet(flatten)]
            payload: MessagePayload,
        }

        // Test Text variant
        let toml_text = r#"
id = "msg-001"
content = "Hello, world!"
"#;
        let result: Message = from_str(toml_text).unwrap();
        assert_eq!(result.id, "msg-001");
        assert_eq!(
            result.payload,
            MessagePayload::Text(TextPayload {
                content: "Hello, world!".to_string()
            })
        );

        // Test Binary variant
        let toml_binary = r#"
id = "msg-002"
data = "SGVsbG8="
encoding = "base64"
"#;
        let result: Message = from_str(toml_binary).unwrap();
        assert_eq!(result.id, "msg-002");
        assert_eq!(
            result.payload,
            MessagePayload::Binary(BinaryPayload {
                data: "SGVsbG8=".to_string(),
                encoding: "base64".to_string()
            })
        );
    }

    #[test]
    fn test_flatten_enum_with_table_header() {
        use facet::Facet;

        #[derive(Facet, Debug, PartialEq)]
        struct DatabaseConfig {
            host: String,
            port: i32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct FileConfig {
            path: String,
        }

        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum StorageKind {
            Database(DatabaseConfig),
            File(FileConfig),
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Storage {
            name: String,
            #[facet(flatten)]
            kind: StorageKind,
        }

        // Using table header style
        let toml_db = r#"
name = "primary"
host = "localhost"
port = 5432
"#;
        let result: Storage = from_str(toml_db).unwrap();
        assert_eq!(result.name, "primary");
        assert_eq!(
            result.kind,
            StorageKind::Database(DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432
            })
        );

        let toml_file = r#"
name = "backup"
path = "/var/data/backup.db"
"#;
        let result: Storage = from_str(toml_file).unwrap();
        assert_eq!(result.name, "backup");
        assert_eq!(
            result.kind,
            StorageKind::File(FileConfig {
                path: "/var/data/backup.db".to_string()
            })
        );
    }

    #[test]
    fn test_collect_top_level_keys() {
        // Test key collection for solver
        let input = r#"
name = "test"
cache = true
foo.bar = 1
"#;
        let source = Source::new(input);
        let tokens: Vec<_> = source.lex().collect();
        let mut events: Vec<Event> = Vec::new();
        parse_document(&tokens, &mut events, &mut ());

        let keys = collect_top_level_keys(input, &events);
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"cache"));
        assert!(keys.contains(&"foo")); // First segment of dotted key
    }

    #[test]
    fn test_collect_top_level_keys_with_table() {
        let input = r#"
name = "test"

[database]
host = "localhost"
"#;
        let source = Source::new(input);
        let tokens: Vec<_> = source.lex().collect();
        let mut events: Vec<Event> = Vec::new();
        parse_document(&tokens, &mut events, &mut ());

        let keys = collect_top_level_keys(input, &events);
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"database"));
    }
}

#[test]
fn test_multiple_dotted_workspace_fields() {
    use facet::Facet;

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct WorkspaceRef {
        pub workspace: bool,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    pub enum StringOrWorkspace {
        String(String),
        Workspace(WorkspaceRef),
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    pub struct Package {
        pub version: Option<StringOrWorkspace>,
        pub description: Option<StringOrWorkspace>,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    pub struct Manifest {
        pub package: Option<Package>,
    }

    // Single dotted workspace field works
    let toml1 = r#"
[package]
version.workspace = true
"#;
    let result: Result<Manifest, _> = from_str(toml1);
    assert!(result.is_ok(), "Single dotted workspace field should work");

    // Multiple dotted workspace fields should also work
    let toml2 = r#"
[package]
version.workspace = true
description.workspace = true
"#;
    let result: Result<Manifest, _> = from_str(toml2);
    assert!(
        result.is_ok(),
        "Multiple dotted workspace fields failed: {:?}",
        result.err()
    );

    let manifest = result.unwrap();
    assert_eq!(
        manifest.package.as_ref().unwrap().version,
        Some(StringOrWorkspace::Workspace(WorkspaceRef {
            workspace: true
        }))
    );
    assert_eq!(
        manifest.package.as_ref().unwrap().description,
        Some(StringOrWorkspace::Workspace(WorkspaceRef {
            workspace: true
        }))
    );
}
