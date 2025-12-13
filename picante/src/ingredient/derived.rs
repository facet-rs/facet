use crate::db::{DynIngredient, IngredientLookup, Touch};
use crate::error::{PicanteError, PicanteResult};
use crate::frame::{self, ActiveFrameHandle};
use crate::key::{Dep, DynKey, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use dashmap::DashMap;
use facet::Facet;
use facet_assert::{Sameness, check_same};
use futures::FutureExt;
use futures::future::BoxFuture;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, trace};

type ComputeFuture<'db, V> = BoxFuture<'db, PicanteResult<V>>;
type ComputeFn<DB, K, V> = dyn for<'db> Fn(&'db DB, K) -> ComputeFuture<'db, V> + Send + Sync;

struct AccessResult<V> {
    value: Option<V>,
    changed_at: Revision,
}

/// A memoized async derived query ingredient.
pub struct DerivedIngredient<DB, K, V> {
    kind: QueryKindId,
    kind_name: &'static str,
    cells: DashMap<K, Arc<Cell<V>>>,
    compute: Arc<ComputeFn<DB, K, V>>,
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
        Self {
            kind,
            kind_name,
            cells: DashMap::new(),
            compute: Arc::new(compute),
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

    /// Get the value for `key` at the database's current revision.
    pub async fn get(&self, db: &DB, key: K) -> PicanteResult<V> {
        let result =
            frame::scope_if_needed(|| async move { self.access_scoped(db, key, true).await })
                .await?;
        Ok(result
            .value
            .expect("access_scoped(want_value = true) returned no value"))
    }

    /// Ensure the value is valid at the current revision and return its `changed_at`.
    pub async fn touch(&self, db: &DB, key: K) -> PicanteResult<Revision> {
        let result =
            frame::scope_if_needed(|| async move { self.access_scoped(db, key, false).await })
                .await?;
        Ok(result.changed_at)
    }

    async fn access_scoped(
        &self,
        db: &DB,
        key: K,
        want_value: bool,
    ) -> PicanteResult<AccessResult<V>> {
        let requested = DynKey {
            kind: self.kind,
            key: Key::encode_facet(&key)?,
        };
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

        let cell = self
            .cells
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Cell::new()))
            .clone();

        loop {
            let rev = db.runtime().current_revision();

            // 1) fast path: read current state
            enum Observed<V> {
                Ready {
                    value: Option<V>,
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
                    State::Ready {
                        value,
                        verified_at,
                        changed_at,
                        ..
                    } if *verified_at == rev => Observed::Ready {
                        value: want_value.then(|| value.clone()),
                        changed_at: *changed_at,
                    },
                    State::Poisoned { error, verified_at } if *verified_at == rev => {
                        Observed::Error(error.clone())
                    }
                    State::Running { started_at } => {
                        trace!(
                            kind = self.kind.0,
                            key_hash = %format!("{:016x}", key_hash),
                            started_at = started_at.0,
                            "wait on running cell"
                        );
                        Observed::Running
                    }
                    State::Ready {
                        deps, changed_at, ..
                    } => Observed::StaleReady {
                        deps: deps.clone(),
                        changed_at: *changed_at,
                    },
                    _ => Observed::StaleOther,
                }
            };

            match observed {
                Observed::Ready { value, changed_at } => {
                    // Ensure we return a value consistent with *now*.
                    if db.runtime().current_revision() == rev {
                        return Ok(AccessResult { value, changed_at });
                    }
                    continue;
                }
                Observed::Error(e) => {
                    if db.runtime().current_revision() == rev {
                        return Err(e);
                    }
                    continue;
                }
                Observed::Running => {
                    // Running: wait for the owner to finish.
                    cell.notify.notified().await;
                    continue;
                }
                Observed::StaleReady { deps, changed_at } => {
                    if self
                        .try_revalidate(db, &requested, rev, &deps, changed_at)
                        .await?
                    {
                        let mut state = cell.state.lock().await;
                        match &mut *state {
                            State::Ready {
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
                                    return Ok(AccessResult {
                                        value: out_value,
                                        changed_at: out_changed_at,
                                    });
                                }
                                continue;
                            }
                            State::Running { .. } => {
                                // Someone else raced and started recomputing.
                                continue;
                            }
                            _ => continue,
                        }
                    }
                }
                Observed::StaleOther => {}
            }

