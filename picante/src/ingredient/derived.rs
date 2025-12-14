use crate::db::{DynIngredient, IngredientLookup, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame::{self, ActiveFrameHandle};
use crate::key::{Dep, DynKey, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use facet::Facet;
use futures::FutureExt;
use futures::future::BoxFuture;
use parking_lot::RwLock;
use std::any::Any;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, trace};

type ComputeFuture<'db, V> = BoxFuture<'db, PicanteResult<V>>;
type ComputeFn<DB, K, V> = dyn for<'db> Fn(&'db DB, K) -> ComputeFuture<'db, V> + Send + Sync;

// ============================================================================
// Type-erased compute infrastructure (for dyn dispatch)
// ============================================================================

/// Type-erased Arc<dyn Any> for storing values without knowing V
type ArcAny = Arc<dyn Any + Send + Sync>;

/// Type-erased compute future that returns ArcAny
type ComputeFut<'a> = BoxFuture<'a, PicanteResult<ArcAny>>;

/// Function pointer for deep equality check without knowing V
type EqErasedFn = fn(&dyn Any, &dyn Any) -> bool;

/// Trait for type-erased compute function (dyn dispatch)
///
/// This trait allows the state machine to call compute() without being generic
/// over the closure/future type F. Each query implements this via TypedCompute<DB,K,V>.
trait ErasedCompute<DB>: Send + Sync {
    /// Compute the value for a given key, returning type-erased result
    fn compute<'a>(&'a self, db: &'a DB, key: Key) -> ComputeFut<'a>;
}

/// Typed adapter that implements ErasedCompute for a specific (DB, K, V)
///
/// This is the small per-query wrapper that boxes the future and erases types.
/// The state machine in DerivedCore stays monomorphic by calling through the trait.
struct TypedCompute<DB, K, V> {
    f: Arc<ComputeFn<DB, K, V>>,
    _phantom: PhantomData<(K, V)>,
}

impl<DB, K, V> ErasedCompute<DB> for TypedCompute<DB, K, V>
where
    DB: IngredientLookup + Send + Sync + 'static,
    K: Facet<'static> + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    fn compute<'a>(&'a self, db: &'a DB, key: Key) -> ComputeFut<'a> {
        Box::pin(async move {
            let k: K = key.decode_facet()?;
            let v: V = (self.f)(db, k).await?;
            Ok(Arc::new(v) as ArcAny)
        })
    }
}

/// Deep equality helper for type-erased values
///
/// Uses facet_assert::check_same to perform deep equality comparison on values
/// without the state machine needing to know the concrete type V.
fn eq_erased_for<V>(a: &dyn Any, b: &dyn Any) -> bool
where
    V: Facet<'static> + 'static,
{
    let Some(a) = a.downcast_ref::<V>() else {
        return false;
    };
    let Some(b) = b.downcast_ref::<V>() else {
        return false;
    };

    matches!(facet_assert::check_same(a, b), facet_assert::Sameness::Same)
}

// ============================================================================
// Non-generic core: state machine compiled ONCE
// ============================================================================

/// Non-generic core containing the type-erased state machine.
///
/// By keeping this struct non-generic and making its methods generic over parameters,
/// we compile the 300+ line state machine ONCE instead of per-(DB,K,V) combination.
struct DerivedCore {
    kind: QueryKindId,
    kind_name: &'static str,
    cells: RwLock<im::HashMap<DynKey, Arc<ErasedCell>>>,
}

impl DerivedCore {
    fn new(kind: QueryKindId, kind_name: &'static str) -> Self {
        Self {
            kind,
            kind_name,
            cells: RwLock::new(im::HashMap::new()),
        }
    }

