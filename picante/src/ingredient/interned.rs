use crate::db::{DynIngredient, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame;
use crate::key::{Dep, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use crate::runtime::HasRuntime;
use dashmap::DashMap;
use facet::Facet;
use futures::future::BoxFuture;
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
    #[tracing::instrument(level = "debug", skip_all, fields(kind = self.kind.0))]
    pub fn intern(&self, value: K) -> PicanteResult<InternId> {
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
    #[tracing::instrument(level = "trace", skip_all, fields(kind = self.kind.0, id = id.0))]
    pub fn get<DB: HasRuntime>(&self, _db: &DB, id: InternId) -> PicanteResult<Arc<K>> {
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

                let bytes = facet_postcard::to_vec(&rec).map_err(|e| {
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
            let rec: InternedRecord<K> = facet_postcard::from_slice(&bytes).map_err(|e| {
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
