//! Cache for compiled deserializers.
//!
//! Compiled functions are cached by (ConstTypeId, ConstTypeId) to avoid
//! recompilation on every deserialization call.

use std::collections::HashMap;
use std::sync::OnceLock;

use facet_core::{ConstTypeId, Facet};
use parking_lot::RwLock;

use super::compiler::{self, CompiledDeserializer};
use super::helpers;
use crate::FormatParser;

/// Cache key: (target type's ConstTypeId, parser's ConstTypeId)
type CacheKey = (ConstTypeId, ConstTypeId);

/// Global cache of compiled deserializers.
///
/// The value is a type-erased pointer to the compiled function.
/// Safety: The function pointer is valid for the lifetime of the program
/// because JIT memory is never freed.
static CACHE: OnceLock<RwLock<HashMap<CacheKey, CachedDeserializer>>> = OnceLock::new();

/// Type-erased compiled deserializer stored in the cache.
struct CachedDeserializer {
    /// Raw pointer to the compiled function.
    /// The actual signature depends on the (T, P) types.
    fn_ptr: *const u8,
}

// Safety: The function pointer points to JIT-compiled code that is
// thread-safe (no mutable static state).
unsafe impl Send for CachedDeserializer {}
unsafe impl Sync for CachedDeserializer {}

fn cache() -> &'static RwLock<HashMap<CacheKey, CachedDeserializer>> {
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Get a compiled deserializer from cache, or compile and cache it.
///
/// Returns `None` if compilation fails (type not JIT-compatible).
pub fn get_or_compile<'de, T, P>(key: CacheKey) -> Option<CompiledDeserializer<T, P>>
where
    T: Facet<'de>,
    P: FormatParser<'de>,
{
    // Fast path: check read lock first
    {
        let cache = cache().read();
        if let Some(cached) = cache.get(&key) {
            // Create vtable for this parser type (same for all instances of P)
            let vtable = helpers::make_vtable::<P>();
            // Safety: We only store function pointers that match the (T, P) signature
            return Some(unsafe { CompiledDeserializer::from_ptr(cached.fn_ptr, vtable) });
        }
    }

    // Slow path: compile and insert
    let compiled = compiler::try_compile::<T, P>()?;
    let fn_ptr = compiled.as_ptr();

    {
        let mut cache = cache().write();
        // Double-check in case another thread compiled while we were compiling
        cache.entry(key).or_insert(CachedDeserializer { fn_ptr });
    }

    Some(compiled)
}

/// Clear the cache. Useful for testing.
#[cfg(test)]
pub fn clear_cache() {
    if let Some(cache) = CACHE.get() {
        cache.write().clear();
    }
}
