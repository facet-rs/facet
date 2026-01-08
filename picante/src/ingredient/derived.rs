use crate::db::{DynIngredient, IngredientLookup, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame::{self, ActiveFrameHandle};
use crate::inflight::{self, InFlightKey, InFlightState, SharedCacheRecord, TryLeadResult};
use crate::key::{Dep, DynKey, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use facet::Facet;
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
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

// ============================================================================
// Type-erased persistence callbacks (function pointers to avoid monomorphization)
// ============================================================================

/// Data returned when decoding a record (type-erased)
struct ErasedRecordData {
    dyn_key: DynKey,
    value: ArcAny,
    verified_at: Revision,
    changed_at: Revision,
    deps: Arc<[Dep]>,
}

/// Encode a single record to bytes (called from erased save_records)
type EncodeRecordFn = fn(
    kind_name: &'static str,
    dyn_key: &DynKey,
    value: &ArcAny,
    verified_at: Revision,
    changed_at: Revision,
    deps: &[Dep],
) -> PicanteResult<Vec<u8>>;

/// Decode a single record from bytes (called from erased load_records)
/// Takes owned Vec<u8> because facet_postcard::from_slice requires 'static
type DecodeRecordFn = fn(kind: QueryKindId, bytes: Vec<u8>) -> PicanteResult<ErasedRecordData>;

/// Encode incremental record (key + optional value) for WAL
type EncodeIncrementalFn = fn(
    kind_name: &'static str,
    dyn_key: &DynKey,
    value: &ArcAny,
    verified_at: Revision,
    changed_at: Revision,
    deps: &[Dep],
) -> PicanteResult<(Vec<u8>, Vec<u8>)>;

/// Apply a WAL entry (insert or delete)
/// Takes owned bytes because facet_postcard::from_slice requires 'static
type ApplyWalEntryFn = fn(
    kind: QueryKindId,
    key_bytes: Vec<u8>,
    value_bytes: Option<Vec<u8>>,
) -> PicanteResult<ApplyWalResult>;

/// Result of applying a WAL entry
struct ApplyWalResult {
    dyn_key: DynKey,
    cell: Option<Arc<ErasedCell>>, // None = delete
}

/// Function pointer for deep equality check without knowing V
type EqErasedFn = fn(&dyn Any, &dyn Any) -> bool;

// r[type-erasure.mechanism]
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
/// Uses autoref specialization to prefer PartialEq when available,
/// falling back to byte-wise comparison otherwise.
/// This avoids pulling in facet-diff and all its transitive feature dependencies.
fn eq_erased_for<V>(a: &dyn Any, b: &dyn Any) -> bool
where
    V: Facet<'static> + 'static,
{
    crate::facet_eq::facet_eq::<V>(a, b)
}

// ============================================================================
// Non-generic core: state machine compiled ONCE
// ============================================================================

// r[type-erasure.purpose]
// r[type-erasure.benefit]
/// Non-generic core containing the type-erased state machine.
///
/// By keeping this struct non-generic and making its methods generic over parameters,
/// we compile the 300+ line state machine ONCE instead of per-(DB,K,V) combination.
struct DerivedCore {
    kind: QueryKindId,
    kind_name: &'static str,
    cells: RwLock<im::HashMap<DynKey, Arc<ErasedCell>>>,
    // Type-erased persistence callbacks (function pointers, not closures)
    encode_record: EncodeRecordFn,
    decode_record: DecodeRecordFn,
    encode_incremental: EncodeIncrementalFn,
    apply_wal_entry: ApplyWalEntryFn,
}

impl DerivedCore {
    fn new(
        kind: QueryKindId,
        kind_name: &'static str,
        encode_record: EncodeRecordFn,
        decode_record: DecodeRecordFn,
        encode_incremental: EncodeIncrementalFn,
        apply_wal_entry: ApplyWalEntryFn,
    ) -> Self {
        Self {
            kind,
            kind_name,
            cells: RwLock::new(im::HashMap::new()),
            encode_record,
            decode_record,
            encode_incremental,
            apply_wal_entry,
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
            // Create this before inspecting state to avoid missing a notification
            // between observing `Running` and awaiting.
            let notified = cell.notify.notified();

            // 1) fast path: read current state
            enum ErasedObserved {
                Ready {
                    value: Option<Arc<dyn std::any::Any + Send + Sync>>,
                    changed_at: Revision,
                },
                Error(Arc<PicanteError>),
                Running {
                    started_at: Revision,
                },
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
                    ErasedState::Running { started_at } => ErasedObserved::Running {
                        started_at: *started_at,
                    },
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
                ErasedObserved::Running { started_at } => {
                    trace!(
                        kind = self.kind.0,
                        key_hash = %format!("{:016x}", key_hash),
                        started_at = started_at.0,
                        "wait on running cell"
                    );
                    notified.await;
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

            // 3) Check shared completed-result cache for cross-snapshot memoization.
            //    Unlike the in-flight registry, this persists after the leader finishes.
            if let Some(record) =
                inflight::shared_cache_get(db.runtime().id(), self.kind, &requested.key)
            {
                let can_adopt = if record.verified_at == rev {
                    true
                } else {
                    self.try_revalidate(db, &requested, rev, &record.deps, record.changed_at)
                        .await?
                };

                if can_adopt {
                    db.runtime()
                        .update_query_deps(requested.clone(), record.deps.clone());

                    // Mark the adopted cell as verified at the *current* revision.
                    let mut state = cell.state.lock().await;
                    *state = ErasedState::Ready {
                        value: record.value.clone(),
                        verified_at: rev,
                        changed_at: record.changed_at,
                        deps: record.deps.clone(),
                    };
                    drop(state);
                    cell.notify.notify_waiters();

                    // Update the shared cache's verified_at so future lookups can skip revalidation.
                    inflight::shared_cache_put(
                        db.runtime().id(),
                        self.kind,
                        requested.key.clone(),
                        SharedCacheRecord {
                            value: record.value.clone(),
                            deps: record.deps.clone(),
                            changed_at: record.changed_at,
                            verified_at: rev,
                            insert_id: 0,
                        },
                    );

                    if db.runtime().current_revision() == rev {
                        let out_value = want_value.then(|| record.value.clone());
                        return Ok(ErasedAccessResult {
                            value: out_value,
                            changed_at: record.changed_at,
                        });
                    }

                    // If the revision changed mid-adoption, retry.
                    continue;
                }
            }

            // 3) Check global in-flight registry for cross-snapshot deduplication.
            //    This allows concurrent queries from different snapshots to share work.
            let inflight_key = InFlightKey {
                runtime_id: db.runtime().id(),
                revision: rev,
                kind: self.kind,
                key: requested.key.clone(),
            };

            match inflight::try_lead(inflight_key.clone()) {
                TryLeadResult::Follower(entry) => {
                    // Another snapshot is already computing this query.
                    // Wait for it to complete and use its result.
                    trace!(
                        kind = self.kind.0,
                        key_hash = %format!("{:016x}", key_hash),
                        rev = rev.0,
                        "inflight: follower, waiting for leader"
                    );

                    // Reset our local cell to Vacant since we didn't actually start computing.
                    {
                        let mut state = cell.state.lock().await;
                        *state = ErasedState::Vacant;
                    }

                    // Wait for the leader to complete.
                    loop {
                        let notified = entry.notified();
                        let entry_state = entry.state();
                        match entry_state {
                            InFlightState::Running => {
                                // Still computing, wait for notification.
                                notified.await;
                            }
                            InFlightState::Done {
                                value,
                                deps,
                                changed_at,
                            } => {
                                // Leader completed successfully.
                                // Populate our local cell with the result.
                                trace!(
                                    kind = self.kind.0,
                                    key_hash = %format!("{:016x}", key_hash),
                                    rev = rev.0,
                                    "inflight: follower got result from leader"
                                );

                                let out_value = want_value.then(|| value.clone());
                                let mut state = cell.state.lock().await;
                                *state = ErasedState::Ready {
                                    value: value.clone(),
                                    verified_at: rev,
                                    changed_at,
                                    deps: deps.clone(),
                                };
                                drop(state);
                                cell.notify.notify_waiters();

                                // Keep the dependency graph + events consistent even when the
                                // computation happened in another runtime instance.
                                db.runtime()
                                    .update_query_deps(requested.clone(), deps.clone());
                                if changed_at == rev {
                                    db.runtime().notify_query_changed(rev, requested.clone());
                                }

                                // Store in shared completed-result cache for future snapshots.
                                inflight::shared_cache_put(
                                    db.runtime().id(),
                                    self.kind,
                                    requested.key.clone(),
                                    SharedCacheRecord {
                                        value: value.clone(),
                                        deps: deps.clone(),
                                        changed_at,
                                        verified_at: rev,
                                        insert_id: 0,
                                    },
                                );

                                if db.runtime().current_revision() == rev {
                                    return Ok(ErasedAccessResult {
                                        value: out_value,
                                        changed_at,
                                    });
                                }
                                // Revision changed, retry the main loop.
                                break;
                            }
                            InFlightState::Failed(err) => {
                                // Leader failed with an error.
                                trace!(
                                    kind = self.kind.0,
                                    key_hash = %format!("{:016x}", key_hash),
                                    rev = rev.0,
                                    "inflight: follower got error from leader"
                                );

                                let mut state = cell.state.lock().await;
                                *state = ErasedState::Poisoned {
                                    error: err.clone(),
                                    verified_at: rev,
                                };
                                drop(state);
                                cell.notify.notify_waiters();

                                if db.runtime().current_revision() == rev {
                                    return Err(err);
                                }
                                break;
                            }
                            InFlightState::Cancelled => {
                                // Leader was cancelled. We should retry and potentially
                                // become the new leader.
                                trace!(
                                    kind = self.kind.0,
                                    key_hash = %format!("{:016x}", key_hash),
                                    rev = rev.0,
                                    "inflight: leader cancelled, will retry"
                                );
                                break;
                            }
                        }
                    }
                    // Continue the main loop (retry or handle revision change).
                    continue;
                }
                TryLeadResult::Leader(guard) => {
                    // We're the leader. Proceed with computation.
                    trace!(
                        kind = self.kind.0,
                        key_hash = %format!("{:016x}", key_hash),
                        rev = rev.0,
                        "inflight: leader, computing"
                    );

                    // Run compute under an active frame.
                    let frame = ActiveFrameHandle::new(requested.clone(), rev);
                    let _frame_guard = frame::push_frame(frame.clone());

                    debug!(
                        kind = self.kind.0,
                        key_hash = %format!("{:016x}", key_hash),
                        rev = rev.0,
                        "compute: start"
                    );

                    // Call compute through trait object (dyn dispatch)
                    let result =
                        std::panic::AssertUnwindSafe(compute.compute(db, requested.key.clone()))
                            .catch_unwind()
                            .await;

                    let deps: Arc<[Dep]> = frame.take_deps().into();

                    // 4) finalize
                    match result {
                        Ok(Ok(out)) => {
                            // r[revision.early-cutoff]
                            // r[cell.compute]
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

                            // Update local cell.
                            let mut state = cell.state.lock().await;
                            *state = ErasedState::Ready {
                                value: out.clone(),
                                verified_at: rev,
                                changed_at,
                                deps: deps.clone(),
                            };
                            drop(state);
                            cell.notify.notify_waiters();

                            // Store in shared completed-result cache for future snapshots.
                            inflight::shared_cache_put(
                                db.runtime().id(),
                                self.kind,
                                requested.key.clone(),
                                SharedCacheRecord {
                                    value: out.clone(),
                                    deps: deps.clone(),
                                    changed_at,
                                    verified_at: rev,
                                    insert_id: 0,
                                },
                            );

                            // Complete the global in-flight entry so followers can use the result.
                            guard.complete(out, deps, changed_at);

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

                            // Fail the global in-flight entry so followers get the error.
                            guard.fail(err.clone());

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

                            // Fail the global in-flight entry so followers get the panic error.
                            guard.fail(err.clone());

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
        }
    }

    // r[cell.revalidate]
    // r[cell.revalidate-missing]
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

    // ========================================================================
    // Type-erased persistence methods (compiled ONCE, use function pointers)
    // ========================================================================

    /// Save all records using type-erased callbacks
    async fn save_records_erased(&self) -> PicanteResult<Vec<Vec<u8>>> {
        // Collect snapshot under lock, then release before async work
        let snapshot: Vec<(DynKey, Arc<ErasedCell>)> = {
            let cells = self.cells.read();
            cells.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };
        let mut records = Vec::with_capacity(snapshot.len());

        for (dyn_key, cell) in snapshot {
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

            // Call through function pointer (monomorphized once per K,V at construction)
            let bytes = (self.encode_record)(
                self.kind_name,
                &dyn_key,
                value,
                *verified_at,
                *changed_at,
                deps,
            )?;
            records.push(bytes);
        }
        debug!(
            kind = self.kind.0,
            records = records.len(),
            "save_records (derived, erased)"
        );
        Ok(records)
    }

    /// Load records using type-erased callbacks
    fn load_records_erased(&self, records: Vec<Vec<u8>>) -> PicanteResult<()> {
        for bytes in records {
            // Call through function pointer (takes owned Vec<u8>)
            let data = (self.decode_record)(self.kind, bytes)?;

            let cell = Arc::new(ErasedCell::new_ready(
                data.value,
                data.verified_at,
                data.changed_at,
                data.deps,
            ));
            let mut cells = self.cells.write();
            cells.insert(data.dyn_key, cell);
        }
        Ok(())
    }

    /// Save incremental records using type-erased callbacks
    async fn save_incremental_records_erased(
        &self,
        since_revision: u64,
    ) -> PicanteResult<Vec<(u64, Vec<u8>, Option<Vec<u8>>)>> {
        // Collect snapshot under lock, then release before async work
        let snapshot: Vec<(DynKey, Arc<ErasedCell>)> = {
            let cells = self.cells.read();
            cells.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };
        let mut changes = Vec::new();

        for (dyn_key, cell) in snapshot {
            let state = cell.state.lock().await;
            let ErasedState::Ready {
                value,
                changed_at,
                verified_at,
                deps,
            } = &*state
            else {
                continue;
            };

            // Only include entries that changed after the base revision
            if changed_at.0 <= since_revision {
                continue;
            }

            // Call through function pointer
            let (key_bytes, value_bytes) = (self.encode_incremental)(
                self.kind_name,
                &dyn_key,
                value,
                *verified_at,
                *changed_at,
                deps,
            )?;

            changes.push((changed_at.0, key_bytes, Some(value_bytes)));
        }

        debug!(
            kind = self.kind.0,
            changes = changes.len(),
            since_revision,
            "save_incremental_records (derived, erased)"
        );

        Ok(changes)
    }

    /// Apply a WAL entry using type-erased callbacks
    fn apply_wal_entry_erased(
        &self,
        _revision: u64,
        key_bytes: Vec<u8>,
        value_bytes: Option<Vec<u8>>,
    ) -> PicanteResult<()> {
        // Pass owned bytes (callback needs 'static for deserialization)
        let result = (self.apply_wal_entry)(self.kind, key_bytes, value_bytes)?;

        let mut cells = self.cells.write();
        if let Some(cell) = result.cell {
            cells.insert(result.dyn_key, cell);
        } else {
            cells.remove(&result.dyn_key);
        }

        Ok(())
    }
}

// ============================================================================
// Helper functions to create persistence callbacks (monomorphized per K,V)
// ============================================================================

/// Create an encode_record function pointer for a specific K, V
fn make_encode_record<K, V>() -> EncodeRecordFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |kind_name, dyn_key, value, verified_at, changed_at, deps| {
        // Decode DynKey back to K
        let key: K = dyn_key.key.decode_facet().map_err(|e| {
            Arc::new(PicanteError::Panic {
                message: format!(
                    "[BUG] failed to decode key for ingredient {} during save: {:?}",
                    kind_name, e
                ),
            })
        })?;

        // Downcast value back to V
        let typed_value: &V = value.downcast_ref::<V>().ok_or_else(|| {
            Arc::new(PicanteError::Panic {
                message: format!(
                    "[BUG] type mismatch in save_records for ingredient {}: \
                     expected {}, got TypeId {:?}",
                    kind_name,
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

        facet_postcard::to_vec(&rec).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "derived record",
                message: format!("{e:?}"),
            })
        })
    }
}

/// Create a decode_record function pointer for a specific K, V
fn make_decode_record<K, V>() -> DecodeRecordFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |kind, bytes| {
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

        let dyn_key = DynKey {
            kind,
            key: Key::encode_facet(&rec.key)?,
        };

        let value = Arc::new(rec.value) as ArcAny;

        Ok(ErasedRecordData {
            dyn_key,
            value,
            verified_at: Revision(rec.verified_at),
            changed_at: Revision(rec.changed_at),
            deps,
        })
    }
}

/// Create an encode_incremental function pointer for a specific K, V
fn make_encode_incremental<K, V>() -> EncodeIncrementalFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |kind_name, dyn_key, value, verified_at, changed_at, deps| {
        // Decode DynKey back to K
        let key: K = dyn_key.key.decode_facet().map_err(|e| {
            Arc::new(PicanteError::Panic {
                message: format!(
                    "[BUG] failed to decode key for ingredient {} during incremental save: {:?}",
                    kind_name, e
                ),
            })
        })?;

        // Downcast value back to V
        let typed_value: &V = value.downcast_ref::<V>().ok_or_else(|| {
            Arc::new(PicanteError::Panic {
                message: format!(
                    "[BUG] type mismatch in save_incremental_records for ingredient {}: \
                     expected {}, got TypeId {:?}",
                    kind_name,
                    std::any::type_name::<V>(),
                    (&**value as &dyn std::any::Any).type_id()
                ),
            })
        })?;

        let dep_records = deps
            .iter()
            .map(|d| DepRecord {
                kind_id: d.kind.as_u32(),
                key_bytes: d.key.bytes().to_vec(),
            })
            .collect();

        let rec = DerivedRecord::<K, V> {
            key: key.clone(),
            value: typed_value.clone(),
            verified_at: verified_at.0,
            changed_at: changed_at.0,
            deps: dep_records,
        };

        let key_bytes = facet_postcard::to_vec(&key).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "derived key",
                message: format!("{e:?}"),
            })
        })?;

        let value_bytes = facet_postcard::to_vec(&rec).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "derived record",
                message: format!("{e:?}"),
            })
        })?;

        Ok((key_bytes, value_bytes))
    }
}

