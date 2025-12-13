//! Shared runtime state for a Picante database (revisions, notifications, etc.).

use crate::key::{Dep, DynKey, Key, QueryKindId};
use crate::revision::Revision;
use dashmap::{DashMap, DashSet};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{broadcast, watch};

/// Shared runtime state for a Picante database: primarily the current revision.
#[derive(Debug)]
pub struct Runtime {
    current_revision: AtomicU64,
    revision_tx: watch::Sender<Revision>,
    events_tx: broadcast::Sender<RuntimeEvent>,
    deps_by_query: DashMap<DynKey, Arc<[Dep]>>,
    reverse_deps: DashMap<DynKey, DashSet<DynKey>>,
}

impl Runtime {
    /// Create a new runtime starting at revision 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the current revision.
    pub fn current_revision(&self) -> Revision {
        Revision(self.current_revision.load(Ordering::Acquire))
    }

    /// Subscribe to revision changes.
    pub fn subscribe_revisions(&self) -> watch::Receiver<Revision> {
        self.revision_tx.subscribe()
    }

    /// Subscribe to runtime events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.events_tx.subscribe()
    }

    /// Bump the current revision and return the new value.
    pub fn bump_revision(&self) -> Revision {
        let next = self.current_revision.fetch_add(1, Ordering::AcqRel) + 1;
        let rev = Revision(next);
        self.revision_tx.send_replace(rev);
        let _ = self
            .events_tx
            .send(RuntimeEvent::RevisionBumped { revision: rev });
        rev
    }

    /// Set the current revision (intended for cache loading).
    pub fn set_current_revision(&self, revision: Revision) {
        self.current_revision.store(revision.0, Ordering::Release);
        self.revision_tx.send_replace(revision);
        let _ = self.events_tx.send(RuntimeEvent::RevisionSet { revision });
    }

    /// Emit an input change event (for live reload / diagnostics).
    pub fn notify_input_set(&self, revision: Revision, kind: QueryKindId, key: Key) {
        let source = DynKey {
            kind,
            key: key.clone(),
        };
        let _ = self.events_tx.send(RuntimeEvent::InputSet {
            revision,
            kind,
            key_hash: key.hash(),
            key,
        });
        self.propagate_invalidation(revision, &source);
    }

    /// Emit an input removal event (for live reload / diagnostics).
    pub fn notify_input_removed(&self, revision: Revision, kind: QueryKindId, key: Key) {
        let source = DynKey {
            kind,
            key: key.clone(),
        };
        let _ = self.events_tx.send(RuntimeEvent::InputRemoved {
            revision,
            kind,
            key_hash: key.hash(),
            key,
        });
        self.propagate_invalidation(revision, &source);
    }

    /// Update the dependency edges for `query`.
    pub fn update_query_deps(&self, query: DynKey, deps: Arc<[Dep]>) {
        let old = self.deps_by_query.insert(query.clone(), deps.clone());

        let new_set: HashSet<Dep> = deps.iter().cloned().collect();

        let old_set: HashSet<Dep> = old
            .as_deref()
            .map(|d| d.iter().cloned().collect())
            .unwrap_or_default();

        for dep in old_set.difference(&new_set) {
            let dep_key = DynKey {
                kind: dep.kind,
                key: dep.key.clone(),
            };

            if let Some(set) = self.reverse_deps.get(&dep_key) {
                set.remove(&query);
                if set.is_empty() {
                    drop(set);
                    self.reverse_deps.remove(&dep_key);
                }
            }
        }

        for dep in new_set {
            let dep_key = DynKey {
                kind: dep.kind,
                key: dep.key.clone(),
            };

            let set = self.reverse_deps.entry(dep_key).or_default();
            set.insert(query.clone());
        }
    }

    /// Emit a derived query change event (for live reload / diagnostics).
    pub fn notify_query_changed(&self, revision: Revision, query: DynKey) {
        let _ = self.events_tx.send(RuntimeEvent::QueryChanged {
            revision,
            kind: query.kind,
            key_hash: query.key.hash(),
            key: query.key,
        });
    }

    /// Clear the in-memory dependency graph (used during cache loads).
    pub fn clear_dependency_graph(&self) {
        self.deps_by_query.clear();
        self.reverse_deps.clear();
    }

    fn propagate_invalidation(&self, revision: Revision, source: &DynKey) {
        let mut queue = VecDeque::new();
        let mut seen: HashSet<DynKey> = HashSet::new();

        queue.push_back(source.clone());
        seen.insert(source.clone());

        while let Some(node) = queue.pop_front() {
            let Some(dependents) = self.reverse_deps.get(&node) else {
                continue;
            };

            for dependent in dependents.iter() {
                let dependent = dependent.clone();
                if !seen.insert(dependent.clone()) {
                    continue;
                }

                let _ = self.events_tx.send(RuntimeEvent::QueryInvalidated {
                    revision,
                    kind: dependent.kind,
                    key_hash: dependent.key.hash(),
                    key: dependent.key.clone(),
                    by_kind: source.kind,
                    by_key_hash: source.key.hash(),
                    by_key: source.key.clone(),
                });

                queue.push_back(dependent);
            }
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        let (revision_tx, _) = watch::channel(Revision(0));
        let (events_tx, _) = broadcast::channel(1024);
        Self {
            current_revision: AtomicU64::new(0),
            revision_tx,
            events_tx,
            deps_by_query: DashMap::new(),
            reverse_deps: DashMap::new(),
        }
    }
}

/// Notifications emitted by a [`Runtime`].
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// The global revision counter was bumped.
    RevisionBumped {
        /// New revision value.
        revision: Revision,
    },
    /// The global revision counter was set directly (usually after cache load).
    RevisionSet {
        /// New revision value.
        revision: Revision,
    },
    /// An input value was set.
    InputSet {
        /// Revision at which the input was set.
        revision: Revision,
        /// Kind id of the input ingredient.
        kind: QueryKindId,
        /// Stable hash of the encoded key bytes (for diagnostics).
        key_hash: u64,
        /// Postcard-encoded key bytes.
        key: Key,
    },
    /// An input value was removed.
    InputRemoved {
        /// Revision at which the input was removed.
        revision: Revision,
        /// Kind id of the input ingredient.
        kind: QueryKindId,
        /// Stable hash of the encoded key bytes (for diagnostics).
        key_hash: u64,
        /// Postcard-encoded key bytes.
        key: Key,
    },
    /// A derived query was invalidated by an input change.
    QueryInvalidated {
        /// Revision at which invalidation happened.
        revision: Revision,
        /// Kind id of the invalidated query.
        kind: QueryKindId,
        /// Stable hash of the invalidated key bytes (for diagnostics).
        key_hash: u64,
        /// Postcard-encoded key bytes for the invalidated query.
        key: Key,
        /// Kind id of the root input that caused invalidation.
        by_kind: QueryKindId,
        /// Stable hash of the root input key bytes (for diagnostics).
        by_key_hash: u64,
        /// Postcard-encoded key bytes for the root input key.
        by_key: Key,
    },
    /// A derived query's output changed at `revision`.
    QueryChanged {
        /// Revision at which the query changed.
        revision: Revision,
        /// Kind id of the changed query.
        kind: QueryKindId,
        /// Stable hash of the changed key bytes (for diagnostics).
        key_hash: u64,
        /// Postcard-encoded key bytes for the changed query.
        key: Key,
    },
}

/// Trait for database types that expose a [`Runtime`].
pub trait HasRuntime {
    /// Access the database runtime.
    fn runtime(&self) -> &Runtime;
}
