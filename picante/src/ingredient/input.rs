use crate::db::{DynIngredient, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame;
use crate::key::{Dep, DynKey, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use crate::runtime::HasRuntime;
use facet::Facet;
use futures_util::future::BoxFuture;
use parking_lot::RwLock;
use std::any::Any;
use std::hash::Hash;
use std::sync::Arc;
use tracing::{debug, trace};

// ============================================================================
// Type-erased storage and callbacks for InputIngredient
// ============================================================================

/// Type-erased value storage
type ArcAny = Arc<dyn Any + Send + Sync>;

/// Type-erased input entry
#[derive(Clone)]
struct ErasedInputEntry {
    value: Option<ArcAny>,
    changed_at: Revision,
}

/// Encode a single record to bytes
type EncodeInputRecordFn =
    fn(dyn_key: &DynKey, value: Option<&ArcAny>, changed_at: Revision) -> PicanteResult<Vec<u8>>;

/// Decode a single record from bytes
/// Takes owned Vec<u8> because facet_postcard::from_slice requires 'static
type DecodeInputRecordFn =
    fn(kind: QueryKindId, bytes: Vec<u8>) -> PicanteResult<(DynKey, ErasedInputEntry)>;

/// Encode key and optional value for incremental save
type EncodeInputIncrementalFn =
    fn(dyn_key: &DynKey, value: Option<&ArcAny>) -> PicanteResult<(Vec<u8>, Option<Vec<u8>>)>;

/// Apply a WAL entry (insert or delete)
/// Takes owned bytes because facet_postcard::from_slice requires 'static
type ApplyInputWalEntryFn = fn(
    kind: QueryKindId,
    key_bytes: Vec<u8>,
    value_bytes: Option<Vec<u8>>,
) -> PicanteResult<(DynKey, ErasedInputEntry)>;

// ============================================================================
// Non-generic core: persistence logic compiled ONCE
// ============================================================================

/// Non-generic core containing type-erased storage and persistence logic.
struct InputCore {
    kind: QueryKindId,
    kind_name: &'static str,
    entries: RwLock<im::HashMap<DynKey, ErasedInputEntry>>,
    // Type-erased persistence callbacks
    encode_record: EncodeInputRecordFn,
    decode_record: DecodeInputRecordFn,
    encode_incremental: EncodeInputIncrementalFn,
    apply_wal_entry: ApplyInputWalEntryFn,
}

impl InputCore {
    fn new(
        kind: QueryKindId,
        kind_name: &'static str,
        encode_record: EncodeInputRecordFn,
        decode_record: DecodeInputRecordFn,
        encode_incremental: EncodeInputIncrementalFn,
        apply_wal_entry: ApplyInputWalEntryFn,
    ) -> Self {
        Self {
            kind,
            kind_name,
            entries: RwLock::new(im::HashMap::new()),
            encode_record,
            decode_record,
            encode_incremental,
            apply_wal_entry,
        }
    }

    // ========================================================================
    // Type-erased persistence methods (compiled ONCE)
    // ========================================================================

    fn save_records_erased(&self) -> PicanteResult<Vec<Vec<u8>>> {
        let entries = self.entries.read();
        let mut records = Vec::with_capacity(entries.len());
        for (dyn_key, entry) in entries.iter() {
            let bytes = (self.encode_record)(dyn_key, entry.value.as_ref(), entry.changed_at)?;
            records.push(bytes);
        }
        debug!(
            kind = self.kind.0,
            records = records.len(),
            "save_records (input, erased)"
        );
        Ok(records)
    }

    fn load_records_erased(&self, records: Vec<Vec<u8>>) -> PicanteResult<()> {
        let mut entries = self.entries.write();
        for bytes in records {
            // Pass owned Vec<u8> (callback needs 'static for deserialization)
            let (dyn_key, entry) = (self.decode_record)(self.kind, bytes)?;
            entries.insert(dyn_key, entry);
        }
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn save_incremental_records_erased(
        &self,
        since_revision: u64,
    ) -> PicanteResult<Vec<(u64, Vec<u8>, Option<Vec<u8>>)>> {
        let entries = self.entries.read();
        let mut changes = Vec::new();

        for (dyn_key, entry) in entries.iter() {
            if entry.changed_at.0 > since_revision {
                let (key_bytes, value_bytes) =
                    (self.encode_incremental)(dyn_key, entry.value.as_ref())?;
                changes.push((entry.changed_at.0, key_bytes, value_bytes));
            }
        }

        debug!(
            kind = self.kind.0,
            changes = changes.len(),
            since_revision,
            "save_incremental_records (input, erased)"
        );

        Ok(changes)
    }

    fn apply_wal_entry_erased(
        &self,
        revision: u64,
        key_bytes: Vec<u8>,
        value_bytes: Option<Vec<u8>>,
    ) -> PicanteResult<()> {
        // Pass owned bytes (callback needs 'static for deserialization)
        let (dyn_key, mut entry) = (self.apply_wal_entry)(self.kind, key_bytes, value_bytes)?;
        // Override with the exact revision from WAL
        entry.changed_at = Revision(revision);

        let mut entries = self.entries.write();
        entries.insert(dyn_key, entry);

        Ok(())
    }
}

// ============================================================================
// Helper functions to create persistence callbacks (monomorphized per K,V)
// ============================================================================

fn make_encode_input_record<K, V>() -> EncodeInputRecordFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |dyn_key, value, changed_at| {
        let key: K = dyn_key.key.decode_facet()?;
        let typed_value: Option<V> = match value {
            Some(arc_any) => Some(
                arc_any
                    .downcast_ref::<V>()
                    .ok_or_else(|| {
                        Arc::new(PicanteError::Panic {
                            message: format!(
                                "[BUG] type mismatch in input save_records: expected {}",
                                std::any::type_name::<V>()
                            ),
                        })
                    })?
                    .clone(),
            ),
            None => None,
        };

        let rec = InputRecord::<K, V> {
            key,
            value: typed_value,
            changed_at: changed_at.0,
        };

        facet_postcard::to_vec(&rec).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "input record",
                message: format!("{e:?}"),
            })
        })
    }
}

