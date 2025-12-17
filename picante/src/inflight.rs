//! In-flight query deduplication registry.
//!
//! This module provides a global registry for tracking in-flight computations,
//! allowing concurrent queries for the same key to coalesce into a single
//! computation across different database snapshots.

use crate::error::PicanteError;
use crate::key::{Dep, Key, QueryKindId};
use crate::revision::Revision;
use crate::runtime::RuntimeId;
use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::trace;

/// Type-erased result value from a computation.
pub(crate) type ArcAny = Arc<dyn std::any::Any + Send + Sync>;

/// Global registry for in-flight computations.
///
/// This allows concurrent queries from different database snapshots to share
/// in-flight work instead of computing the same value multiple times.
static IN_FLIGHT_REGISTRY: std::sync::LazyLock<DashMap<InFlightKey, Arc<InFlightEntry>>> =
    std::sync::LazyLock::new(DashMap::new);

// ============================================================================
// Shared completed-result cache (cross-snapshot memoization)
// ============================================================================

/// A completed derived-query result that can be adopted by other runtimes/snapshots.
#[derive(Clone)]
pub(crate) struct SharedCacheRecord {
    pub(crate) value: ArcAny,
    pub(crate) deps: Arc<[Dep]>,
    pub(crate) changed_at: Revision,
    pub(crate) verified_at: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SharedCacheKey {
    runtime_id: RuntimeId,
    kind: QueryKindId,
    key: Key,
}

static SHARED_CACHE: std::sync::LazyLock<DashMap<SharedCacheKey, SharedCacheRecord>> =
    std::sync::LazyLock::new(DashMap::new);

static SHARED_CACHE_ORDER: std::sync::LazyLock<parking_lot::Mutex<VecDeque<SharedCacheKey>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(VecDeque::new()));

static SHARED_CACHE_MAX_ENTRIES: std::sync::LazyLock<usize> = std::sync::LazyLock::new(|| {
    std::env::var("PICANTE_SHARED_CACHE_MAX_ENTRIES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20_000)
});

pub(crate) fn shared_cache_get(
    runtime_id: RuntimeId,
    kind: QueryKindId,
    key: &Key,
) -> Option<SharedCacheRecord> {
    let k = SharedCacheKey {
        runtime_id,
        kind,
        key: key.clone(),
    };
    SHARED_CACHE.get(&k).map(|v| v.clone())
}

pub(crate) fn shared_cache_put(
    runtime_id: RuntimeId,
    kind: QueryKindId,
    key: Key,
    record: SharedCacheRecord,
) {
    let k = SharedCacheKey {
        runtime_id,
        kind,
        key,
    };

    // Record insertion order for simple FIFO eviction.
    {
        let mut order = SHARED_CACHE_ORDER.lock();
        order.push_back(k.clone());
        while SHARED_CACHE.len() > *SHARED_CACHE_MAX_ENTRIES {
            if let Some(old) = order.pop_front() {
                SHARED_CACHE.remove(&old);
            } else {
                break;
            }
        }
    }

    SHARED_CACHE.insert(k, record);
}

/// Key identifying an in-flight computation.
///
/// Two queries are considered the same if they have the same:
/// - Runtime identity (database family)
/// - Revision
/// - Query kind (ingredient)
/// - Query key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct InFlightKey {
    pub runtime_id: RuntimeId,
    pub revision: Revision,
    pub kind: QueryKindId,
    /// Full query key bytes; equality must be exact to preserve correctness.
    pub key: Key,
}

/// State of an in-flight computation.
#[derive(Debug, Clone)]
pub(crate) enum InFlightState {
    /// Computation is still running.
    Running,
    /// Computation completed successfully with a value and its dependencies.
    Done {
        value: ArcAny,
        deps: Arc<[Dep]>,
        changed_at: Revision,
    },
    /// Computation failed with an error.
    Failed(Arc<PicanteError>),
    /// Computation was cancelled (leader dropped).
    Cancelled,
}

