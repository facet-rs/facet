use std::collections::BTreeMap;

use weavy::exec::StoreHandle;
use weavy::task::{ValueMemories, ValueMemory};

use super::identity::{Digest, FramedField, FramedNode, FramedValue, SchemaId, ValueId};
use super::model::{FailureValue, ProcessTermination};

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
    frozen: Option<FrozenValue>,
}

/// Scheduler-owned semantic execution representation for a published value.
/// References are retained by `ValueId`, never by task-local or store handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FrozenValue {
    Inline(Vec<u8>),
    Opaque(Vec<u8>),
    Reference(ValueId),
    Product(Vec<FrozenValue>),
    Variant { tag: u32, fields: Vec<FrozenValue> },
    DenseArray(Vec<FrozenValue>),
    OrderedMap(Vec<(FrozenValue, FrozenValue)>),
    OrderedSet(Vec<FrozenValue>),
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

    pub(crate) fn frozen(&self) -> Option<&FrozenValue> {
        self.frozen.as_ref()
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

#[derive(Clone, Default)]
pub struct Store {
    entries: Vec<StoreEntry>,
    by_identity: BTreeMap<StoreKey, Handle>,
}

impl Store {
    /// Intern a realized scalar/opaque value. The value becomes a framed
    /// scalar/opaque leaf whose identity is computed by the closed writer, then
    /// deduplicated and stored through the single `intern_tree` path.
    ///
    /// r[impl machine.identity.value-identity-pair]
    /// r[impl machine.identity.hash-at-construction]
    /// r[impl machine.store.dedup]
    pub fn intern_realized(&mut self, schema: SchemaId, bytes: &[u8]) -> Interned {
        let node = FramedNode::leaf(schema, bytes.to_vec());
        self.intern_tree(&node, bytes)
    }

    /// Compute a pre-resolved semantic tree's identity through the closed
    /// writer, deduplicate on `(SchemaId, tier, content)`, and — on first sight
    /// — store its canonical resident bytes as a separate, non-identity-bearing
    /// storage concern. Identity is computed once, before the single mutation.
    ///
    /// r[impl machine.identity.value-identity-pair]
    /// r[impl machine.identity.hash-at-construction]
    /// r[impl machine.store.dedup]
    pub fn intern_tree(&mut self, node: &FramedNode, resident: &[u8]) -> Interned {
        let identity = node.identity();
        self.intern_identity(
            identity,
            Residence::Resident(resident.to_vec()),
            None,
            resident.len() as u64,
        )
    }

    pub(crate) fn attach_frozen(&mut self, handle: Handle, frozen: FrozenValue) {
        if let Some(entry) = self.entries.get_mut(handle.0 as usize)
            && entry.frozen.is_none()
        {
            entry.frozen = Some(frozen);
        }
    }

    pub(crate) fn handle_for_identity(&self, identity: ValueId) -> Option<Handle> {
        self.by_identity
            .get(&StoreKey {
                schema: identity.schema,
                tier: HandleTier::Realized,
                content: identity.content,
            })
            .copied()
    }

    /// Failure identities are constructed solely from typed semantic fields via
    /// start/variant/field/child roles; resident report bytes are a separate,
    /// non-identity-bearing storage concern.
    ///
    /// r[impl machine.error.failure-source-site-identity]
    pub(crate) fn intern_failure(&mut self, failure: FailureValue, resident: &[u8]) -> Interned {
        let node = failure_node(&failure);
        let identity = node.identity();
        self.intern_identity(
            identity,
            Residence::Resident(resident.to_vec()),
            Some(failure),
            resident.len() as u64,
        )
    }

    /// The single dedupe-and-mutate core shared by every intern path. Identity
    /// is already computed; this only looks up or appends one entry.
    fn intern_identity(
        &mut self,
        identity: ValueId,
        residence: Residence,
        failure: Option<FailureValue>,
        bytes_hashed: u64,
    ) -> Interned {
        let key = StoreKey {
            schema: identity.schema,
            tier: HandleTier::Realized,
            content: identity.content,
        };
        if let Some(&handle) = self.by_identity.get(&key) {
            return Interned {
                handle,
                identity,
                deduped: true,
                bytes_hashed,
            };
        }
        let handle = Handle(self.entries.len() as u32);
        self.entries.push(StoreEntry {
            handle,
            identity,
            tier: HandleTier::Realized,
            residence,
            failure,
            frozen: None,
        });
        self.by_identity.insert(key, handle);
        Interned {
            handle,
            identity,
            deduped: false,
            bytes_hashed,
        }
    }

    #[must_use]
    pub fn entry(&self, handle: Handle) -> Option<&StoreEntry> {
        self.entries.get(handle.0 as usize)
    }

    pub(crate) fn entry_by_weavy_handle(&self, handle: StoreHandle) -> Option<&StoreEntry> {
        self.entries.get(handle.index())
    }

    /// Convert only a live store-owned handle to Weavy's opaque entry handle.
    pub(crate) fn weavy_handle(&self, handle: Handle) -> Option<StoreHandle> {
        self.entry(handle)?;
        StoreHandle::new(handle.0 as usize)
    }

    /// Lend store memory with invocation-local ABI views for published values.
    /// Overrides alter only the verified consumer's schema witness bytes; the
    /// store's canonical resident body and semantic identity remain unchanged.
    pub(crate) fn with_value_memory_overrides<R>(
        &self,
        overrides: &[(Handle, Vec<u8>)],
        use_memories: impl FnOnce(ValueMemories<'_>) -> R,
    ) -> R {
        let store = self
            .entries
            .iter()
            .map(|entry| {
                if let Some((_, bytes)) =
                    overrides.iter().find(|(handle, _)| *handle == entry.handle)
                {
                    return ValueMemory::from_slice(bytes);
                }
                match &entry.residence {
                    Residence::Resident(bytes) => ValueMemory::from_slice(bytes),
                    Residence::Evicted { .. } => ValueMemory::empty(),
                }
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

/// Build the pre-resolved framed tree for a failure value using
/// start/variant/field/child roles. Source-site fields determine identity; the
/// subject (when present) contributes by referent `ValueId`. Resident report
/// memory never appears here.
///
/// r[impl machine.error.failure-source-site-identity]
fn failure_node(failure: &FailureValue) -> FramedNode {
    let schema = SchemaId::named("vix.Failure.v1");
    let recipe_schema = SchemaId::named("vix.RecipeId");
    let site_schema = SchemaId::named("vix.FailureSite");
    match failure {
        FailureValue::IndexOutOfBounds {
            recipe,
            site,
            index,
            length,
            subject,
        } => FramedNode::Variant {
            schema,
            tag: 1,
            fields: vec![
                FramedField {
                    schema: recipe_schema,
                    value: FramedValue::Bytes(recipe.0.0.to_vec()),
                },
                FramedField {
                    schema: site_schema,
                    value: FramedValue::Bytes(site.to_le_bytes().to_vec()),
                },
                FramedField {
                    schema: SchemaId::named("vix.i64.index"),
                    value: FramedValue::Bytes(index.to_le_bytes().to_vec()),
                },
                FramedField {
                    schema: SchemaId::named("vix.i64.length"),
                    value: FramedValue::Bytes(length.to_le_bytes().to_vec()),
                },
                FramedField {
                    schema: SchemaId::named("vix.Failure.subject"),
                    value: FramedValue::Optional(*subject),
                },
            ],
        },
        FailureValue::MissingKey { recipe, site }
        | FailureValue::DuplicateKey { recipe, site }
        | FailureValue::MissingDelimiter { recipe, site }
        | FailureValue::InvalidInteger { recipe, site }
        | FailureValue::IntegerOverflow { recipe, site }
        | FailureValue::DivisionByZero { recipe, site }
        | FailureValue::MissingTreeEntry { recipe, site }
        | FailureValue::TreeEntryNotAFile { recipe, site }
        | FailureValue::InvalidText { recipe, site }
        | FailureValue::MissingRegistryArtifact { recipe, site }
        | FailureValue::FetchIntegrity { recipe, site }
        | FailureValue::MalformedArchive { recipe, site } => {
            let tag = match failure {
                FailureValue::MissingKey { .. } => 2,
                FailureValue::DuplicateKey { .. } => 3,
                FailureValue::MissingDelimiter { .. } => 4,
                FailureValue::InvalidInteger { .. } => 5,
                FailureValue::IntegerOverflow { .. } => 6,
                FailureValue::DivisionByZero { .. } => 7,
                FailureValue::MissingTreeEntry { .. } => 8,
                FailureValue::TreeEntryNotAFile { .. } => 9,
                FailureValue::InvalidText { .. } => 10,
                FailureValue::MissingRegistryArtifact { .. } => 11,
                FailureValue::FetchIntegrity { .. } => 12,
                FailureValue::MalformedArchive { .. } => 13,
                FailureValue::IndexOutOfBounds { .. } | FailureValue::ProcessFailure { .. } => {
                    unreachable!("matched above")
                }
            };
            FramedNode::Variant {
                schema,
                tag,
                fields: vec![
                    FramedField {
                        schema: recipe_schema,
                        value: FramedValue::Bytes(recipe.0.0.to_vec()),
                    },
                    FramedField {
                        schema: site_schema,
                        value: FramedValue::Bytes(site.to_le_bytes().to_vec()),
                    },
                ],
            }
        }
        FailureValue::ProcessFailure {
            recipe,
            site,
            termination,
        } => {
            // The raw termination information is semantic failure content: an
            // exit-code failure and a signal failure are different values.
            let (kind, value) = match termination {
                ProcessTermination::Exited { code } => (0u8, *code),
                ProcessTermination::Signaled { signal } => (1u8, *signal),
            };
            let mut termination_bytes = vec![kind];
            termination_bytes.extend_from_slice(&value.to_le_bytes());
            FramedNode::Variant {
                schema,
                tag: 8,
                fields: vec![
                    FramedField {
                        schema: recipe_schema,
                        value: FramedValue::Bytes(recipe.0.0.to_vec()),
                    },
                    FramedField {
                        schema: site_schema,
                        value: FramedValue::Bytes(site.to_le_bytes().to_vec()),
                    },
                    FramedField {
                        schema: SchemaId::named("vix.ProcessTermination"),
                        value: FramedValue::Bytes(termination_bytes),
                    },
                ],
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
    fn intern_tree_dedupes_on_identity_and_keeps_resident_bytes_separate() {
        let mut store = Store::default();
        let node = FramedNode::leaf(SchemaId::named("vix.demo"), b"canonical".to_vec());

        let first = store.intern_tree(&node, b"canonical");
        // Same semantic value, different (non-identity) resident bytes: identity
        // and handle are stable; the store keeps the first resident copy.
        let again = store.intern_tree(&node, b"a different resident encoding");
        assert_eq!(first.identity, again.identity);
        assert_eq!(first.handle, again.handle);
        assert!(again.deduped);
        assert_eq!(first.identity, node.identity());
        assert_eq!(
            store
                .entry(first.handle)
                .and_then(StoreEntry::resident_bytes),
            Some(&b"canonical"[..])
        );

        // Realized scalars are the same framed leaf path.
        let realized = store.intern_realized(SchemaId::named("vix.demo"), b"canonical");
        assert_eq!(realized.identity, first.identity);
        assert!(realized.deduped);
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
