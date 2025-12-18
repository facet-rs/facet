//! Cache for compiled deserializers.
//!
//! Compiled functions are cached by (ConstTypeId, ConstTypeId) to avoid
//! recompilation on every deserialization call.
//!
//! Tier-1 (shape JIT) and Tier-2 (format JIT) use separate caches.

use std::collections::HashMap;
use std::sync::OnceLock;

use facet_core::{ConstTypeId, Facet};
use parking_lot::RwLock;

use super::compiler::{self, CompiledDeserializer};
use super::format_compiler::{self, CompiledFormatDeserializer};
use super::helpers;
use crate::{FormatJitParser, FormatParser};

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
#[allow(dead_code)]
pub fn clear_cache() {
    if let Some(cache) = CACHE.get() {
        cache.write().clear();
    }
}

// =============================================================================
// Tier-2 Format JIT Cache
// =============================================================================

/// Tier-2 cache stores complete CompiledFormatDeserializer instances
/// because they own the JITModule (which owns the compiled code memory).
///
/// Unlike Tier-1 where we just cache the function pointer, Tier-2 needs
/// to keep the module alive.
static FORMAT_CACHE: OnceLock<RwLock<HashMap<CacheKey, CachedFormatDeserializer>>> =
    OnceLock::new();

/// Type-erased Tier-2 compiled deserializer stored in the cache.
struct CachedFormatDeserializer {
    /// Raw pointer to the compiled function.
    fn_ptr: *const u8,
}

// Safety: Same as Tier-1 - JIT-compiled code is thread-safe.
unsafe impl Send for CachedFormatDeserializer {}
unsafe impl Sync for CachedFormatDeserializer {}

fn format_cache() -> &'static RwLock<HashMap<CacheKey, CachedFormatDeserializer>> {
    FORMAT_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Get a Tier-2 compiled deserializer from cache, or compile and cache it.
///
/// Returns `None` if compilation fails (type not Tier-2 compatible).
pub fn get_or_compile_format<'de, T, P>(key: CacheKey) -> Option<CompiledFormatDeserializer<T, P>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Fast path: check read lock first
    {
        let cache = format_cache().read();
        if let Some(cached) = cache.get(&key) {
            // We need to recompile to get a fresh module that owns the code
            // This is a limitation - ideally we'd share the module, but
            // for now we just recompile on cache hit.
            // TODO: Consider using Arc<JITModule> or similar to share ownership
            drop(cache);
            return format_compiler::try_compile_format::<T, P>();
        }
    }

    // Slow path: compile and insert
    let compiled = format_compiler::try_compile_format::<T, P>()?;
    let fn_ptr = compiled.fn_ptr();

    {
        let mut cache = format_cache().write();
        // Double-check in case another thread compiled while we were compiling
        cache
            .entry(key)
            .or_insert(CachedFormatDeserializer { fn_ptr });
    }

    Some(compiled)
}

/// Clear the Tier-2 cache. Useful for testing.
#[cfg(test)]
#[allow(dead_code)]
pub fn clear_format_cache() {
    if let Some(cache) = FORMAT_CACHE.get() {
        cache.write().clear();
    }
}
