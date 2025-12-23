use crate::db::{DynIngredient, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame;
use crate::key::{Dep, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use crate::runtime::HasRuntime;
use facet::Facet;
use futures::future::BoxFuture;
use parking_lot::RwLock;
use std::hash::Hash;
use std::sync::Arc;
use tracing::{debug, trace};

/// Entry in an input ingredient, containing the value and its change revision.
///
/// Note: Trait bounds (`Facet<'static>`, `Send`, `Sync`) are enforced on the
/// `InputIngredient` impl blocks where `InputEntry<V>` is used, not on this
/// struct definition. This follows Rust best practice of placing bounds on impls
/// rather than type definitions.
#[derive(Clone)]
pub struct InputEntry<V> {
    /// The value, or None if removed.
    pub value: Option<V>,
    /// The revision at which this entry was last changed.
    pub changed_at: Revision,
}

/// A key-value input ingredient.
///
/// Reads record dependencies into the current query frame (if one exists).
/// Uses `im::HashMap` with `RwLock` internally. This enables O(1) snapshot cloning
/// via structural sharing, at the cost of requiring explicit locking (compared to
/// lock-free `DashMap`). The trade-off favors snapshot efficiency for database
/// state capture and time-travel debugging scenarios.
pub struct InputIngredient<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    kind: QueryKindId,
    kind_name: &'static str,
    entries: RwLock<im::HashMap<K, InputEntry<V>>>,
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
            entries: RwLock::new(im::HashMap::new()),
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
        // Check if value is unchanged (read lock)
        {
            let entries = self.entries.read();
            if let Some(existing) = entries.get(&key)
                && let Some(existing_value) = existing.value.as_ref()
                && crate::facet_eq::facet_eq_direct(existing_value, &value)
            {
                trace!(
                    kind = self.kind.0,
                    changed_at = existing.changed_at.0,
                    "input set no-op (same value)"
                );
                return existing.changed_at;
            }
        }

        // Value changed, take write lock
        let encoded_key = Key::encode_facet(&key).ok();
        let rev = db.runtime().bump_revision();
        {
            let mut entries = self.entries.write();
            entries.insert(
                key,
                InputEntry {
                    value: Some(value),
                    changed_at: rev,
                },
            );
        }
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
        // Check current state (read lock)
        {
            let entries = self.entries.read();
            match entries.get(key) {
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
        }

        // Need to remove, take write lock
        let encoded_key = Key::encode_facet(key).ok();
        let rev = db.runtime().bump_revision();
        {
            let mut entries = self.entries.write();
            entries.insert(
                key.clone(),
                InputEntry {
                    value: None,
                    changed_at: rev,
                },
            );
        }
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
            let encoded_key = Key::encode_facet(key)?;
            trace!(kind = self.kind.0, key_hash = %format!("{:016x}", encoded_key.hash()), "input dep");
            frame::record_dep(Dep {
                kind: self.kind,
                key: encoded_key,
            });
        }

        let entries = self.entries.read();
        Ok(entries.get(key).and_then(|e| e.value.clone()))
    }

    /// The last revision at which this input was changed.
    pub fn changed_at(&self, key: &K) -> Option<Revision> {
        let entries = self.entries.read();
        entries.get(key).map(|e| e.changed_at)
    }

    /// Create a snapshot of this ingredient's data.
    ///
    /// This is an O(1) operation due to structural sharing in `im::HashMap`.
    /// The returned map is immutable and can be used for consistent reads
    /// while the live ingredient continues to be modified.
    pub fn snapshot(&self) -> im::HashMap<K, InputEntry<V>> {
        self.entries.read().clone()
    }

    /// Create a new ingredient initialized from a snapshot.
    ///
    /// This is used when creating database snapshots. The returned ingredient
    /// contains the same data as the snapshot but is independent of the original.
    pub fn new_from_snapshot(
        kind: QueryKindId,
        kind_name: &'static str,
        entries: im::HashMap<K, InputEntry<V>>,
    ) -> Self {
        Self {
            kind,
            kind_name,
            entries: RwLock::new(entries),
        }
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
        let mut entries = self.entries.write();
        *entries = im::HashMap::new();
    }

    fn save_records(&self) -> BoxFuture<'_, PicanteResult<Vec<Vec<u8>>>> {
        Box::pin(async move {
            let entries = self.entries.read();
            let mut records = Vec::with_capacity(entries.len());
            for (key, entry) in entries.iter() {
                let rec = InputRecord::<K, V> {
                    key: key.clone(),
                    value: entry.value.clone(),
                    changed_at: entry.changed_at.0,
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
        let mut entries = self.entries.write();
        for bytes in records {
            let rec: InputRecord<K, V> = facet_postcard::from_slice(&bytes).map_err(|e| {
                Arc::new(PicanteError::Decode {
                    what: "input record",
                    message: format!("{e:?}"),
                })
            })?;
            entries.insert(
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
            let entries = self.entries.read();
            let changed_at = entries
                .get(&key)
                .map(|e| e.changed_at)
                .unwrap_or(Revision(0));
            Ok(Touch { changed_at })
        })
    }
}