    /// Type-erased state machine implementation (compiled ONCE per DB type).
    ///
    /// This method uses trait objects (dyn ErasedCompute) instead of generic closures,
    /// so it compiles once per DB type instead of per-(DB,K,V) combination.
    ///
    /// Runtime cost: one vtable call + one BoxFuture allocation per compute.
    /// Compile-time win: 50+ copies reduced to ~2 copies (DB + DatabaseSnapshot).
    async fn access_scoped_erased<DB>(
        &self,
        db: &DB,
        requested: DynKey,
        want_value: bool,
        compute: &dyn ErasedCompute<DB>,
        eq_erased: EqErasedFn,
    ) -> PicanteResult<ErasedAccessResult>
    where
        DB: IngredientLookup + Send + Sync + 'static,
    {
        let key_hash = requested.key.hash();

        if let Some(stack) = frame::find_cycle(&requested) {
            return Err(Arc::new(PicanteError::Cycle {
                requested: requested.clone(),
                stack,
            }));
        }

        // 0) record dependency into parent frame (if any)
        if want_value && frame::has_active_frame() {
            trace!(
                kind = self.kind.0,
                key_hash = %format!("{:016x}", key_hash),
                "derived dep"
            );
            frame::record_dep(Dep {
                kind: self.kind,
                key: requested.key.clone(),
            });
        }

        // Get or create the cell for this key
        let cell = {
            // Fast path: read lock
            if let Some(cell) = self.cells.read().get(&requested) {
                cell.clone()
            } else {
                // Slow path: write lock, double-check after acquiring lock
                let mut cells = self.cells.write();
                if let Some(cell) = cells.get(&requested) {
                    cell.clone()
                } else {
                    let cell = Arc::new(ErasedCell::new());
                    cells.insert(requested.clone(), cell.clone());
                    cell
                }
            }
        };

        loop {
            let rev = db.runtime().current_revision();

            // 1) fast path: read current state
            enum ErasedObserved {
                Ready {
                    value: Option<Arc<dyn std::any::Any + Send + Sync>>,
                    changed_at: Revision,
                },
                Error(Arc<PicanteError>),
                Running,
                StaleReady {
                    deps: Arc<[Dep]>,
                    changed_at: Revision,
                },
                StaleOther,
            }

            let observed = {
                let state = cell.state.lock().await;
                match &*state {
                    ErasedState::Ready {
                        value,
                        verified_at,
                        changed_at,
                        ..
                    } if *verified_at == rev => ErasedObserved::Ready {
                        value: want_value.then(|| value.clone()),
                        changed_at: *changed_at,
                    },
                    ErasedState::Poisoned { error, verified_at } if *verified_at == rev => {
                        ErasedObserved::Error(error.clone())
                    }
                    ErasedState::Running { started_at } => {
                        trace!(
                            kind = self.kind.0,
                            key_hash = %format!("{:016x}", key_hash),
                            started_at = started_at.0,
                            "wait on running cell"
                        );
                        ErasedObserved::Running
                    }
                    ErasedState::Ready {
                        deps, changed_at, ..
                    } => ErasedObserved::StaleReady {
                        deps: deps.clone(),
                        changed_at: *changed_at,
                    },
                    _ => ErasedObserved::StaleOther,
                }
            };

            match observed {
                ErasedObserved::Ready { value, changed_at } => {
                    // Ensure we return a value consistent with *now*.
                    if db.runtime().current_revision() == rev {
                        return Ok(ErasedAccessResult { value, changed_at });
                    }
                    continue;
                }
                ErasedObserved::Error(e) => {
                    if db.runtime().current_revision() == rev {
                        return Err(e);
                    }
                    continue;
                }
                ErasedObserved::Running => {
                    // Running: wait for the owner to finish.
                    cell.notify.notified().await;
                    continue;
                }
                ErasedObserved::StaleReady { deps, changed_at } => {
                    if self
                        .try_revalidate(db, &requested, rev, &deps, changed_at)
                        .await?
                    {
                        let mut state = cell.state.lock().await;
                        match &mut *state {
                            ErasedState::Ready {
                                value,
                                verified_at,
                                changed_at,
                                ..
                            } => {
                                *verified_at = rev;
                                let out_value = want_value.then(|| value.clone());
                                let out_changed_at = *changed_at;
                                drop(state);

                                if db.runtime().current_revision() == rev {
                                    return Ok(ErasedAccessResult {
                                        value: out_value,
                                        changed_at: out_changed_at,
                                    });
                                }
                                continue;
                            }
                            ErasedState::Running { .. } => {
                                // Someone else raced and started recomputing.
                                continue;
                            }
                            _ => continue,
                        }
                    }
                }
                ErasedObserved::StaleOther => {}
            }

            // 2) attempt to start computation
            let (started, prev) = {
                let mut prev: Option<(Arc<dyn std::any::Any + Send + Sync>, Revision)> = None;
                let mut state = cell.state.lock().await;
                match &*state {
                    ErasedState::Ready { verified_at, .. } if *verified_at == rev => (false, None), // raced
                    ErasedState::Poisoned { verified_at, .. } if *verified_at == rev => {
                        (false, None)
                    } // raced
                    ErasedState::Running { .. } => (false, None), // someone else started
                    _ => {
                        let old = std::mem::replace(
                            &mut *state,
                            ErasedState::Running { started_at: rev },
                        );
                        if let ErasedState::Ready {
                            value, changed_at, ..
                        } = old
                        {
                            prev = Some((value, changed_at));
                        }
                        (true, prev)
                    }
                }
            };

            if !started {
                // Either we raced and the value became available, or someone else is running.
                continue;
            }

            // 3) run compute under an active frame
            let frame = ActiveFrameHandle::new(requested.clone(), rev);
            let _guard = frame::push_frame(frame.clone());

            debug!(
                kind = self.kind.0,
                key_hash = %format!("{:016x}", key_hash),
                rev = rev.0,
                "compute: start"
            );

            // Call compute through trait object (dyn dispatch)
            let result = std::panic::AssertUnwindSafe(compute.compute(db, requested.key.clone()))
                .catch_unwind()
                .await;

            let deps: Arc<[Dep]> = frame.take_deps().into();

            // 4) finalize
            match result {
                Ok(Ok(out)) => {
                    let changed_at = match prev {
                        Some((prev_value, prev_changed_at)) => {
                            // Fast path: pointer equality (values are literally the same Arc)
                            // Slow path: deep equality via eq_erased function pointer
                            let is_same = Arc::ptr_eq(&prev_value, &out)
                                || eq_erased(prev_value.as_ref(), out.as_ref());

                            if is_same { prev_changed_at } else { rev }
                        }
                        None => rev,
                    };

                    db.runtime()
                        .update_query_deps(requested.clone(), deps.clone());
                    if changed_at == rev {
                        db.runtime().notify_query_changed(rev, requested.clone());
                    }

                    let out_value = want_value.then(|| out.clone());
                    let mut state = cell.state.lock().await;
                    *state = ErasedState::Ready {
                        value: out,
                        verified_at: rev,
                        changed_at,
                        deps,
                    };
                    drop(state);
                    cell.notify.notify_waiters();

                    debug!(
                        kind = self.kind.0,
                        key_hash = %format!("{:016x}", key_hash),
                        rev = rev.0,
                        "compute: ok"
                    );

                    // 5) stale check
                    if db.runtime().current_revision() == rev {
                        return Ok(ErasedAccessResult {
                            value: out_value,
                            changed_at,
                        });
                    }
                    continue;
                }
                Ok(Err(err)) => {
                    let mut state = cell.state.lock().await;
                    *state = ErasedState::Poisoned {
                        error: err.clone(),
                        verified_at: rev,
                    };
                    drop(state);
                    cell.notify.notify_waiters();

                    debug!(
                        kind = self.kind.0,
                        key_hash = %format!("{:016x}", key_hash),
                        rev = rev.0,
                        "compute: err"
                    );

                    if db.runtime().current_revision() == rev {
                        return Err(err);
                    }
                    continue;
                }
                Err(panic_payload) => {
                    let err = Arc::new(PicanteError::Panic {
                        message: panic_message(panic_payload),
                    });

                    let mut state = cell.state.lock().await;
                    *state = ErasedState::Poisoned {
                        error: err.clone(),
                        verified_at: rev,
                    };
                    drop(state);
                    cell.notify.notify_waiters();

                    debug!(
                        kind = self.kind.0,
                        key_hash = %format!("{:016x}", key_hash),
                        rev = rev.0,
                        "compute: panic"
                    );

                    if db.runtime().current_revision() == rev {
                        return Err(err);
                    }
                    continue;
                }
            }
        }
    }