/// Create an apply_wal_entry function pointer for a specific K, V
fn make_apply_wal_entry<K, V>() -> ApplyWalEntryFn
where
    K: Clone + Eq + Hash + Facet<'static> + Send + Sync + 'static,
    V: Clone + Facet<'static> + Send + Sync + 'static,
{
    |kind, key_bytes, value_bytes| {
        let key: K = facet_postcard::from_slice(&key_bytes).map_err(|e| {
            Arc::new(PicanteError::Decode {
                what: "derived key from WAL",
                message: format!("{e:?}"),
            })
        })?;

        let dyn_key = DynKey {
            kind,
            key: Key::encode_facet(&key)?,
        };

        if let Some(value_bytes) = value_bytes {
            // Deserialize the full DerivedRecord
            let rec: DerivedRecord<K, V> =
                facet_postcard::from_slice(&value_bytes).map_err(|e| {
                    Arc::new(PicanteError::Decode {
                        what: "derived record from WAL",
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

            let erased_value = Arc::new(rec.value) as ArcAny;

            let cell = Arc::new(ErasedCell::new_ready(
                erased_value,
                Revision(rec.verified_at),
                Revision(rec.changed_at),
                deps,
            ));

            Ok(ApplyWalResult {
                dyn_key,
                cell: Some(cell),
            })
        } else {
            // Delete operation
            Ok(ApplyWalResult {
                dyn_key,
                cell: None,
            })
        }
    }
}

// ============================================================================
// Thin generic wrapper (one per DB/K/V, but minimal code)
// ============================================================================

// r[derived.type]
// r[derived.memoization]
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

        // Create persistence callbacks (monomorphized once per K,V)
        let encode_record = make_encode_record::<K, V>();
        let decode_record = make_decode_record::<K, V>();
        let encode_incremental = make_encode_incremental::<K, V>();
        let apply_wal_entry = make_apply_wal_entry::<K, V>();

        Self {
            core: DerivedCore::new(
                kind,
                kind_name,
                encode_record,
                decode_record,
                encode_incremental,
                apply_wal_entry,
            ),
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

    // r[derived.get]
    // r[cell.access]
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

    /// Look up the raw (type-erased) cell for `key`.
    ///
    /// This is intended for cache promotion across runtimes/snapshots.
    pub fn cell_for_key(&self, key: &K) -> PicanteResult<Option<Arc<ErasedCell>>> {
        let dyn_key = DynKey {
            kind: self.core.kind,
            key: Key::encode_facet(key)?,
        };
        Ok(self.core.cells.read().get(&dyn_key).cloned())
    }

    /// Insert a ready cell record into this ingredient (overwriting any existing cell).
    ///
    /// This is intended for cache promotion (e.g. from a snapshot back into a live DB).
    pub fn insert_ready_record(&self, key: &K, record: ErasedReadyRecord) -> PicanteResult<()> {
        let dyn_key = DynKey {
            kind: self.core.kind,
            key: Key::encode_facet(key)?,
        };
        let cell = Arc::new(ErasedCell::new_ready(
            record.value,
            record.verified_at,
            record.changed_at,
            record.deps,
        ));

        let mut cells = self.core.cells.write();
        cells.insert(dyn_key, cell);
        Ok(())
    }

    /// Check whether a ready cell record is still valid against `db` at its current revision.
    ///
    /// If this returns `true`, the record can safely be promoted into another runtime
    /// (e.g. from a request snapshot back into the live database).
    pub async fn record_is_valid_on(
        &self,
        db: &DB,
        record: &ErasedReadyRecord,
    ) -> PicanteResult<bool> {
        let self_changed_at = record.changed_at;
        for dep in record.deps.iter() {
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

    // r[snapshot.derived]
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
    // r[cell.waiter]
    notify: Notify,
}

// r[cell.states]
/// Type-erased state (not generic over V).
///
/// Values are stored as `Arc<dyn Any + Send + Sync>` where the Any contains V.
/// This enables:
/// - Cheap snapshot cloning via Arc::clone
/// - Type-safe downcast at access boundaries
/// - Single compilation of state machine logic
enum ErasedState {
    Vacant,
    // r[cell.leader-local]
    Running {
        started_at: Revision,
    },
    // r[cell.stale]
    // r[revision.verified_at]
    // r[revision.changed_at]
    Ready {
        /// The cached value, stored as Arc<dyn Any> where the Any is V.
        /// Use Arc::downcast::<V>() to recover the Arc<V>.
        value: Arc<dyn std::any::Any + Send + Sync>,
        verified_at: Revision,
        changed_at: Revision,
        deps: Arc<[Dep]>,
    },
    // r[cell.poison]
    // r[cell.poison-scoped]
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

    /// If this cell is in `Ready` state, return its runtime metadata and value.
    ///
    /// This is primarily intended for cache promotion (e.g. from a snapshot back
    /// into a live database) and persistence helpers.
    pub async fn ready_record(&self) -> Option<ErasedReadyRecord> {
        let state = self.state.lock().await;
        match &*state {
            ErasedState::Ready {
                value,
                verified_at,
                changed_at,
                deps,
            } => Some(ErasedReadyRecord {
                value: value.clone(),
                verified_at: *verified_at,
                changed_at: *changed_at,
                deps: deps.clone(),
            }),
            _ => None,
        }
    }
}

/// A type-erased derived-cell record that can be re-inserted into another runtime.
#[derive(Clone)]
pub struct ErasedReadyRecord {
    /// Type-erased value (`Arc<V>` stored behind `dyn Any`).
    pub value: Arc<dyn std::any::Any + Send + Sync>,
    /// Revision at which this value was last verified.
    pub verified_at: Revision,
    /// Last revision at which the value logically changed.
    pub changed_at: Revision,
    /// Dependencies (kind + key) read by this query.
    pub deps: Arc<[Dep]>,
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
        // Delegate to type-erased core (compiled once, uses function pointers)
        Box::pin(self.core.save_records_erased())
    }

    fn load_records(&self, records: Vec<Vec<u8>>) -> PicanteResult<()> {
        // Delegate to type-erased core (compiled once, uses function pointers)
        self.core.load_records_erased(records)
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

    fn save_incremental_records(
        &self,
        since_revision: u64,
    ) -> BoxFuture<'_, PicanteResult<Vec<(u64, Vec<u8>, Option<Vec<u8>>)>>> {
        // Delegate to type-erased core (compiled once, uses function pointers)
        Box::pin(self.core.save_incremental_records_erased(since_revision))
    }

    fn apply_wal_entry(
        &self,
        revision: u64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
    ) -> PicanteResult<()> {
        // Delegate to type-erased core (compiled once, uses function pointers)
        self.core.apply_wal_entry_erased(revision, key, value)
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