fn make_decode_input_record<K, V>() -> DecodeInputRecordFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |kind, bytes| {
        let rec: InputRecord<K, V> = facet_postcard::from_slice(&bytes).map_err(|e| {
            Arc::new(PicanteError::Decode {
                what: "input record",
                message: format!("{e:?}"),
            })
        })?;

        let dyn_key = DynKey {
            kind,
            key: Key::encode_facet(&rec.key)?,
        };

        let value: Option<ArcAny> = rec.value.map(|v| Arc::new(v) as ArcAny);

        Ok((
            dyn_key,
            ErasedInputEntry {
                value,
                changed_at: Revision(rec.changed_at),
            },
        ))
    }
}

fn make_encode_input_incremental<K, V>() -> EncodeInputIncrementalFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |dyn_key, value| {
        let key: K = dyn_key.key.decode_facet()?;

        let key_bytes = facet_postcard::to_vec(&key).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "input key",
                message: format!("{e:?}"),
            })
        })?;

        let value_bytes = match value {
            Some(arc_any) => {
                let typed_value: &V = arc_any.downcast_ref::<V>().ok_or_else(|| {
                    Arc::new(PicanteError::Panic {
                        message: format!(
                            "[BUG] type mismatch in input save_incremental: expected {}",
                            std::any::type_name::<V>()
                        ),
                    })
                })?;
                Some(facet_postcard::to_vec(typed_value).map_err(|e| {
                    Arc::new(PicanteError::Encode {
                        what: "input value",
                        message: format!("{e:?}"),
                    })
                })?)
            }
            None => None,
        };

        Ok((key_bytes, value_bytes))
    }
}

fn make_apply_input_wal_entry<K, V>() -> ApplyInputWalEntryFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |kind, key_bytes, value_bytes| {
        let key: K = facet_postcard::from_slice(&key_bytes).map_err(|e| {
            Arc::new(PicanteError::Decode {
                what: "input key from WAL",
                message: format!("{e:?}"),
            })
        })?;

        let value: Option<ArcAny> = match value_bytes {
            Some(bytes) => {
                let v: V = facet_postcard::from_slice(&bytes).map_err(|e| {
                    Arc::new(PicanteError::Decode {
                        what: "input value from WAL",
                        message: format!("{e:?}"),
                    })
                })?;
                Some(Arc::new(v) as ArcAny)
            }
            None => None,
        };

        let dyn_key = DynKey {
            kind,
            key: Key::encode_facet(&key)?,
        };

        Ok((
            dyn_key,
            ErasedInputEntry {
                value,
                changed_at: Revision(0), // Will be overridden by caller
            },
        ))
    }
}

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