    async fn try_revalidate<DB>(
        &self,
        db: &DB,
        requested: &DynKey,
        rev: Revision,
        deps: &Arc<[Dep]>,
        self_changed_at: Revision,
    ) -> PicanteResult<bool>
    where
        DB: IngredientLookup + Send + Sync + 'static,
    {
        trace!(
            kind = self.kind.0,
            key_hash = %format!("{:016x}", requested.key.hash()),
            deps = deps.len(),
            "revalidate: start"
        );

        let frame = ActiveFrameHandle::new(requested.clone(), rev);
        let _guard = frame::push_frame(frame);

        for dep in deps.iter() {
            let Some(ingredient) = db.ingredient(dep.kind) else {
                return Ok(false);
            };

            let touch = ingredient.touch(db, dep.key.clone()).await?;
            if touch.changed_at > self_changed_at {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

// ============================================================================
// Thin generic wrapper (one per DB/K/V, but minimal code)
// ============================================================================

/// A memoized async derived query ingredient.
///
/// This is a thin wrapper around `DerivedCore` that handles key encoding
/// and value downcasting. The heavy state machine logic is in the core, which
/// is compiled once instead of per (DB, K, V) combination.
pub struct DerivedIngredient<DB, K, V>
where
    K: Clone + Eq + Hash,
{
    /// Non-generic core containing the type-erased state machine
    core: DerivedCore,
    /// Type information for K and V
    _phantom: PhantomData<(K, V)>,
    /// Type-erased compute function (trait object for dyn dispatch)
    compute: Arc<dyn ErasedCompute<DB>>,
    /// Deep equality function for detecting value changes
    eq_erased: EqErasedFn,
}

impl<DB, K, V> DerivedIngredient<DB, K, V>
where
    DB: IngredientLookup + Send + Sync + 'static,
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    /// Create a new derived ingredient.
    pub fn new(
        kind: QueryKindId,
        kind_name: &'static str,
        compute: impl for<'db> Fn(&'db DB, K) -> ComputeFuture<'db, V> + Send + Sync + 'static,
    ) -> Self {
        // Create typed adapter and erase to trait object
        let typed_compute = TypedCompute {
            f: Arc::new(compute),
            _phantom: PhantomData,
        };
        let compute_erased: Arc<dyn ErasedCompute<DB>> = Arc::new(typed_compute);

        Self {
            core: DerivedCore::new(kind, kind_name),
            _phantom: PhantomData,
            compute: compute_erased,
            eq_erased: eq_erased_for::<V>,
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

    /// Get the value for `key` at the database's current revision.
    pub async fn get(&self, db: &DB, key: K) -> PicanteResult<V> {
        // Encode key once (avoids re-encoding on every lookup)
        let dyn_key = DynKey {
            kind: self.core.kind,
            key: Key::encode_facet(&key)?,
        };

        // Ensure we have a task-local query stack (required for cycle detection + dep tracking).
        let result = frame::scope_if_needed(|| async {
            // Call type-erased core with trait object (dyn dispatch)
            self.core
                .access_scoped_erased(
                    db,
                    dyn_key.clone(),
                    true,
                    self.compute.as_ref(),
                    self.eq_erased,
                )
                .await
        })
        .await?;

        // Downcast at the boundary - MUST succeed due to type safety
        let arc_any = result.value.ok_or_else(|| {
            Arc::new(PicanteError::Panic {
                message: format!("[BUG] expected value but got None for key {:?}", dyn_key),
            })
        })?;

        // Downcast Arc<dyn Any> → Arc<V>
        let arc_v = arc_any.downcast::<V>().map_err(|any| {
            Arc::new(PicanteError::Panic {
                message: format!(
                    "[BUG] type mismatch in get() for ingredient {}: expected {}, got TypeId {:?}",
                    self.core.kind_name,
                    std::any::type_name::<V>(),
                    (&*any as &dyn std::any::Any).type_id()
                ),
            })
        })?;

        // Extract V from Arc (try_unwrap if sole owner, else clone)
        let value = Arc::try_unwrap(arc_v).unwrap_or_else(|arc| (*arc).clone());

        Ok(value)
    }

    /// Ensure the value is valid at the current revision and return its `changed_at`.
    pub async fn touch(&self, db: &DB, key: K) -> PicanteResult<Revision> {
        // Encode key once
        let dyn_key = DynKey {
            kind: self.core.kind,
            key: Key::encode_facet(&key)?,
        };

        // Ensure we have a task-local query stack (required for cycle detection + dep tracking).
        // Note: touch may still compute/revalidate; it just doesn't return the value to the caller.
        let result = frame::scope_if_needed(|| async {
            self.core
                .access_scoped_erased(db, dyn_key, false, self.compute.as_ref(), self.eq_erased)
                .await
        })
        .await?;

        Ok(result.changed_at)
    }

    /// Create a snapshot of this ingredient's cells.
    ///
    /// This is an O(1) operation due to structural sharing in `im::HashMap`.
    /// The returned map shares structure with the live ingredient.
    pub fn snapshot(&self) -> im::HashMap<DynKey, Arc<ErasedCell>> {
        self.core.cells.read().clone()
    }

    /// Load cells from a snapshot into this ingredient.
    ///
    /// This is used when creating database snapshots. Existing cells are replaced.
    pub fn load_cells(&self, cells: im::HashMap<DynKey, Arc<ErasedCell>>) {
        *self.core.cells.write() = cells;
    }

    /// Create a deep snapshot of this ingredient's cells.
    ///
    /// Unlike `snapshot()` which shares `Arc<Cell>` references, this method
    /// creates new `Cell` instances with cloned Ready states. This ensures
    /// the snapshot's cells are independent of the original and won't be
    /// affected by subsequent updates to the original.
    ///
    /// Cells that are not Ready (Vacant, Running, Poisoned) are not included
    /// in the snapshot since they represent transient or invalid states.
    ///
    /// With type-erased storage, cloning is cheap: `Arc<dyn Any>` clone just
    /// bumps the refcount, avoiding deep clone of the value itself.
    pub async fn snapshot_cells_deep(&self) -> im::HashMap<DynKey, Arc<ErasedCell>>
    where
        V: Clone,
    {
        // Collect all cells under lock, then release before async work
        let cells_snapshot: Vec<(DynKey, Arc<ErasedCell>)> = {
            let cells = self.core.cells.read();
            cells.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut result = im::HashMap::new();

        for (dyn_key, cell) in cells_snapshot {
            let state = cell.state.lock().await;
            if let ErasedState::Ready {
                value,
                verified_at,
                changed_at,
                deps,
            } = &*state
            {
                // Clone the Arc<dyn Any> - just bumps refcount (cheap!)
                let cloned_value = value.clone();

                let new_cell = Arc::new(ErasedCell::new_ready(
                    cloned_value,
                    *verified_at,
                    *changed_at,
                    deps.clone(),
                ));
                result.insert(dyn_key, new_cell);
            }
        }

        result
    }
}

// ============================================================================
// Type-erased cell structures (for compile-time optimization)
// ============================================================================

/// Type-erased memoization cell (not generic over V).
///
/// This allows the state machine logic to be compiled once instead of being
/// monomorphized for every query type, dramatically reducing compile times.
pub struct ErasedCell {
    state: Mutex<ErasedState>,
    notify: Notify,
}

/// Type-erased state (not generic over V).
///
/// Values are stored as `Arc<dyn Any + Send + Sync>` where the Any contains V.
/// This enables:
/// - Cheap snapshot cloning via Arc::clone
/// - Type-safe downcast at access boundaries
/// - Single compilation of state machine logic
enum ErasedState {
    Vacant,
    Running {
        started_at: Revision,
    },
    Ready {
        /// The cached value, stored as Arc<dyn Any> where the Any is V.
        /// Use Arc::downcast::<V>() to recover the Arc<V>.
        value: Arc<dyn std::any::Any + Send + Sync>,
        verified_at: Revision,
        changed_at: Revision,
        deps: Arc<[Dep]>,
    },
    Poisoned {
        error: Arc<PicanteError>,
        verified_at: Revision,
    },
}

impl ErasedCell {
    fn new() -> Self {
        Self {
            state: Mutex::new(ErasedState::Vacant),
            notify: Notify::new(),
        }
    }

    fn new_ready(
        value: Arc<dyn std::any::Any + Send + Sync>,
        verified_at: Revision,
        changed_at: Revision,
        deps: Arc<[Dep]>,
    ) -> Self {
        Self {
            state: Mutex::new(ErasedState::Ready {
                value,
                verified_at,
                changed_at,
                deps,
            }),
            notify: Notify::new(),
        }
    }
}

/// Result type for erased access (not generic over V).
struct ErasedAccessResult {
    value: Option<Arc<dyn std::any::Any + Send + Sync>>,
    changed_at: Revision,
}

#[derive(Debug, Clone, Facet)]
struct DepRecord {
    kind_id: u32,
    key_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Facet)]
struct DerivedRecord<K, V> {
    key: K,
    value: V,
    verified_at: u64,
    changed_at: u64,
    deps: Vec<DepRecord>,
}

impl<DB, K, V> PersistableIngredient for DerivedIngredient<DB, K, V>
where
    DB: IngredientLookup + Send + Sync + 'static,
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
        SectionType::Derived
    }

    fn clear(&self) {
        let mut cells = self.core.cells.write();
        *cells = im::HashMap::new();
    }

    fn save_records(&self) -> BoxFuture<'_, PicanteResult<Vec<Vec<u8>>>> {
        Box::pin(async move {
            // Collect snapshot under lock, then release before async work
            let snapshot: Vec<(DynKey, Arc<ErasedCell>)> = {
                let cells = self.core.cells.read();
                cells.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            };
            let mut records = Vec::with_capacity(snapshot.len());

            for (dyn_key, cell) in snapshot {
                // Decode DynKey back to K (we're in DerivedIngredient<DB, K, V> so we know K!)
                let key: K = dyn_key.key.decode_facet().map_err(|e| {
                    Arc::new(PicanteError::Panic {
                        message: format!(
                            "[BUG] failed to decode key for ingredient {} during save: {:?}",
                            self.core.kind_name, e
                        ),
                    })
                })?;

                let state = cell.state.lock().await;
                let ErasedState::Ready {
                    value,
                    verified_at,
                    changed_at,
                    deps,
                } = &*state
                else {
                    continue;
                };

                // Downcast value back to V - MUST succeed (we're in DerivedIngredient<DB, K, V>!)
                let typed_value: &V = value.downcast_ref::<V>().ok_or_else(|| {
                    Arc::new(PicanteError::Panic {
                        message: format!(
                            "[BUG] type mismatch in save_records for ingredient {}: \
                             expected {}, got TypeId {:?}",
                            self.core.kind_name,
                            std::any::type_name::<V>(),
                            (&**value as &dyn std::any::Any).type_id()
                        ),
                    })
                })?;

                let deps = deps
                    .iter()
                    .map(|d| DepRecord {
                        kind_id: d.kind.as_u32(),
                        key_bytes: d.key.bytes().to_vec(),
                    })
                    .collect();

                let rec = DerivedRecord::<K, V> {
                    key,
                    value: typed_value.clone(),
                    verified_at: verified_at.0,
                    changed_at: changed_at.0,
                    deps,
                };

                let bytes = facet_postcard::to_vec(&rec).map_err(|e| {
                    Arc::new(PicanteError::Encode {
                        what: "derived record",
                        message: format!("{e:?}"),
                    })
                })?;
                records.push(bytes);
            }
            debug!(
                kind = self.core.kind.0,
                records = records.len(),
                "save_records (derived)"
            );
            Ok(records)
        })
    }

    fn load_records(&self, records: Vec<Vec<u8>>) -> PicanteResult<()> {
        for bytes in records {
            let rec: DerivedRecord<K, V> = facet_postcard::from_slice(&bytes).map_err(|e| {
                Arc::new(PicanteError::Decode {
                    what: "derived record",
                    message: format!("{e:?}"),
                })
            })?;

            let deps: Arc<[Dep]> = rec
                .deps
                .into_iter()
                .map(|d| Dep {
                    kind: QueryKindId(d.kind_id),
                    key: Key::from_bytes(d.key_bytes),
                })
                .collect::<Vec<_>>()
                .into();

            // Create DynKey from K
            let dyn_key = DynKey {
                kind: self.core.kind,
                key: Key::encode_facet(&rec.key)?,
            };

            // Wrap value as Arc<dyn Any>
            let erased_value = Arc::new(rec.value) as Arc<dyn std::any::Any + Send + Sync>;

            let cell = Arc::new(ErasedCell::new_ready(
                erased_value,
                Revision(rec.verified_at),
                Revision(rec.changed_at),
                deps,
            ));
            let mut cells = self.core.cells.write();
            cells.insert(dyn_key, cell);
        }
        Ok(())
    }

    fn restore_runtime_state<'a>(
        &'a self,
        runtime: &'a crate::runtime::Runtime,
    ) -> BoxFuture<'a, PicanteResult<()>> {
        Box::pin(async move {
            // Collect snapshot under lock, then release before async work
            let snapshot: Vec<(DynKey, Arc<ErasedCell>)> = {
                let cells = self.core.cells.read();
                cells.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            };

            for (dyn_key, cell) in snapshot {
                let state = cell.state.lock().await;
                let ErasedState::Ready { deps, .. } = &*state else {
                    continue;
                };

                runtime.update_query_deps(dyn_key, deps.clone());
            }

            debug!(kind = self.core.kind.0, "restore_runtime_state (derived)");
            Ok(())
        })
    }
}

impl<DB, K, V> DynIngredient<DB> for DerivedIngredient<DB, K, V>
where
    DB: IngredientLookup + Send + Sync + 'static,
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    fn touch<'a>(&'a self, db: &'a DB, key: Key) -> BoxFuture<'a, PicanteResult<Touch>> {
        Box::pin(async move {
            let key: K = key.decode_facet()?;
            let changed_at = self.touch(db, key).await?;
            Ok(Touch { changed_at })
        })
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
