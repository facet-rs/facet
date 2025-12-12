use crate::error::{PicanteError, PicanteResult};
use crate::frame::{self, ActiveFrameHandle};
use crate::key::{Dep, DynKey, Key, QueryKindId};
use crate::persist::{PersistableIngredient, SectionType};
use crate::revision::Revision;
use crate::runtime::HasRuntime;
use dashmap::DashMap;
use facet::Facet;
use futures::FutureExt;
use futures::future::BoxFuture;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, trace};

type ComputeFuture<'db, V> = BoxFuture<'db, PicanteResult<V>>;
type ComputeFn<DB, K, V> = dyn for<'db> Fn(&'db DB, K) -> ComputeFuture<'db, V> + Send + Sync;

/// A memoized async derived query ingredient.
pub struct DerivedIngredient<DB, K, V> {
    kind: QueryKindId,
    kind_name: &'static str,
    cells: DashMap<K, Arc<Cell<V>>>,
    compute: Arc<ComputeFn<DB, K, V>>,
}

impl<DB, K, V> DerivedIngredient<DB, K, V>
where
    DB: HasRuntime + Send + Sync + 'static,
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
        frame::scope_if_needed(|| async move { self.get_scoped(db, key).await }).await
    }

    async fn get_scoped(&self, db: &DB, key: K) -> PicanteResult<V> {
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
        if frame::has_active_frame() {
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
                Ready(V),
                Error(Arc<PicanteError>),
                Running,
                Stale,
            }

            let observed = {
                let state = cell.state.lock().await;
                match &*state {
                    State::Ready {
                        value, verified_at, ..
                    } if *verified_at == rev => Observed::Ready(value.clone()),
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
                    _ => Observed::Stale,
                }
            };

            match observed {
                Observed::Ready(v) => {
                    // Ensure we return a value consistent with *now*.
                    if db.runtime().current_revision() == rev {
                        return Ok(v);
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
                Observed::Stale => {}
            }

            // 2) attempt to start computation
            let started = {
                let mut state = cell.state.lock().await;
                match &*state {
                    State::Ready { verified_at, .. } if *verified_at == rev => false, // raced
                    State::Poisoned { verified_at, .. } if *verified_at == rev => false, // raced
                    State::Running { .. } => false, // someone else started
                    _ => {
                        *state = State::Running { started_at: rev };
                        true
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

            let deps = frame.take_deps();

            // 4) finalize
            match result {
                Ok(Ok(out)) => {
                    let mut state = cell.state.lock().await;
                    *state = State::Ready {
                        value: out.clone(),
                        verified_at: rev,
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
                        return Ok(out);
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

    fn new_ready(value: V, verified_at: Revision, deps: Vec<Dep>) -> Self {
        Self {
            state: Mutex::new(State::Ready {
                value,
                verified_at,
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
        deps: Vec<Dep>,
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
    deps: Vec<DepRecord>,
}

impl<DB, K, V> PersistableIngredient for DerivedIngredient<DB, K, V>
where
    DB: HasRuntime + Send + Sync + 'static,
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

            let deps = rec
                .deps
                .into_iter()
                .map(|d| Dep {
                    kind: QueryKindId(d.kind_id),
                    key: Key::from_bytes(d.key_bytes),
                })
                .collect();

            let cell = Arc::new(Cell::new_ready(rec.value, Revision(rec.verified_at), deps));
            self.cells.insert(rec.key, cell);
        }
        Ok(())
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
