use std::collections::BTreeMap;

use weavy::exec::StoreHandle;
use weavy::task::{ValueMemories, ValueMemory};

use super::identity::{Digest, SchemaId, ValueId, hash_framed};

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
}

impl StoreEntry {
    #[must_use]
    pub fn resident_bytes(&self) -> Option<&[u8]> {
        match &self.residence {
            Residence::Resident(bytes) => Some(bytes),
            Residence::Evicted { .. } => None,
        }
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
        });
        self.by_identity.insert(key, handle);
        Interned {
            handle,
            identity,
            deduped: false,
            bytes_hashed: bytes.len() as u64,
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
