//! Host-side dispatch of cell tracing records through the local tracing system.
//!
//! This module provides the machinery to convert `TracingRecord` from cells
//! into proper `tracing` events that flow through the host's subscriber.
//!
//! # Design
//!
//! The challenge is that `tracing` requires compile-time field names (as `&'static str`),
//! but cell records have dynamic fields. We solve this by:
//!
//! 1. **Interning field names** - Field names are interned (Box::leak) to get `'static` lifetime
//! 2. **Caching callsites per unique field set** - Each unique combination of (level, field_names)
//!    gets its own callsite, metadata, and field set
//! 3. **Using `Event::dispatch()`** - Events are dispatched through the proper tracing API
//!
//! This means dynamic fields like `user_id=42` become real tracing fields that subscribers
//! can filter on, format, and process individually.
//!
//! # Safety
//!
//! This module uses unsafe code for:
//! - String interning (Box::leak to get 'static str)
//! - Callsite creation (cyclic reference between callsite and metadata)
//! - Cache access (returning 'static references to cached callsites)
//!
//! All unsafe usage is sound because:
//! - Interned strings are never deallocated (intentional memory leak for 'static)
//! - Callsites are never removed from the cache
//! - The cache lives for the lifetime of the program

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tracing_core::callsite::Identifier;
use tracing_core::field::{Field, FieldSet, Value};
use tracing_core::metadata::Kind;
use tracing_core::{Event, Metadata};

use crate::{FieldValue, Level, TaggedRecord, TracingRecord};

// ============================================================================
// String Interning
// ============================================================================

/// Global string interner for field names.
/// Field names are interned to get `'static` lifetime required by tracing.
static STRING_INTERNER: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();

/// Intern a string to get a `'static` reference.
/// If the string is already interned, returns the existing reference.
fn intern_string(s: &str) -> &'static str {
    let interner = STRING_INTERNER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = interner.lock().unwrap();

    if let Some(&interned) = map.get(s) {
        return interned;
    }

    // SAFETY: We intentionally leak this string to get 'static lifetime.
    // This is bounded because there's a finite number of unique field names.
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    map.insert(s.to_string(), leaked);
    leaked
}

// ============================================================================
// Dynamic Callsite Cache
// ============================================================================

/// A cached callsite for a specific set of field names at a specific level.
struct CachedCallsite {
    /// Static metadata for this callsite
    metadata: &'static Metadata<'static>,
    /// Cached field references in the same order as field names
    fields: Vec<Field>,
}

/// Key for the callsite cache: (level, target, field names in order)
type CallsiteKey = (Level, &'static str, Vec<&'static str>);

/// Global cache of callsites, keyed by (level, target, field_names).
static CALLSITE_CACHE: OnceLock<Mutex<HashMap<CallsiteKey, CachedCallsite>>> = OnceLock::new();

/// Base field names that are always present in cell events.
/// Note: "target" is part of event metadata, not a field.
/// Note: "cell" is redundant - the target already identifies the source.
const BASE_FIELD_NAMES: &[&str] = &["message"];