            // 2) attempt to start computation
            let (started, prev) = {
                let mut prev: Option<(V, Revision)> = None;
                let mut state = cell.state.lock().await;
                match &*state {
                    State::Ready { verified_at, .. } if *verified_at == rev => (false, None), // raced
                    State::Poisoned { verified_at, .. } if *verified_at == rev => (false, None), // raced
                    State::Running { .. } => (false, None), // someone else started
                    _ => {
                        let old =
                            std::mem::replace(&mut *state, State::Running { started_at: rev });
                        if let State::Ready {
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

            let result = std::panic::AssertUnwindSafe((self.compute)(db, key.clone()))
                .catch_unwind()
                .await;

            let deps: Arc<[Dep]> = frame.take_deps().into();

            // 4) finalize
            match result {
                Ok(Ok(out)) => {
                    let changed_at = match prev {
                        Some((prev_value, prev_changed_at)) => {
                            match check_same(&prev_value, &out) {
                                Sameness::Same => prev_changed_at,
                                _ => rev,
                            }
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
                    *state = State::Ready {
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
                        return Ok(AccessResult {
                            value: out_value,
                            changed_at,
                        });
                    }
                    continue;
                }
                Ok(Err(err)) => {
                    let mut state = cell.state.lock().await;
                    *state = State::Poisoned {
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
                    *state = State::Poisoned {
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

    async fn try_revalidate(
        &self,
        db: &DB,
        requested: &DynKey,
        rev: Revision,
        deps: &Arc<[Dep]>,
        self_changed_at: Revision,
    ) -> PicanteResult<bool> {
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

struct Cell<V> {
    state: Mutex<State<V>>,
    notify: Notify,
}

impl<V> Cell<V> {
    fn new() -> Self {
        Self {
            state: Mutex::new(State::Vacant),
            notify: Notify::new(),
        }
    }

    fn new_ready(value: V, verified_at: Revision, changed_at: Revision, deps: Arc<[Dep]>) -> Self {
        Self {
            state: Mutex::new(State::Ready {
                value,
                verified_at,
                changed_at,
                deps,
            }),
            notify: Notify::new(),
        }
    }
}

enum State<V> {
    Vacant,
    Running {
        started_at: Revision,
    },
    Ready {
        value: V,
        verified_at: Revision,
        changed_at: Revision,
        deps: Arc<[Dep]>,
    },
    Poisoned {
        error: Arc<PicanteError>,
        verified_at: Revision,
    },
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
        self.kind
    }

    fn kind_name(&self) -> &'static str {
        self.kind_name
    }

    fn section_type(&self) -> SectionType {
        SectionType::Derived
    }

    fn clear(&self) {
        self.cells.clear();
    }

    fn save_records(&self) -> BoxFuture<'_, PicanteResult<Vec<Vec<u8>>>> {
        Box::pin(async move {
            let mut records = Vec::with_capacity(self.cells.len());
            let snapshot: Vec<(K, Arc<Cell<V>>)> = self
                .cells
                .iter()
                .map(|e| (e.key().clone(), e.value().clone()))
                .collect();

            for (key, cell) in snapshot {
                let state = cell.state.lock().await;
                let State::Ready {
                    value,
                    verified_at,
                    changed_at,
                    deps,
                } = &*state
                else {
                    continue;
                };

                let deps = deps
                    .iter()
                    .map(|d| DepRecord {
                        kind_id: d.kind.as_u32(),
                        key_bytes: d.key.bytes().to_vec(),
                    })
                    .collect();

                let rec = DerivedRecord::<K, V> {
                    key,
                    value: value.clone(),
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
                kind = self.kind.0,
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

            let cell = Arc::new(Cell::new_ready(
                rec.value,
                Revision(rec.verified_at),
                Revision(rec.changed_at),
                deps,
            ));
            self.cells.insert(rec.key, cell);
        }
        Ok(())
    }

    fn restore_runtime_state<'a>(
        &'a self,
        runtime: &'a crate::runtime::Runtime,
    ) -> BoxFuture<'a, PicanteResult<()>> {
        Box::pin(async move {
            let snapshot: Vec<(K, Arc<Cell<V>>)> = self
                .cells
                .iter()
                .map(|e| (e.key().clone(), e.value().clone()))
                .collect();

            for (key, cell) in snapshot {
                let state = cell.state.lock().await;
                let State::Ready { deps, .. } = &*state else {
                    continue;
                };

                let query = DynKey {
                    kind: self.kind,
                    key: Key::encode_facet(&key)?,
                };
                runtime.update_query_deps(query, deps.clone());
            }

            debug!(kind = self.kind.0, "restore_runtime_state (derived)");
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
