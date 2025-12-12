use crate::revision::Revision;
use std::sync::atomic::{AtomicU64, Ordering};

/// Shared runtime state for a Picante database: primarily the current revision.
#[derive(Debug, Default)]
pub struct Runtime {
    current_revision: AtomicU64,
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

    /// Bump the current revision and return the new value.
    pub fn bump_revision(&self) -> Revision {
        let next = self.current_revision.fetch_add(1, Ordering::AcqRel) + 1;
        Revision(next)
    }

    /// Set the current revision (intended for cache loading).
    pub fn set_current_revision(&self, revision: Revision) {
        self.current_revision.store(revision.0, Ordering::Release);
    }
}

/// Trait for database types that expose a [`Runtime`].
pub trait HasRuntime {
    /// Access the database runtime.
    fn runtime(&self) -> &Runtime;
}