/// An entry in the in-flight registry.
pub(crate) struct InFlightEntry {
    /// Current state of the computation. Protected by parking_lot::Mutex for sync access.
    state: parking_lot::Mutex<InFlightState>,
    /// Notifier for waiters.
    notify: Notify,
}

impl InFlightEntry {
    fn new() -> Self {
        Self {
            state: parking_lot::Mutex::new(InFlightState::Running),
            notify: Notify::new(),
        }
    }

    /// Get the current state.
    pub(crate) fn state(&self) -> InFlightState {
        self.state.lock().clone()
    }

    /// Set the state to Done and notify waiters.
    fn complete(&self, value: ArcAny, deps: Arc<[Dep]>, changed_at: Revision) {
        trace!("inflight entry: completing with success");
        *self.state.lock() = InFlightState::Done {
            value,
            deps,
            changed_at,
        };
        self.notify.notify_waiters();
    }

    /// Set the state to Failed and notify waiters.
    fn fail(&self, error: Arc<PicanteError>) {
        trace!("inflight entry: completing with error");
        *self.state.lock() = InFlightState::Failed(error);
        self.notify.notify_waiters();
    }

    /// Set the state to Cancelled and notify waiters.
    fn cancel(&self) {
        trace!("inflight entry: cancelled (leader dropped)");
        *self.state.lock() = InFlightState::Cancelled;
        self.notify.notify_waiters();
    }

    /// Wait for the computation to complete.
    pub(crate) fn notified(&self) -> impl std::future::Future<Output = ()> + '_ {
        self.notify.notified()
    }
}

/// Result of trying to become the leader for an in-flight computation.
pub(crate) enum TryLeadResult {
    /// We became the leader. The guard MUST be used to complete/fail/cancel.
    Leader(InFlightGuard),
    /// Someone else is already computing. Wait on the entry.
    Follower(Arc<InFlightEntry>),
}

/// Guard that ensures the in-flight entry is properly cleaned up.
///
/// When dropped without calling `complete` or `fail`, marks the entry as cancelled.
pub(crate) struct InFlightGuard {
    key: InFlightKey,
    entry: Arc<InFlightEntry>,
    completed: bool,
}

impl InFlightGuard {
    /// Mark the computation as successfully completed.
    pub(crate) fn complete(mut self, value: ArcAny, deps: Arc<[Dep]>, changed_at: Revision) {
        self.entry.complete(value, deps, changed_at);
        self.completed = true;
        // Entry stays in registry briefly so followers can read the result,
        // then we remove it.
        IN_FLIGHT_REGISTRY.remove(&self.key);
    }

    /// Mark the computation as failed.
    pub(crate) fn fail(mut self, error: Arc<PicanteError>) {
        self.entry.fail(error);
        self.completed = true;
        IN_FLIGHT_REGISTRY.remove(&self.key);
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if !self.completed {
            // Leader was dropped without completing (likely cancelled/panicked).
            // Mark as cancelled so followers can retry.
            self.entry.cancel();
            IN_FLIGHT_REGISTRY.remove(&self.key);
        }
    }
}

/// Try to become the leader for a computation, or get the existing entry if
/// someone else is already computing.
pub(crate) fn try_lead(key: InFlightKey) -> TryLeadResult {
    use dashmap::mapref::entry::Entry;

    match IN_FLIGHT_REGISTRY.entry(key.clone()) {
        Entry::Occupied(occupied) => {
            // Someone else is already computing (or has completed).
            TryLeadResult::Follower(occupied.get().clone())
        }
        Entry::Vacant(vacant) => {
            // We're the first - become the leader.
            let entry = Arc::new(InFlightEntry::new());
            vacant.insert(entry.clone());
            TryLeadResult::Leader(InFlightGuard {
                key,
                entry,
                completed: false,
            })
        }
    }
}
