//! Cache for compiled deserializers.
//!
//! Compiled functions are cached by (ConstTypeId, ConstTypeId) to avoid
//! recompilation on every deserialization call.
//!
//! Tier-1 (shape JIT) and Tier-2 (format JIT) use separate caches.
//!
//! ## Performance Optimization: Thread-Local Cache
//!
//! For tight loops calling the same `(T, P)` instantiation repeatedly, we use
//! a thread-local single-entry cache to avoid the global HashMap lookup entirely.
//!
//! The key insight is that the address of a monomorphized function like
//! `fn mono_tag::<T, P>() {}` is unique per instantiation. This gives us an
//! O(1) discriminator that we can use as a cache key without hashing.

use std::cell::RefCell;
use std::sync::OnceLock;

use facet_core::{ConstTypeId, Facet};
use museair::bfast::HashMap;
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
    CACHE.get_or_init(|| RwLock::new(HashMap::default()))
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

use std::sync::Arc;

use super::format_compiler::CachedFormatModule;

/// Tier-2 cache stores Arc<CachedFormatModule> which owns the JITModule
/// (and thus the compiled code memory). Multiple deserializer handles
/// can share the same cached module.
static FORMAT_CACHE: OnceLock<RwLock<HashMap<CacheKey, Arc<CachedFormatModule>>>> = OnceLock::new();

fn format_cache() -> &'static RwLock<HashMap<CacheKey, Arc<CachedFormatModule>>> {
    FORMAT_CACHE.get_or_init(|| RwLock::new(HashMap::default()))
}

// =============================================================================
// Thread-Local Single-Entry Cache for Tier-2
// =============================================================================
//
// This optimization eliminates HashMap lookup overhead in tight loops that
// repeatedly deserialize the same type. We use CacheKey (ConstTypeId pair)
// directly since it's cheap to compare and immune to compiler optimizations.
//
// NOTE: We initially tried using function pointer addresses as keys, but
// LLVM's Identical Code Folding (ICF) can merge empty generic functions,
// causing all instantiations to share the same address. ConstTypeId is safe.

/// Thread-local cache entry for Tier-2 compiled deserializers.
struct TlsCacheEntry {
    /// The cache key (type IDs for T and P)
    key: CacheKey,
    /// The cached module (cheap Arc clone on hit)
    module: Arc<CachedFormatModule>,
}

thread_local! {
    /// Single-entry thread-local cache for Tier-2.
    /// This handles the common case of tight loops deserializing the same type.
    static FORMAT_TLS_CACHE: RefCell<Option<TlsCacheEntry>> = const { RefCell::new(None) };
}

/// Get a Tier-2 compiled deserializer from cache, or compile and cache it.
///
/// Returns `None` if compilation fails (type not Tier-2 compatible).
///
/// This function uses a three-tier lookup strategy:
/// 1. **TLS single-entry cache**: O(1) key comparison (fastest)
/// 2. **Global cache read lock**: HashMap lookup with read lock
/// 3. **Compile + cache**: JIT compile and store in both caches
pub fn get_or_compile_format<'de, T, P>(key: CacheKey) -> Option<CompiledFormatDeserializer<T, P>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Tier 1: Check thread-local single-entry cache (fastest path)
    // This avoids HashMap lookup + RwLock in tight loops
    let tls_hit = FORMAT_TLS_CACHE.with(|cache| {
        let cache = cache.borrow();
        if let Some(entry) = cache.as_ref() {
            if entry.key == key {
                // TLS hit! Return a handle sharing the cached module
                return Some(CompiledFormatDeserializer::from_cached(Arc::clone(
                    &entry.module,
                )));
            }
        }
        None
    });

    if let Some(deser) = tls_hit {
        return Some(deser);
    }

    // Tier 2: Check global cache with read lock
    {
        let cache = format_cache().read();
        if let Some(cached) = cache.get(&key) {
            // Global cache hit: update TLS and return
            let module = Arc::clone(cached);
            FORMAT_TLS_CACHE.with(|tls| {
                *tls.borrow_mut() = Some(TlsCacheEntry {
                    key,
                    module: Arc::clone(&module),
                });
            });
            return Some(CompiledFormatDeserializer::from_cached(module));
        }
    }

    // Tier 3: Compile and insert into both caches
    let (module, fn_ptr) = format_compiler::try_compile_format_module::<T, P>()?;
    let cached = Arc::new(CachedFormatModule::new(module, fn_ptr));

    {
        let mut cache = format_cache().write();
        // Double-check in case another thread compiled while we were compiling
        cache.entry(key).or_insert_with(|| Arc::clone(&cached));
    }

    // Update TLS cache for future fast lookups
    FORMAT_TLS_CACHE.with(|tls| {
        *tls.borrow_mut() = Some(TlsCacheEntry {
            key,
            module: Arc::clone(&cached),
        });
    });

    Some(CompiledFormatDeserializer::from_cached(cached))
}

/// Get a reusable Tier-2 compiled deserializer handle.
///
/// This is the recommended API for performance-critical hot loops. By obtaining
/// the handle once and reusing it, you bypass all cache lookups entirely.
///
/// # Example
///
/// ```ignore
/// use facet_format::jit;
///
/// // Get handle once (does cache lookup + possible compilation)
/// let deser = jit::get_format_deserializer::<Vec<u64>, MyParser>()
///     .expect("type not Tier-2 compatible");
///
/// // Hot loop: no cache lookup, just direct function call
/// for data in dataset {
///     let mut parser = MyParser::new(data);
///     let value: Vec<u64> = deser.deserialize(&mut parser)?;
/// }
/// ```
///
/// Returns `None` if the type is not Tier-2 compatible.
pub fn get_format_deserializer<'de, T, P>() -> Option<CompiledFormatDeserializer<T, P>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    let key = (T::SHAPE.id, ConstTypeId::of::<P>());
    get_or_compile_format::<T, P>(key)
}

/// Clear the Tier-2 cache. Useful for testing.
#[cfg(test)]
#[allow(dead_code)]
pub fn clear_format_cache() {
    if let Some(cache) = FORMAT_CACHE.get() {
        cache.write().clear();
    }
    // Also clear thread-local cache
    FORMAT_TLS_CACHE.with(|tls| {
        *tls.borrow_mut() = None;
    });
}
