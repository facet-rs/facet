use crate::db::{DynIngredient, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame;
use crate::key::{Dep, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use crate::runtime::HasRuntime;
use dashmap::DashMap;
use facet::Facet;
use futures_util::future::BoxFuture;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{debug, trace};

/// An identifier returned from [`InternedIngredient::intern`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Facet)]
#[repr(transparent)]
pub struct InternId(pub u32);

/// An ingredient that interns values and returns stable ids.
///
/// Interned values are immutable: interning does **not** bump the database revision.
pub struct InternedIngredient<K> {
    kind: QueryKindId,
    kind_name: &'static str,
    next_id: AtomicU32,
    by_value: DashMap<Key, InternId>,
    by_id: DashMap<InternId, Arc<K>>,
}

impl<K> InternedIngredient<K>
where
    K: Facet<'static> + Send + Sync + 'static,
{
    /// Create an empty interned ingredient.
    pub fn new(kind: QueryKindId, kind_name: &'static str) -> Self {
        Self {
            kind,
            kind_name,
            next_id: AtomicU32::new(0),
            by_value: DashMap::new(),
            by_id: DashMap::new(),
        }
    }

    /// The stable kind id.
    pub fn kind(&self) -> QueryKindId {
        self.kind
    }

    /// Debug name for this ingredient.
    pub fn kind_name(&self) -> &'static str {
        self.kind_name
    }

    /// Intern `value` and return its stable id.
    pub fn intern(&self, value: K) -> PicanteResult<InternId> {
        let _span = tracing::debug_span!("intern", kind = self.kind.0).entered();
        let key = Key::encode_facet(&value)?;
        let key_hash = key.hash();

        match self.by_value.entry(key) {
            dashmap::mapref::entry::Entry::Occupied(e) => Ok(*e.get()),
            dashmap::mapref::entry::Entry::Vacant(e) => {
                let id = InternId(self.next_id.fetch_add(1, Ordering::AcqRel));
                self.by_id.insert(id, Arc::new(value));
                e.insert(id);
                debug!(
                    kind = self.kind.0,
                    key_hash = %format!("{:016x}", key_hash),
                    id = id.0,
                    "interned"
                );
                Ok(id)
            }
        }
    }

    /// Look up an interned value by id.
    ///
    /// If there's an active query frame, records a dependency edge.
    pub fn get<DB: HasRuntime>(&self, _db: &DB, id: InternId) -> PicanteResult<Arc<K>> {
        let _span = tracing::trace_span!("get", kind = self.kind.0, id = id.0).entered();
        if frame::has_active_frame() {
            let key = Key::encode_facet(&id)?;
            trace!(
                kind = self.kind.0,
                key_hash = %format!("{:016x}", key.hash()),
                id = id.0,
                "interned dep"
            );
            frame::record_dep(Dep {
                kind: self.kind,
                key,
            });
        }

        self.by_id.get(&id).map(|v| v.clone()).ok_or_else(|| {
            Arc::new(PicanteError::MissingInternedValue {
                kind: self.kind,
                id: id.0,
            })
        })
    }
}

#[derive(Debug, Clone, Facet)]
struct InternedRecord<K> {
    id: u32,
    value: Arc<K>,
}

impl<K> PersistableIngredient for InternedIngredient<K>
where
    K: Facet<'static> + Send + Sync + 'static,
{
    fn kind(&self) -> QueryKindId {
        self.kind
    }

    fn kind_name(&self) -> &'static str {
        self.kind_name
    }

    fn section_type(&self) -> SectionType {
        SectionType::Interned
    }

    fn clear(&self) {
        self.by_value.clear();
        self.by_id.clear();
        self.next_id.store(0, Ordering::Release);
    }

    fn save_records(&self) -> BoxFuture<'_, PicanteResult<Vec<Vec<u8>>>> {
        Box::pin(async move {
            let mut snapshot: Vec<(InternId, Arc<K>)> = self
                .by_id
                .iter()
                .map(|e| (*e.key(), e.value().clone()))
                .collect();
            snapshot.sort_by_key(|(id, _)| id.0);

            let mut records = Vec::with_capacity(snapshot.len());
            for (id, value) in snapshot {
                let rec = InternedRecord::<K> { id: id.0, value };

                let bytes = facet_format_postcard::to_vec(&rec).map_err(|e| {
                    Arc::new(PicanteError::Encode {
                        what: "interned record",
                        message: format!("{e:?}"),
                    })
                })?;
                records.push(bytes);
            }

            debug!(
                kind = self.kind.0,
                records = records.len(),
                "save_records (interned)"
            );
            Ok(records)
        })
    }

    fn load_records(&self, records: Vec<Vec<u8>>) -> PicanteResult<()> {
        self.clear();

        let mut max_id: u32 = 0;

        for bytes in records {
            let rec: InternedRecord<K> =
                facet_format_postcard::from_slice(&bytes).map_err(|e| {
                    Arc::new(PicanteError::Decode {
                        what: "interned record",
                        message: format!("{e:?}"),
                    })
                })?;

            let id = InternId(rec.id);
            max_id = max_id.max(id.0);

            if self.by_id.contains_key(&id) {
                return Err(Arc::new(PicanteError::Cache {
                    message: format!("duplicate interned id {} in `{}`", id.0, self.kind_name),
                }));
            }

            let key = Key::encode_facet(rec.value.as_ref())?;
            if let Some(existing) = self.by_value.insert(key, id) {
                return Err(Arc::new(PicanteError::Cache {
                    message: format!(
                        "duplicate interned value for `{}` (ids {} and {})",
                        self.kind_name, existing.0, id.0
                    ),
                }));
            }

            self.by_id.insert(id, rec.value);
        }

        self.next_id
            .store(max_id.saturating_add(1), Ordering::Release);
        Ok(())
    }

    fn save_incremental_records(
        &self,
        _since_revision: u64,
    ) -> BoxFuture<'_, PicanteResult<Vec<(u64, Vec<u8>, Option<Vec<u8>>)>>> {
        Box::pin(async move {
            // For interned ingredients, we don't track revisions per entry.
            // Since interned values are immutable (never modified or deleted),
            // we can't efficiently determine which entries are "new" without
            // additional tracking.
            //
            // The simplest approach is to not support incremental persistence
            // for interned ingredients - they should be included in the base
            // snapshot only. This is reasonable because:
            // 1. Interned values are typically small and don't change often
            // 2. The full set of interned values is usually not that large
            // 3. Interning doesn't bump revisions, so WAL would be noisy
            //
            // Alternative: We could add a `created_at_revision` field to track
            // when each ID was first interned, but that adds overhead for
            // minimal benefit.

            debug!(
                kind = self.kind.0,
                "save_incremental_records (interned): not supported, use full snapshot"
            );

            // Return empty - no incremental changes
            Ok(vec![])
        })
    }

    fn apply_wal_entry(
        &self,
        _revision: u64,
        _key: Vec<u8>,
        value: Option<Vec<u8>>,
    ) -> PicanteResult<()> {
        // Even though we don't generate incremental records for interned ingredients,
        // we should still be able to apply them if they somehow exist in the WAL
        // (e.g., from a future version that does track them).

        if let Some(value_bytes) = value {
            let rec: InternedRecord<K> =
                facet_format_postcard::from_slice(&value_bytes).map_err(|e| {
                    Arc::new(PicanteError::Decode {
                        what: "interned record from WAL",
                        message: format!("{e:?}"),
                    })
                })?;

            let id = InternId(rec.id);
            let key = Key::encode_facet(rec.value.as_ref())?;

            // Insert into both maps
            self.by_id.insert(id, rec.value);
            self.by_value.insert(key, id);

            // Update next_id if necessary
            let current_next = self.next_id.load(Ordering::Acquire);
            if id.0 >= current_next {
                self.next_id
                    .store(id.0.saturating_add(1), Ordering::Release);
            }
        }
        // Note: We ignore deletions since interned values are never deleted

        Ok(())
    }
}

impl<DB, K> DynIngredient<DB> for InternedIngredient<K>
where
    DB: HasRuntime + Send + Sync + 'static,
    K: Facet<'static> + Send + Sync + 'static,
{
    fn touch<'a>(&'a self, _db: &'a DB, key: Key) -> BoxFuture<'a, PicanteResult<Touch>> {
        Box::pin(async move {
            let id: InternId = key.decode_facet()?;
            if !self.by_id.contains_key(&id) {
                return Err(Arc::new(PicanteError::MissingInternedValue {
                    kind: self.kind,
                    id: id.0,
                }));
            }
            Ok(Touch {
                changed_at: Revision(0),
            })
        })
    }
}
