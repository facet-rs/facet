//! Lock-free read, leak-on-insert cache used by every codec backend.
//!
//! [`LeakedCache`] is the shared shape behind both the Rust JIT's compiled
//! encoder/decoder cache and (eventually) the Swift codec FFI's compiled
//! handle cache. Both backends pay the same hot-path cost — one atomic load
//! plus one pointer copy on a hit, no locking — and lean on the same
//! museair-hashed `HashMap` swapped via `ArcSwap::rcu` on insert.
//!
//! Entries are leaked at insertion time so callers receive a `&'static V`
//! that lives for the process. Reference counting would only buy us atomic
//! clones on every hot-path lookup; we never evict from these caches.

use std::collections::HashMap;
use std::hash::Hash;

use arc_swap::ArcSwap;
use museair::FixedState;

/// Hasher used by [`LeakedCache`]. Replaces `RandomState` (SipHash13). The
/// cache is process-local, never receives untrusted input, and keys are
/// stable code-segment pointers + small integers — cryptographic resistance
/// buys nothing here, and SipHash on small keys was visible in profiles.
pub type CacheHasher = FixedState;

type Map<K, V> = HashMap<K, &'static V, CacheHasher>;

fn new_map<K, V>() -> Map<K, V> {
    HashMap::with_hasher(FixedState::new(0))
}

/// Process-local, lock-free-read, leak-on-insert cache keyed by `K` with
/// `&'static V` values.
///
/// Steady-state lookup is one atomic load + one pointer copy via `ArcSwap`.
/// Insertions copy-on-write the inner `HashMap` via `ArcSwap::rcu`, so
/// they're rare-path operations only (one per distinct key ever seen by
/// this process).
pub struct LeakedCache<K, V>
where
    K: Hash + Eq + Copy + 'static,
    V: 'static,
{
    map: ArcSwap<Map<K, V>>,
}

impl<K, V> Default for LeakedCache<K, V>
where
    K: Hash + Eq + Copy + 'static,
    V: 'static,
{
    fn default() -> Self {
        Self {
            map: ArcSwap::from_pointee(new_map()),
        }
    }
}

impl<K, V> LeakedCache<K, V>
where
    K: Hash + Eq + Copy + 'static,
    V: 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up an entry by key. One atomic load and (on hit) one pointer copy.
    pub fn get(&self, key: &K) -> Option<&'static V> {
        self.map.load().get(key).copied()
    }

    /// Insert `value`, leak it, and return the resulting `&'static V`. The
    /// entry lives for the process lifetime. If a concurrent insert wins
    /// the race, the leaked allocation we created is leaked but unreferenced
    /// — a single one-time cost per loser.
    pub fn insert(&self, key: K, value: V) -> &'static V {
        let leaked: &'static V = Box::leak(Box::new(value));
        self.map.rcu(|cur| {
            let mut next = (**cur).clone();
            next.insert(key, leaked);
            next
        });
        leaked
    }

    /// Look up an entry, or insert one produced by `make` if absent. Returns
    /// the existing entry on race so all callers observe the same `&'static V`.
    pub fn get_or_insert_with(&self, key: K, make: impl FnOnce() -> V) -> &'static V {
        if let Some(entry) = self.get(&key) {
            return entry;
        }
        let leaked: &'static V = Box::leak(Box::new(make()));
        let mut inserted = leaked;
        self.map.rcu(|cur| {
            if let Some(&existing) = cur.get(&key) {
                inserted = existing;
                return (**cur).clone();
            }
            let mut next = (**cur).clone();
            next.insert(key, leaked);
            inserted = leaked;
            next
        });
        inserted
    }

    /// Number of entries currently cached.
    pub fn len(&self) -> usize {
        self.map.load().len()
    }

    /// Returns `true` if no entries are cached.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