// r[input.type]
/// A key-value input ingredient.
///
/// Reads record dependencies into the current query frame (if one exists).
/// Uses `im::HashMap` with `RwLock` internally. This enables O(1) snapshot cloning
/// via structural sharing, at the cost of requiring explicit locking (compared to
/// lock-free `DashMap`). The trade-off favors snapshot efficiency for database
/// state capture and time-travel debugging scenarios.
///
/// Storage is type-erased internally (using `DynKey` and `Arc<dyn Any>`) to enable
/// persistence code to be compiled once instead of per (K, V) combination.
pub struct InputIngredient<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    /// Type-erased core with persistence logic
    core: InputCore,
    /// Phantom data for K and V types
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> InputIngredient<K, V>
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    /// Create an empty input ingredient.
    pub fn new(kind: QueryKindId, kind_name: &'static str) -> Self {
        Self {
            core: InputCore::new(
                kind,
                kind_name,
                make_encode_input_record::<K, V>(),
                make_decode_input_record::<K, V>(),
                make_encode_input_incremental::<K, V>(),
                make_apply_input_wal_entry::<K, V>(),
            ),
            _phantom: std::marker::PhantomData,
        }
    }

    /// The stable kind id.
    pub fn kind(&self) -> QueryKindId {
        self.core.kind
    }

    /// Debug name for this ingredient.
    pub fn kind_name(&self) -> &'static str {
        self.core.kind_name
    }

    // r[input.set]
    // r[input.revision-on-change]
    /// Set an input value.
    ///
    /// Bumps the runtime revision only if the value actually changed.
    pub fn set<DB: HasRuntime>(&self, db: &DB, key: K, value: V) -> Revision {
        let _span = tracing::debug_span!("set", kind = self.core.kind.0).entered();

        // Encode key for storage
        let dyn_key = match Key::encode_facet(&key) {
            Ok(encoded) => DynKey {
                kind: self.core.kind,
                key: encoded,
            },
            Err(_) => return Revision(0), // Can't encode key, no-op
        };

        // Check if value is unchanged (read lock)
        {
            let entries = self.core.entries.read();
            if let Some(existing) = entries.get(&dyn_key)
                && let Some(existing_value) = existing.value.as_ref()
                && let Some(typed_existing) = existing_value.downcast_ref::<V>()
                && crate::facet_eq::facet_eq_direct(typed_existing, &value)
            {
                trace!(
                    kind = self.core.kind.0,
                    changed_at = existing.changed_at.0,
                    "input set no-op (same value)"
                );
                return existing.changed_at;
            }
        }

        // Value changed, take write lock
        let rev = db.runtime().bump_revision();
        {
            let mut entries = self.core.entries.write();
            entries.insert(
                dyn_key.clone(),
                ErasedInputEntry {
                    value: Some(Arc::new(value) as ArcAny),
                    changed_at: rev,
                },
            );
        }
        db.runtime()
            .notify_input_set(rev, self.core.kind, dyn_key.key);
        rev
    }

    // r[input.remove]
    /// Remove an input value.
    ///
    /// Bumps the runtime revision only if the value existed.
    pub fn remove<DB: HasRuntime>(&self, db: &DB, key: &K) -> Revision {
        let _span = tracing::debug_span!("remove", kind = self.core.kind.0).entered();

        // Encode key for storage
        let dyn_key = match Key::encode_facet(key) {
            Ok(encoded) => DynKey {
                kind: self.core.kind,
                key: encoded,
            },
            Err(_) => return Revision(0), // Can't encode key, no-op
        };

        // Check current state (read lock)
        {
            let entries = self.core.entries.read();
            match entries.get(&dyn_key) {
                Some(existing) if existing.value.is_none() => {
                    trace!(
                        kind = self.core.kind.0,
                        changed_at = existing.changed_at.0,
                        "input remove no-op (already removed)"
                    );
                    return existing.changed_at;
                }
                None => {
                    trace!(kind = self.core.kind.0, "input remove no-op (missing)");
                    return Revision(0);
                }
                _ => {}
            }
        }

        // Need to remove, take write lock
        let rev = db.runtime().bump_revision();
        {
            let mut entries = self.core.entries.write();
            entries.insert(
                dyn_key.clone(),
                ErasedInputEntry {
                    value: None,
                    changed_at: rev,
                },
            );
        }
        db.runtime()
            .notify_input_removed(rev, self.core.kind, dyn_key.key);
        rev
    }

    // r[input.get]
    /// Read an input value.
    ///
    /// If there's an active query frame, records a dependency edge.
    pub fn get<DB: HasRuntime>(&self, _db: &DB, key: &K) -> PicanteResult<Option<V>> {
        let _span = tracing::trace_span!("get", kind = self.core.kind.0).entered();

        let encoded_key = Key::encode_facet(key)?;
        let dyn_key = DynKey {
            kind: self.core.kind,
            key: encoded_key.clone(),
        };

        if frame::has_active_frame() {
            trace!(kind = self.core.kind.0, key_hash = %format!("{:016x}", encoded_key.hash()), "input dep");
            frame::record_dep(Dep {
                kind: self.core.kind,
                key: encoded_key,
            });
        }

        let entries = self.core.entries.read();
        Ok(entries.get(&dyn_key).and_then(|e| {
            e.value
                .as_ref()
                .and_then(|v| v.downcast_ref::<V>())
                .cloned()
        }))
    }

    /// The last revision at which this input was changed.
    pub fn changed_at(&self, key: &K) -> Option<Revision> {
        let dyn_key = DynKey {
            kind: self.core.kind,
            key: Key::encode_facet(key).ok()?,
        };
        let entries = self.core.entries.read();
        entries.get(&dyn_key).map(|e| e.changed_at)
    }

    // r[snapshot.input]
    /// Create a snapshot of this ingredient's data.
    ///
    /// This is an O(1) operation due to structural sharing in `im::HashMap`.
    /// The returned map is immutable and can be used for consistent reads
    /// while the live ingredient continues to be modified.
    pub fn snapshot(&self) -> im::HashMap<K, InputEntry<V>> {
        let entries = self.core.entries.read();
        entries
            .iter()
            .filter_map(|(dyn_key, entry)| {
                let key: K = dyn_key.key.decode_facet().ok()?;
                let value = entry
                    .value
                    .as_ref()
                    .and_then(|v| v.downcast_ref::<V>())
                    .cloned();
                Some((
                    key,
                    InputEntry {
                        value,
                        changed_at: entry.changed_at,
                    },
                ))
            })
            .collect()
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
        let core = InputCore::new(
            kind,
            kind_name,
            make_encode_input_record::<K, V>(),
            make_decode_input_record::<K, V>(),
            make_encode_input_incremental::<K, V>(),
            make_apply_input_wal_entry::<K, V>(),
        );

        // Convert typed entries to type-erased storage
        {
            let mut erased_entries = core.entries.write();
            for (key, entry) in entries {
                if let Ok(encoded_key) = Key::encode_facet(&key) {
                    let dyn_key = DynKey {
                        kind,
                        key: encoded_key,
                    };
                    let erased_value = entry.value.map(|v| Arc::new(v) as ArcAny);
                    erased_entries.insert(
                        dyn_key,
                        ErasedInputEntry {
                            value: erased_value,
                            changed_at: entry.changed_at,
                        },
                    );
                }
            }
        }

        Self {
            core,
            _phantom: std::marker::PhantomData,
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
        self.core.kind
    }

    fn kind_name(&self) -> &'static str {
        self.core.kind_name
    }

    fn section_type(&self) -> SectionType {
        SectionType::Input
    }

    fn clear(&self) {
        let mut entries = self.core.entries.write();
        *entries = im::HashMap::new();
    }

    fn save_records(&self) -> BoxFuture<'_, PicanteResult<Vec<Vec<u8>>>> {
        // Delegate to type-erased core (compiled once)
        Box::pin(async move { self.core.save_records_erased() })
    }

    fn load_records(&self, records: Vec<Vec<u8>>) -> PicanteResult<()> {
        // Delegate to type-erased core (compiled once)
        self.core.load_records_erased(records)
    }

    fn save_incremental_records(
        &self,
        since_revision: u64,
    ) -> BoxFuture<'_, PicanteResult<Vec<(u64, Vec<u8>, Option<Vec<u8>>)>>> {
        // Delegate to type-erased core (compiled once)
        Box::pin(async move { self.core.save_incremental_records_erased(since_revision) })
    }

    fn apply_wal_entry(
        &self,
        revision: u64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
    ) -> PicanteResult<()> {
        // Delegate to type-erased core (compiled once)
        self.core.apply_wal_entry_erased(revision, key, value)
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
            let dyn_key = DynKey {
                kind: self.core.kind,
                key,
            };
            let entries = self.core.entries.read();
            let changed_at = entries
                .get(&dyn_key)
                .map(|e| e.changed_at)
                .unwrap_or(Revision(0));
            Ok(Touch { changed_at })
        })
    }
}