/// Create or get a cached callsite for the given level, target, and dynamic field names.
fn get_or_create_callsite(
    level: Level,
    target: &str,
    dynamic_field_names: &[&str],
) -> &'static CachedCallsite {
    let cache = CALLSITE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    // Intern the target for use in metadata and cache key
    let target_static = intern_string(target);

    // Build the key: level + target + base fields + dynamic fields (all interned)
    let mut all_field_names: Vec<&'static str> =
        BASE_FIELD_NAMES.iter().map(|&s| intern_string(s)).collect();
    for name in dynamic_field_names {
        all_field_names.push(intern_string(name));
    }

    let key = (level, target_static, all_field_names.clone());

    let mut cache_guard = cache.lock().unwrap();

    // Check if we already have this callsite
    if let Some(cached) = cache_guard.get(&key) {
        // SAFETY: We never remove entries from the cache, so this pointer
        // remains valid for the lifetime of the program.
        let ptr = cached as *const CachedCallsite;
        drop(cache_guard);
        return unsafe { &*ptr };
    }

    // Create new callsite - this requires careful construction due to cyclic references

    // 1. Leak the field names slice
    let field_names_static: &'static [&'static str] =
        Box::leak(all_field_names.clone().into_boxed_slice());

    // 2. Convert level
    let tracing_level = match level {
        Level::Error => tracing_core::Level::ERROR,
        Level::Warn => tracing_core::Level::WARN,
        Level::Info => tracing_core::Level::INFO,
        Level::Debug => tracing_core::Level::DEBUG,
        Level::Trace => tracing_core::Level::TRACE,
    };

    // 3. Create a placeholder callsite using UnsafeCell for interior mutability
    // We need the callsite address before we can create the metadata,
    // but the callsite needs the metadata pointer. We solve this by:
    // - Creating a callsite with uninitialized metadata
    // - Creating the metadata with the callsite's address
    // - Writing the metadata pointer back to the callsite
    use std::cell::UnsafeCell;

    struct DynCallsiteCell {
        metadata: UnsafeCell<*const Metadata<'static>>,
    }

    // SAFETY: We only write to the UnsafeCell once (during initialization)
    // and only read from it after initialization is complete.
    unsafe impl Sync for DynCallsiteCell {}

    impl tracing_core::callsite::Callsite for DynCallsiteCell {
        fn set_interest(&self, _: tracing_core::subscriber::Interest) {}
        fn metadata(&self) -> &'static Metadata<'static> {
            // SAFETY: metadata is always set before this is called
            unsafe { &**self.metadata.get() }
        }
    }

    let callsite_box = Box::new(DynCallsiteCell {
        metadata: UnsafeCell::new(std::ptr::null()),
    });
    let callsite_ptr = Box::into_raw(callsite_box);
    let callsite_static: &'static DynCallsiteCell = unsafe { &*callsite_ptr };

    // 4. Create the FieldSet with the callsite's identifier
    let field_set = FieldSet::new(field_names_static, Identifier(callsite_static));

    // 5. Create and leak the metadata
    let metadata: &'static Metadata<'static> = Box::leak(Box::new(Metadata::new(
        "cell event",
        target_static,
        tracing_level,
        None, // file - not meaningful for remote events
        None, // line - not meaningful for remote events
        None, // module_path - not meaningful for remote events
        field_set,
        Kind::EVENT,
    )));

    // 6. Update the callsite with the real metadata
    // SAFETY: We have exclusive access to this callsite (just created it),
    // and we're setting a valid metadata pointer. The UnsafeCell allows
    // interior mutability.
    unsafe {
        *(*callsite_ptr).metadata.get() = metadata;
    }

    // 7. Register the callsite with tracing
    tracing_core::callsite::register(callsite_static);

    // 8. Get field references
    let fields: Vec<Field> = all_field_names
        .iter()
        .map(|name| metadata.fields().field(name).expect("field must exist"))
        .collect();

    // 9. Store in cache (we don't need to keep the callsite pointer - it's registered globally)
    let cached = CachedCallsite { metadata, fields };
    cache_guard.insert(key.clone(), cached);

    // Return reference to cached entry
    // SAFETY: Entry was just inserted and will never be removed.
    let ptr = cache_guard.get(&key).unwrap() as *const CachedCallsite;
    drop(cache_guard);
    unsafe { &*ptr }
}

// ============================================================================
// Public API
// ============================================================================

/// Dispatch a tagged tracing record through the host's tracing subscriber.
///
/// This converts the `TaggedRecord` into a proper `tracing::Event` and
/// dispatches it through the current subscriber, preserving:
/// - Log level (as the event level)
/// - Target (as the event's metadata target - enables filtering by cell)
/// - Message (as a `message` field)
/// - All dynamic fields with their original names and typed values
///
/// # Field Handling
///
/// Dynamic field names are interned (leaked) to satisfy tracing's `'static`
/// lifetime requirement. Callsites are cached per unique (level, target, field_names)
/// combination.
///
/// For example, a cell event like:
/// ```ignore
/// info!(user_id = 42, action = "login", "User logged in");
/// ```
///
/// Will be dispatched as a tracing event with:
/// - target = "my_cell::auth" (in metadata, for filtering)
/// - `message = "User logged in"`
/// - `user_id = 42` (as i64)
/// - `action = "login"` (as str)
///
/// These are real tracing fields that subscribers can filter and format.
pub fn dispatch_record(tagged: &TaggedRecord) {
    match &tagged.record {
        TracingRecord::Event {
            level,
            target,
            message,
            fields,
            ..
        } => {
            dispatch_event(*level, target, message.as_deref().unwrap_or(""), fields);
        }
        TracingRecord::SpanEnter {
            name,
            level,
            target,
            ..
        } => {
            if *level >= Level::Debug {
                let msg = format!("-> {}", name);
                dispatch_event(*level, target, &msg, &[]);
            }
        }
        TracingRecord::SpanExit { .. } | TracingRecord::SpanClose { .. } => {
            // Usually too verbose
        }
    }
}

