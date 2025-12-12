//! Shared runtime state for a Picante database (revisions, notifications, etc.).

use crate::key::{Key, QueryKindId};
use crate::revision::Revision;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{broadcast, watch};

/// Shared runtime state for a Picante database: primarily the current revision.
#[derive(Debug)]
pub struct Runtime {
    current_revision: AtomicU64,
    revision_tx: watch::Sender<Revision>,
    events_tx: broadcast::Sender<RuntimeEvent>,
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
        let _ = self.events_tx.send(RuntimeEvent::InputSet {
            revision,
            kind,
            key_hash: key.hash(),
            key,
        });
    }

    /// Emit an input removal event (for live reload / diagnostics).
    pub fn notify_input_removed(&self, revision: Revision, kind: QueryKindId, key: Key) {
        let _ = self.events_tx.send(RuntimeEvent::InputRemoved {
            revision,
            kind,
            key_hash: key.hash(),
            key,
        });
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
}

/// Trait for database types that expose a [`Runtime`].
pub trait HasRuntime {
    /// Access the database runtime.
    fn runtime(&self) -> &Runtime;
}
