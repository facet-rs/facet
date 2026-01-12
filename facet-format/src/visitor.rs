extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};

use facet_reflect::FieldInfo;
use facet_solver::Resolution;

/// Result of checking a serialized field against the active resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldMatch<'a> {
    /// Field exists in the schema and this is the first time we've seen it.
    KnownFirst(&'a FieldInfo),
    /// Field exists but was already provided earlier (duplicate).
    Duplicate(&'a FieldInfo),
    /// Field name is not part of this resolution.
    Unknown,
}

/// Tracks which fields have been seen while deserializing a struct.
///
/// This is the first building block of the shared visitor requested in issue
/// #1127 – it centralizes duplicate detection, unknown-field tracking, and
/// default synthesis hooks so individual format crates don't have to reimplement
/// the bookkeeping.
pub struct StructFieldTracker<'a> {
    resolution: &'a Resolution,
    seen: BTreeSet<&'static str>,
}

impl<'a> StructFieldTracker<'a> {
    /// Create a tracker for the given resolution.
    pub const fn new(resolution: &'a Resolution) -> Self {
        Self {
            resolution,
            seen: BTreeSet::new(),
        }
    }

    /// Record an incoming serialized field and classify it.
    pub fn record(&mut self, name: &str) -> FieldMatch<'a> {
        match self.resolution.field(name) {
            Some(info) => {
                if self.seen.insert(info.serialized_name) {
                    FieldMatch::KnownFirst(info)
                } else {
                    FieldMatch::Duplicate(info)
                }
            }
            None => FieldMatch::Unknown,
        }
    }

    /// Return serialized names for required fields that have not been seen yet.
    pub fn missing_required(&self) -> Vec<&'static str> {
        self.resolution
            .required_field_names()
            .iter()
            .copied()
            .filter(|name| !self.seen.contains(name))
            .collect()
    }

    /// Iterate over optional fields that are still unset (useful for defaults).
    pub fn missing_optional(&self) -> impl Iterator<Item = &'a FieldInfo> {
        self.resolution
            .fields()
            .values()
            .filter(move |info| !info.required && !self.seen.contains(&info.serialized_name))
    }
}