/// Dispatch an event through the tracing system.
fn dispatch_event(level: Level, target: &str, message: &str, fields: &[(String, FieldValue)]) {
    // Get dynamic field names
    let dynamic_names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();

    // Get or create the callsite for this level + target + field combination
    let cached = get_or_create_callsite(level, target, &dynamic_names);

    // Build values for base fields + dynamic fields
    // We need to box dynamic values to keep them alive and get &dyn Value
    let boxed_dynamic: Vec<Box<dyn Value + '_>> = fields
        .iter()
        .map(|(_, v)| -> Box<dyn Value + '_> {
            match v {
                FieldValue::I64(x) => Box::new(*x),
                FieldValue::U64(x) => Box::new(*x),
                FieldValue::Bool(x) => Box::new(*x),
                FieldValue::Str(x) => Box::new(x.as_str()),
            }
        })
        .collect();

    // Build the values array with the exact number of fields
    let num_fields = 1 + fields.len(); // message + dynamic
    let mut values: Vec<(&Field, Option<&dyn Value>)> = Vec::with_capacity(num_fields);

    // Base field: message
    values.push((&cached.fields[0], Some(&message as &dyn Value)));

    // Dynamic fields
    for (i, boxed) in boxed_dynamic.iter().enumerate() {
        values.push((&cached.fields[1 + i], Some(boxed.as_ref())));
    }

    // Dispatch the event
    dispatch_with_value_count(cached.metadata, &cached.fields, &values);
}

/// Dispatch with the correct number of values.
/// Uses match on field count since value_set requires a fixed-size array.
fn dispatch_with_value_count(
    meta: &'static Metadata<'static>,
    _fields: &[Field],
    values: &[(&Field, Option<&dyn Value>)],
) {
    // The value_set method requires a fixed-size array type that implements ValidLen.
    // ValidLen is implemented for arrays up to size 32.
    // We handle up to 19 fields (3 base + 16 dynamic).

    match values.len() {
        3 => {
            let arr: [_; 3] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        4 => {
            let arr: [_; 4] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        5 => {
            let arr: [_; 5] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        6 => {
            let arr: [_; 6] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        7 => {
            let arr: [_; 7] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        8 => {
            let arr: [_; 8] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        9 => {
            let arr: [_; 9] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        10 => {
            let arr: [_; 10] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        11 => {
            let arr: [_; 11] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        12 => {
            let arr: [_; 12] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        13 => {
            let arr: [_; 13] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        14 => {
            let arr: [_; 14] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        15 => {
            let arr: [_; 15] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        16 => {
            let arr: [_; 16] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        17 => {
            let arr: [_; 17] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        18 => {
            let arr: [_; 18] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        19 => {
            let arr: [_; 19] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        n if n > 19 => {
            // Truncate to 19 fields (3 base + 16 dynamic)
            let arr: [_; 19] = std::array::from_fn(|i| values[i]);
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
        _ => {
            // Shouldn't happen (always have at least 3 base fields)
            // Pad with None values
            let arr: [(&Field, Option<&dyn Value>); 3] =
                std::array::from_fn(|i| values.get(i).cloned().unwrap_or((&_fields[0], None)));
            Event::dispatch(meta, &meta.fields().value_set(&arr));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_string() {
        let s1 = intern_string("hello");
        let s2 = intern_string("hello");
        let s3 = intern_string("world");

        // Same string should return same pointer
        assert!(std::ptr::eq(s1, s2));
        // Different strings should return different pointers
        assert!(!std::ptr::eq(s1, s3));
    }

    #[test]
    fn test_get_or_create_callsite() {
        let cs1 = get_or_create_callsite(Level::Info, "my_cell::auth", &["user_id", "action"]);
        let cs2 = get_or_create_callsite(Level::Info, "my_cell::auth", &["user_id", "action"]);
        let cs3 = get_or_create_callsite(Level::Info, "my_cell::auth", &["different_field"]);
        let cs4 = get_or_create_callsite(Level::Error, "my_cell::auth", &["user_id", "action"]);
        let cs5 = get_or_create_callsite(Level::Info, "other_cell", &["user_id", "action"]);

        // Same level + target + fields should return same callsite
        assert!(std::ptr::eq(cs1, cs2));
        // Different fields should return different callsite
        assert!(!std::ptr::eq(cs1, cs3));
        // Different level should return different callsite
        assert!(!std::ptr::eq(cs1, cs4));
        // Different target should return different callsite
        assert!(!std::ptr::eq(cs1, cs5));
    }

    #[test]
    fn test_callsite_has_correct_fields() {
        let cs = get_or_create_callsite(Level::Info, "test_target", &["count", "name"]);

        // Should have 3 fields: message, count, name
        assert_eq!(cs.fields.len(), 3);

        // Field names should match
        let field_names: Vec<&str> = cs.metadata.fields().iter().map(|f| f.name()).collect();
        assert_eq!(field_names, vec!["message", "count", "name"]);

        // Target should be in metadata, not fields
        assert_eq!(cs.metadata.target(), "test_target");
    }
}
