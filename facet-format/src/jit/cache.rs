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
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::OnceLock;

use facet_core::{ConstTypeId, Facet};
use museair::bfast::HashMap;
use parking_lot::RwLock;

use super::compiler::{self, CachedModule, CompiledDeserializer};
use super::format_compiler::{self, CompiledFormatDeserializer};
use super::helpers;
use crate::{FormatJitParser, FormatParser};

/// Cache key: (target type's ConstTypeId, parser's ConstTypeId)
type CacheKey = (ConstTypeId, ConstTypeId);

/// Global cache of compiled deserializers.
///
/// The value is an Arc to the cached module which owns the JITModule memory.
/// The Arc keeps the compiled code alive as long as there are references.
static CACHE: OnceLock<RwLock<HashMap<CacheKey, Arc<CachedModule>>>> = OnceLock::new();

fn cache() -> &'static RwLock<HashMap<CacheKey, Arc<CachedModule>>> {
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
            return Some(CompiledDeserializer::from_cached(
                Arc::clone(cached),
                vtable,
            ));
        }
    }

    // Slow path: compile and insert
    let result = compiler::try_compile_module::<T>()?;
    let cached = Arc::new(CachedModule::new(
        result.module,
        result.nested_modules,
        result.fn_ptr,
    ));

    {
        let mut cache = cache().write();
        // Double-check in case another thread compiled while we were compiling
        cache.entry(key).or_insert_with(|| Arc::clone(&cached));
    }

    // Create vtable for this parser type
    let vtable = helpers::make_vtable::<P>();
    Some(CompiledDeserializer::from_cached(cached, vtable))
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

use std::sync::atomic::{AtomicU64, Ordering};

use super::format_compiler::CachedFormatModule;

/// Reason why Tier-2 compilation was refused or failed.
#[derive(Debug, Clone, Copy)]
pub enum CachedFormatMiss {
    /// Shape check said no, or emitter returned None (unsupported shape).
    Unsupported,
    /// Code size budget exceeded (not yet implemented, reserved for Step 3).
    #[allow(dead_code)]
    TooLarge,
    /// Cranelift verifier/define_function failure (internal error, non-retriable).
    #[allow(dead_code)]
    CompileFailed,
}

/// Cache entry for Tier-2: either a compiled module (hit) or a known failure (miss).
#[derive(Clone)]
pub enum CachedFormatCacheEntry {
    /// Compilation succeeded: cached module ready to use.
    Hit(Arc<CachedFormatModule>),
    /// Compilation failed/refused: cache the failure to avoid recompiling.
    Miss(CachedFormatMiss),
}

/// Bounded cache structure for Tier-2 format JIT.
/// Tracks insertion order for FIFO eviction when capacity is exceeded.
struct BoundedFormatCache {
    /// Map of cache keys to entries (Hit or Miss)
    entries: HashMap<CacheKey, CachedFormatCacheEntry>,
    /// Insertion order queue for FIFO eviction
    insertion_order: VecDeque<CacheKey>,
    /// Maximum number of entries (from env var or default)
    max_entries: usize,
}

