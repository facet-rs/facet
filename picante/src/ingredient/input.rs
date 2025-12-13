use crate::db::{DynIngredient, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame;
use crate::key::{Dep, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use crate::runtime::HasRuntime;
use dashmap::DashMap;
use facet::Facet;
use facet_assert::check_same_report;
use futures::future::BoxFuture;
use std::hash::Hash;
use std::sync::Arc;
use tracing::{debug, trace};

struct InputEntry<V> {
    value: Option<V>,
    changed_at: Revision,
}

/// A key-value input ingredient.
///
/// Reads record dependencies into the current query frame (if one exists).
pub struct InputIngredient<K, V> {
    kind: QueryKindId,
    kind_name: &'static str,
    entries: DashMap<K, InputEntry<V>>,
}

impl<K, V> InputIngredient<K, V>
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    /// Create an empty input ingredient.
    pub fn new(kind: QueryKindId, kind_name: &'static str) -> Self {
        Self {
            kind,
            kind_name,
            entries: DashMap::new(),
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

    /// Set an input value.
    ///
    /// Bumps the runtime revision only if the value actually changed.
    #[tracing::instrument(level = "debug", skip_all, fields(kind = self.kind.0))]
    pub fn set<DB: HasRuntime>(&self, db: &DB, key: K, value: V) -> Revision {
        if let Some(existing) = self.entries.get(&key)
            && let Some(existing_value) = existing.value.as_ref()
            && check_same_report(existing_value, &value).is_same()
        {
            trace!(
                kind = self.kind.0,
                changed_at = existing.changed_at.0,
                "input set no-op (same value)"
            );
            return existing.changed_at;
        }

        let encoded_key = Key::encode_facet(&key).ok();
        let rev = db.runtime().bump_revision();
        self.entries.insert(
            key,
            InputEntry {
                value: Some(value),
                changed_at: rev,
            },
        );
        if let Some(encoded_key) = encoded_key {
            db.runtime().notify_input_set(rev, self.kind, encoded_key);
        }
        rev
    }

    /// Remove an input value.
    ///
    /// Bumps the runtime revision only if the value existed.
    #[tracing::instrument(level = "debug", skip_all, fields(kind = self.kind.0))]
    pub fn remove<DB: HasRuntime>(&self, db: &DB, key: &K) -> Revision {
        match self.entries.get(key) {
            Some(existing) if existing.value.is_none() => {
                trace!(
                    kind = self.kind.0,
                    changed_at = existing.changed_at.0,
                    "input remove no-op (already removed)"
                );
                return existing.changed_at;
            }
            None => {
                trace!(kind = self.kind.0, "input remove no-op (missing)");
                return Revision(0);
            }
            _ => {}
        }

        let encoded_key = Key::encode_facet(key).ok();
        let rev = db.runtime().bump_revision();
        self.entries.insert(
            key.clone(),
            InputEntry {
                value: None,
                changed_at: rev,
            },
        );
        if let Some(encoded_key) = encoded_key {
            db.runtime()
                .notify_input_removed(rev, self.kind, encoded_key);
        }
        rev
    }

    /// Read an input value.
    ///
    /// If there's an active query frame, records a dependency edge.
    #[tracing::instrument(level = "trace", skip_all, fields(kind = self.kind.0))]
    pub fn get<DB: HasRuntime>(&self, _db: &DB, key: &K) -> PicanteResult<Option<V>> {
        if frame::has_active_frame() {
            let key = Key::encode_facet(key)?;
            trace!(kind = self.kind.0, key_hash = %format!("{:016x}", key.hash()), "input dep");
            frame::record_dep(Dep {
                kind: self.kind,
                key,
            });
        }

        Ok(self.entries.get(key).and_then(|e| e.value.clone()))
    }

    /// The last revision at which this input was changed.
    pub fn changed_at(&self, key: &K) -> Option<Revision> {
        self.entries.get(key).map(|e| e.changed_at)
    }
}

#[derive(Debug, Clone, Facet)]
struct InputRecord<K, V> {
    key: K,
    value: Option<V>,
    changed_at: u64,
}

impl<K, V> PersistableIngredient for InputIngredient<K, V>
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    fn kind(&self) -> QueryKindId {
        self.kind
    }

    fn kind_name(&self) -> &'static str {
        self.kind_name
    }

    fn section_type(&self) -> SectionType {
        SectionType::Input
    }

    fn clear(&self) {
        self.entries.clear();
    }

    fn save_records(&self) -> BoxFuture<'_, PicanteResult<Vec<Vec<u8>>>> {
        Box::pin(async move {
            let mut records = Vec::with_capacity(self.entries.len());
            for entry in self.entries.iter() {
                let rec = InputRecord::<K, V> {
                    key: entry.key().clone(),
                    value: entry.value().value.clone(),
                    changed_at: entry.value().changed_at.0,
                };
                let bytes = facet_postcard::to_vec(&rec).map_err(|e| {
                    Arc::new(PicanteError::Encode {
                        what: "input record",
                        message: format!("{e:?}"),
                    })
                })?;
                records.push(bytes);
            }
            debug!(
                kind = self.kind.0,
                records = records.len(),
                "save_records (input)"
            );
            Ok(records)
        })
    }

    fn load_records(&self, records: Vec<Vec<u8>>) -> PicanteResult<()> {
        for bytes in records {
            let rec: InputRecord<K, V> = facet_postcard::from_slice(&bytes).map_err(|e| {
                Arc::new(PicanteError::Decode {
                    what: "input record",
                    message: format!("{e:?}"),
                })
            })?;
            self.entries.insert(
                rec.key,
                InputEntry {
                    value: rec.value,
                    changed_at: Revision(rec.changed_at),
                },
            );
        }
        Ok(())
    }
}

impl<DB, K, V> DynIngredient<DB> for InputIngredient<K, V>
where
    DB: HasRuntime + Send + Sync + 'static,
    K: Clone + Eq + Hash + facet::Facet<'static> + Send + Sync + 'static,
    V: Clone + facet::Facet<'static> + Send + Sync + 'static,
{
    fn touch<'a>(&'a self, _db: &'a DB, key: Key) -> BoxFuture<'a, PicanteResult<Touch>> {
        Box::pin(async move {
            let key: K = key.decode_facet()?;
            let changed_at = self
                .entries
                .get(&key)
                .map(|e| e.changed_at)
                .unwrap_or(Revision(0));
            Ok(Touch { changed_at })
        })
    }
}
