use std::collections::BTreeMap;

use weavy::exec::StoreHandle;
use weavy::task::{ValueMemories, ValueMemory};

use super::identity::{Digest, SchemaId, ValueId, hash_framed};
use super::model::FailureValue;

/// Store-owned handle. It is valid for one runtime snapshot and is never
/// reused for a different entry during that lifetime. Resident bytes may be
/// evicted and rehydrated without changing the handle.
///
/// r[impl machine.store.handle-opaque]
/// r[impl machine.store.handle-store-assigned]
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle(u32);

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum HandleTier {
    Pending,
    Realized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Residence {
    Resident(Vec<u8>),
    Evicted { sources: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreEntry {
    pub handle: Handle,
    pub identity: ValueId,
    pub tier: HandleTier,
    residence: Residence,
    failure: Option<FailureValue>,
}

impl StoreEntry {
    #[must_use]
    pub fn resident_bytes(&self) -> Option<&[u8]> {
        match &self.residence {
            Residence::Resident(bytes) => Some(bytes),
            Residence::Evicted { .. } => None,
        }
    }

    #[must_use]
    pub fn failure(&self) -> Option<&FailureValue> {
        self.failure.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StoreKey {
    schema: SchemaId,
    tier: HandleTier,
    content: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interned {
    pub handle: Handle,
    pub identity: ValueId,
    pub deduped: bool,
    pub bytes_hashed: u64,
}

#[derive(Default)]
pub struct Store {
    entries: Vec<StoreEntry>,
    by_identity: BTreeMap<StoreKey, Handle>,
}

impl Store {
    /// Construct identity once, carry it on the entry, and deduplicate by the
    /// schema/tier/content triple.
    ///
    /// r[impl machine.identity.value-identity-pair]
    /// r[impl machine.identity.hash-at-construction]
    /// r[impl machine.store.dedup]
    pub fn intern_realized(&mut self, schema: SchemaId, bytes: &[u8]) -> Interned {
        let content = hash_framed(b"vix.value.v1", &[&schema.0.0, bytes]);
        let identity = ValueId { schema, content };
        let key = StoreKey {
            schema,
            tier: HandleTier::Realized,
            content,
        };
        if let Some(&handle) = self.by_identity.get(&key) {
            return Interned {
                handle,
                identity,
                deduped: true,
                bytes_hashed: bytes.len() as u64,
            };
        }
        let handle = Handle(self.entries.len() as u32);
        self.entries.push(StoreEntry {
            handle,
            identity,
            tier: HandleTier::Realized,
            residence: Residence::Resident(bytes.to_vec()),
            failure: None,
        });
        self.by_identity.insert(key, handle);
        Interned {
            handle,
            identity,
            deduped: false,
            bytes_hashed: bytes.len() as u64,
        }
    }

    /// Failure identities are constructed solely from typed semantic fields;
    /// resident bytes are a separate, non-identity-bearing storage concern.
    pub(crate) fn intern_failure(&mut self, failure: FailureValue, resident: &[u8]) -> Interned {
        let schema = SchemaId::named("vix.Failure.v1");
        let identity = failure_identity(schema, &failure);
        let key = StoreKey {
            schema,
            tier: HandleTier::Realized,
            content: identity.content,
        };
        if let Some(&handle) = self.by_identity.get(&key) {
            return Interned {
                handle,
                identity,
                deduped: true,
                bytes_hashed: resident.len() as u64,
            };
        }
        let handle = Handle(self.entries.len() as u32);
        self.entries.push(StoreEntry {
            handle,
            identity,
            tier: HandleTier::Realized,
            residence: Residence::Resident(resident.to_vec()),
            failure: Some(failure),
        });
        self.by_identity.insert(key, handle);
        Interned {
            handle,
            identity,
            deduped: false,
            bytes_hashed: resident.len() as u64,
        }
    }

    #[must_use]
    pub fn entry(&self, handle: Handle) -> Option<&StoreEntry> {
        self.entries.get(handle.0 as usize)
    }

    /// Convert only a live store-owned handle to Weavy's opaque entry handle.
    pub(crate) fn weavy_handle(&self, handle: Handle) -> Option<StoreHandle> {
        self.entry(handle)?;
        StoreHandle::new(handle.0 as usize)
    }

    /// Lend Weavy a non-owning memory table for the duration of one drive.
    /// Resident bodies stay borrowed from the store and are never cloned.
    pub(crate) fn with_value_memories<R>(
        &self,
        use_memories: impl FnOnce(ValueMemories<'_>) -> R,
    ) -> R {
        let store = self
            .entries
            .iter()
            .map(|entry| match &entry.residence {
                Residence::Resident(bytes) => ValueMemory::from_slice(bytes),
                Residence::Evicted { .. } => ValueMemory::empty(),
            })
            .collect::<Vec<_>>();
        use_memories(ValueMemories {
            store: &store,
            molten: &[],
        })
    }

    /// Borrowed inspection never clones resident bodies.
    ///
    /// r[impl machine.store.snapshot-no-clone]
    pub fn inspect(&self) -> impl Iterator<Item = &StoreEntry> {
        self.entries.iter()
    }
}

fn failure_identity(schema: SchemaId, failure: &FailureValue) -> ValueId {
    match failure {
        FailureValue::IndexOutOfBounds {
            recipe,
            site,
            index,
            length,
            subject,
        } => {
            let tag = [1u8];
            let site = site.to_le_bytes();
            let index = index.to_le_bytes();
            let length = length.to_le_bytes();
            let none = [0u8];
            let some = [1u8];
            let mut fields = vec![
                &schema.0.0[..],
                &tag[..],
                &recipe.0.0[..],
                &site[..],
                &index[..],
                &length[..],
            ];
            match subject {
                Some(subject) => {
                    fields.push(&some);
                    fields.push(&subject.schema.0.0);
                    fields.push(&subject.content.0);
                }
                None => fields.push(&none),
            }
            ValueId {
                schema,
                content: hash_framed(b"vix.value.v1", &fields),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RecipeId;

    fn failure(site: u32, index: i64, length: i64) -> FailureValue {
        FailureValue::IndexOutOfBounds {
            recipe: RecipeId::from_canonical_vir(b"array-failure-recipe"),
            site,
            index,
            length,
            subject: None,
        }
    }

    #[test]
    fn failure_identity_is_semantic_and_residence_independent() {
        let mut store = Store::default();
        let first = store.intern_failure(failure(7, 9, 3), b"first report memory");
        let replay = store.intern_failure(failure(7, 9, 3), b"different report memory");
        assert_eq!(first.identity, replay.identity);
        assert_eq!(first.handle, replay.handle);
        assert!(replay.deduped);
        assert_ne!(
            first.identity,
            store.intern_failure(failure(8, 9, 3), b"").identity
        );
        assert_ne!(
            first.identity,
            store.intern_failure(failure(7, 10, 3), b"").identity
        );
        assert_ne!(
            first.identity,
            store.intern_failure(failure(7, 9, 4), b"").identity
        );
        assert!(matches!(
            store.entry(first.handle).and_then(StoreEntry::failure),
            Some(FailureValue::IndexOutOfBounds { subject: None, .. })
        ));
    }
}