impl BoundedFormatCache {
    fn new() -> Self {
        let max_entries = std::env::var("FACET_TIER2_CACHE_MAX_ENTRIES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1024); // Default: 1024 entries

        Self {
            entries: HashMap::default(),
            insertion_order: VecDeque::new(),
            max_entries,
        }
    }

    fn get(&self, key: &CacheKey) -> Option<&CachedFormatCacheEntry> {
        self.entries.get(key)
    }

    fn insert(&mut self, key: CacheKey, value: CachedFormatCacheEntry) {
        // If key already exists, remove it from insertion_order (we'll re-add at end)
        if self.entries.contains_key(&key) {
            self.insertion_order.retain(|k| k != &key);
        }

        // Check if we need to evict
        while self.entries.len() >= self.max_entries && !self.insertion_order.is_empty() {
            if let Some(oldest_key) = self.insertion_order.pop_front() {
                self.entries.remove(&oldest_key);
                CACHE_EVICTIONS.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Insert new entry and track insertion order
        self.entries.insert(key, value);
        self.insertion_order.push_back(key);
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.insertion_order.clear();
    }
}

/// Tier-2 cache stores `CachedFormatCacheEntry` which can represent both
/// successful compilations and known failures (negative cache).
/// Bounded by FACET_TIER2_CACHE_MAX_ENTRIES (default: 1024).
static FORMAT_CACHE: OnceLock<RwLock<BoundedFormatCache>> = OnceLock::new();

fn format_cache() -> &'static RwLock<BoundedFormatCache> {
    FORMAT_CACHE.get_or_init(|| RwLock::new(BoundedFormatCache::new()))
}

// Observability: cache hit/miss/eviction counters
static CACHE_HIT: AtomicU64 = AtomicU64::new(0);
static CACHE_MISS_NEGATIVE: AtomicU64 = AtomicU64::new(0);
static CACHE_MISS_COMPILE: AtomicU64 = AtomicU64::new(0);
static CACHE_EVICTIONS: AtomicU64 = AtomicU64::new(0);

/// Get cache statistics (for testing/debugging).
/// Returns: (hits, negative_hits, compile_attempts, evictions)
#[allow(dead_code)]
pub fn get_cache_stats() -> (u64, u64, u64, u64) {
    (
        CACHE_HIT.load(Ordering::Relaxed),
        CACHE_MISS_NEGATIVE.load(Ordering::Relaxed),
        CACHE_MISS_COMPILE.load(Ordering::Relaxed),
        CACHE_EVICTIONS.load(Ordering::Relaxed),
    )
}

/// Reset cache statistics (for testing).
pub fn reset_cache_stats() {
    CACHE_HIT.store(0, Ordering::Relaxed);
    CACHE_MISS_NEGATIVE.store(0, Ordering::Relaxed);
    CACHE_MISS_COMPILE.store(0, Ordering::Relaxed);
    CACHE_EVICTIONS.store(0, Ordering::Relaxed);
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
/// Can cache both successful compilations (Hit) and known failures (Miss).
struct TlsCacheEntry {
    /// The cache key (type IDs for T and P)
    key: CacheKey,
    /// The cached entry (hit or miss)
    entry: CachedFormatCacheEntry,
}

thread_local! {
    /// Single-entry thread-local cache for Tier-2.
    /// This handles the common case of tight loops deserializing the same type.
    /// Caches both hits (compiled modules) and misses (known failures) to avoid
    /// repeated HashMap lookups and compilation attempts.
    static FORMAT_TLS_CACHE: RefCell<Option<TlsCacheEntry>> = const { RefCell::new(None) };
}

/// Get a Tier-2 compiled deserializer from cache, or compile and cache it.
///
/// Returns `None` if compilation fails (type not Tier-2 compatible).
/// Caches both successful compilations and failures (negative cache) to avoid
/// repeated compilation attempts on known-unsupported types.
///
/// This function uses a three-tier lookup strategy:
/// 1. **TLS single-entry cache**: O(1) key comparison (fastest, caches hits and misses)
/// 2. **Global cache read lock**: HashMap lookup with read lock (caches hits and misses)
/// 3. **Compile + cache**: JIT compile and store result (hit or miss) in both caches
pub fn get_or_compile_format<'de, T, P>(key: CacheKey) -> Option<CompiledFormatDeserializer<T, P>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Tier 1: Check thread-local single-entry cache (fastest path)
    // This avoids HashMap lookup + RwLock in tight loops
    let tls_result = FORMAT_TLS_CACHE.with(|cache| {
        let cache = cache.borrow();
        if let Some(entry) = cache.as_ref()
            && entry.key == key
        {
            // TLS hit! Check if it's a compiled module or a known failure
            match &entry.entry {
                CachedFormatCacheEntry::Hit(module) => {
                    CACHE_HIT.fetch_add(1, Ordering::Relaxed);
                    return Some(Some(CompiledFormatDeserializer::from_cached(Arc::clone(
                        module,
                    ))));
                }
                CachedFormatCacheEntry::Miss(_reason) => {
                    // Negative cache hit: compilation known to fail, return None immediately
                    CACHE_MISS_NEGATIVE.fetch_add(1, Ordering::Relaxed);
                    return Some(None);
                }
            }
        }
        None
    });

    // If TLS had an entry (hit or miss), return it
    if let Some(result) = tls_result {
        return result;
    }

    // Tier 2: Check global cache with read lock
    let global_result = {
        let cache = format_cache().read();
        cache.get(&key).cloned()
    };

    if let Some(cached_entry) = global_result {
        // Global cache hit: update TLS and return
        let entry = cached_entry.clone();
        FORMAT_TLS_CACHE.with(|tls| {
            *tls.borrow_mut() = Some(TlsCacheEntry {
                key,
                entry: cached_entry,
            });
        });

        match entry {
            CachedFormatCacheEntry::Hit(module) => {
                CACHE_HIT.fetch_add(1, Ordering::Relaxed);
                return Some(CompiledFormatDeserializer::from_cached(module));
            }
            CachedFormatCacheEntry::Miss(_reason) => {
                // Negative cache hit from global cache
                CACHE_MISS_NEGATIVE.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        }
    }

    // Tier 3: Compile and insert into both caches (cache hits AND misses)
    CACHE_MISS_COMPILE.fetch_add(1, Ordering::Relaxed);

    let cache_entry = match format_compiler::try_compile_format_module::<T, P>() {
        Some((module, fn_ptr)) => {
            // Compilation succeeded: create Hit entry
            let cached_module = Arc::new(CachedFormatModule::new(module, fn_ptr));
            CachedFormatCacheEntry::Hit(cached_module)
        }
        None => {
            // Compilation failed/unsupported: create Miss entry
            CachedFormatCacheEntry::Miss(CachedFormatMiss::Unsupported)
        }
    };

    // Insert into global cache (with eviction if at capacity)
    {
        let mut cache = format_cache().write();
        // Double-check in case another thread compiled while we were compiling
        if cache.get(&key).is_none() {
            cache.insert(key, cache_entry.clone());
        }
    }

    // Update TLS cache for future fast lookups
    FORMAT_TLS_CACHE.with(|tls| {
        *tls.borrow_mut() = Some(TlsCacheEntry {
            key,
            entry: cache_entry.clone(),
        });
    });

    // Return the result
    match cache_entry {
        CachedFormatCacheEntry::Hit(module) => {
            Some(CompiledFormatDeserializer::from_cached(module))
        }
        CachedFormatCacheEntry::Miss(_reason) => None,
    }
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
pub fn clear_format_cache() {
    if let Some(cache) = FORMAT_CACHE.get() {
        cache.write().clear();
    }
    // Also clear thread-local cache
    FORMAT_TLS_CACHE.with(|tls| {
        *tls.borrow_mut() = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry_clone() {
        // Verify CachedFormatCacheEntry is Clone (required for cache operations)
        let miss = CachedFormatCacheEntry::Miss(CachedFormatMiss::Unsupported);
        let _cloned = miss.clone();
    }

    // Note: Negative cache testing is verified via:
    // 1. Benchmarks showing no repeated compilation attempts
    // 2. Cache stats showing CACHE_MISS_NEGATIVE increments
    //
    // Manual verification command:
    //   FACET_JIT_TRACE=1 cargo bench <unsupported_workload>
    // Should show:
    //   - First attempt: CACHE_MISS_COMPILE=1
    //   - Subsequent attempts: CACHE_MISS_NEGATIVE increasing, CACHE_MISS_COMPILE unchanged
}
