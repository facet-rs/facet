//! The demand-engine design ORACLE: an interpret-only executable model of vix
//! evaluation semantics, for testing design decisions by running programs.
//!
//! Not the real engine — no JIT, no distribution, no laziness yet — but the
//! parts that define IDENTITY and CACHING are exact:
//!
//!   - every value is hashable and TOTALLY ORDERED (floats: total order with
//!     NaN last; maps are BTreeMaps, so iteration IS canonical order; enum
//!     variants order by DECLARATION index);
//!   - memo keys are canonical-AST-hash × args-hash. Function identity is the
//!     AST modulo spans: editing whitespace/comments anywhere — including
//!     inside the function — does not change its hash (the rmeta-cutoff idea
//!     in miniature);
//!   - the aggregation unit v0 = named function call ("we don't memo 2 + x");
//!   - primitives (`fetch`, `X::acquire`) are OBSERVATIONS pinned in a
//!     journal; a warm run replays the pin instead of re-observing;
//!   - partial application: a call missing params (with a trailing `..`)
//!     yields a Partial value; completing it merges arguments;
//!   - cache hits/misses/observations are EVENTS — the oracle is observable
//!     by construction, because that's the product's whole posture.
//!
//! The picante mapping this rehearses: `call_fn` is a query, the memo key is
//! the query key, the journal is the input layer. When the semantics settle,
//! these become picante queries and this file becomes their reference oracle.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::ast::Spanned;
use crate::ast::{self, Arg, ArrayElem, Block, CommandPart, Expr, Member, PathRef, Pattern, Stmt};
use crate::module::{EnumInfo, ModuleTables, StructInfo, VariantShape, load_module_tables};

// ---------------------------------------------------------------------------
// Values: hashable, totally ordered, canonical — and SHIPPABLE. Facet is the
// data plane: a value serializes to postcard bytes and reconstitutes on
// another host (or another oracle, which is the same thing minus the wire).
// ---------------------------------------------------------------------------

#[derive(facet::Facet, Debug, Clone)]
#[repr(u8)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Path(String),
    Flag(String),
    Tuple(Vec<Value>),
    Array(Vec<Value>),
    Map(BTreeMap<Value, Value>),
    /// Fields in DECLARATION order (that order is the total order).
    Struct {
        name: String,
        fields: Vec<(String, Value)>,
    },
    /// `index` is the declaration index — the total order over variants.
    Variant {
        enum_name: String,
        index: usize,
        name: String,
        payload: Payload,
    },
    /// A top-level function as a value.
    Fn {
        name: String,
        hash: u64,
    },
    /// A closure: canonical body hash + captured environment.
    Closure {
        hash: u64,
        params: Vec<String>,
        body: Box<Expr>,
        env: Vec<(String, Value)>,
    },
    /// A partially-applied named function (`f(a: x, ..)`).
    Partial {
        func: String,
        given: Vec<(String, Value)>,
    },
    /// A content-addressed tree — the currency of exec composition. Reading a
    /// Tree VALUE is pure (it's a value, not the world); only mounted trees
    /// at exec time are observed.
    Tree(crate::exec::Tree),
    /// A tree STILL BEING PRODUCED by a live run. Projection (`pending / path`)
    /// demands one path; identity (hash/cmp/ship) and whole-tree uses (mount,
    /// glob, merge) force the flush. The id points into the run registry.
    PendingTree {
        run: u64,
    },
}

#[derive(facet::Facet, Debug, Clone)]
#[repr(u8)]
pub enum Payload {
    Unit,
    Tuple(Vec<Value>),
    Record(Vec<(String, Value)>),
}

impl Value {
    fn rank(&self) -> u8 {
        match self {
            Value::Int(_) => 0,
            Value::Float(_) => 1,
            Value::Bool(_) => 2,
            Value::Str(_) => 3,
            Value::Path(_) => 4,
            Value::Flag(_) => 5,
            Value::Tuple(_) => 6,
            Value::Array(_) => 7,
            Value::Map(_) => 8,
            Value::Struct { .. } => 9,
            Value::Variant { .. } => 10,
            Value::Fn { .. } => 11,
            Value::Closure { .. } => 12,
            Value::Partial { .. } => 13,
            // Pending trees rank AS trees: identity forces them into one.
            Value::Tree(_) | Value::PendingTree { .. } => 14,
        }
    }

    /// Identity forces: a pending tree resolves to its flushed tree before
    /// hashing/comparison. Panics if the registry lost the run (oracle-grade).
    fn forced_tree(&self) -> Option<crate::exec::Tree> {
        match self {
            Value::Tree(t) => Some(t.clone()),
            Value::PendingTree { run } => Some(force_run(*run).expect("pending run flushes").0),
            _ => None,
        }
    }

    /// A SHORT human rendering — event payloads say what is being computed
    /// without dumping whole trees. Paths/strings verbatim, aggregates
    /// summarized, trees by identity.
    pub fn short(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Bool(v) => v.to_string(),
            Value::Str(v) => format!("{v:?}"),
            Value::Path(v) => v.clone(),
            Value::Flag(v) => v.clone(),
            Value::Tuple(vs) => format!(
                "({})",
                vs.iter().map(|v| v.short()).collect::<Vec<_>>().join(", ")
            ),
            Value::Array(vs) => format!(
                "[{}]",
                vs.iter().map(|v| v.short()).collect::<Vec<_>>().join(", ")
            ),
            Value::Map(entries) => format!("{{…{} entries}}", entries.len()),
            Value::Struct { name, fields } => format!("{name}{{…{}}}", fields.len()),
            Value::Variant {
                enum_name, name, ..
            } => format!("{enum_name}::{name}"),
            Value::Fn { name, .. } => format!("fn {name}"),
            Value::Closure { .. } => "closure".to_string(),
            Value::Partial { func, .. } => format!("partial {func}"),
            Value::Tree(t) => {
                let mut h = DefaultHasher::new();
                self.hash_into(&mut h);
                format!("tree({:08x}, {} paths)", h.finish() as u32, t.entries.len())
            }
            Value::PendingTree { run } => format!("pending(run {run})"),
        }
    }

    /// Structural hash — the canonical identity of a value.
    pub fn hash_into(&self, h: &mut DefaultHasher) {
        self.rank().hash(h);
        match self {
            Value::Int(v) => v.hash(h),
            Value::Float(v) => normalize_float(*v).to_bits().hash(h),
            Value::Bool(v) => v.hash(h),
            Value::Str(v) | Value::Path(v) | Value::Flag(v) => v.hash(h),
            Value::Tuple(vs) | Value::Array(vs) => {
                vs.len().hash(h);
                for v in vs {
                    v.hash_into(h);
                }
            }
            Value::Map(m) => {
                m.len().hash(h);
                for (k, v) in m {
                    k.hash_into(h);
                    v.hash_into(h);
                }
            }
            Value::Struct { name, fields } => {
                name.hash(h);
                for (fname, v) in fields {
                    fname.hash(h);
                    v.hash_into(h);
                }
            }
            Value::Variant {
                enum_name,
                index,
                payload,
                ..
            } => {
                enum_name.hash(h);
                index.hash(h);
                match payload {
                    Payload::Unit => {}
                    Payload::Tuple(vs) => {
                        for v in vs {
                            v.hash_into(h);
                        }
                    }
                    Payload::Record(fs) => {
                        for (n, v) in fs {
                            n.hash(h);
                            v.hash_into(h);
                        }
                    }
                }
            }
            Value::Fn { hash, .. } => hash.hash(h),
            Value::Closure { hash, env, .. } => {
                hash.hash(h);
                for (n, v) in env {
                    n.hash(h);
                    v.hash_into(h);
                }
            }
            Value::Partial { func, given } => {
                func.hash(h);
                for (n, v) in given {
                    n.hash(h);
                    v.hash_into(h);
                }
            }
            Value::Tree(_) | Value::PendingTree { .. } => {
                self.forced_tree().expect("tree rank").fingerprint().hash(h)
            }
        }
    }

    pub fn canon_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.hash_into(&mut h);
        h.finish()
    }
}

/// Total order on floats: -0.0 == 0.0, NaN normalized and sorted last.
fn normalize_float(v: f64) -> f64 {
    if v.is_nan() {
        f64::NAN // one canonical NaN bit pattern via the constant
    } else if v == 0.0 {
        0.0
    } else {
        v
    }
}

fn float_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    normalize_float(a).total_cmp(&normalize_float(b))
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => float_cmp(*a, *b),
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Str(a), Value::Str(b))
            | (Value::Path(a), Value::Path(b))
            | (Value::Flag(a), Value::Flag(b)) => a.cmp(b),
            (Value::Tuple(a), Value::Tuple(b)) | (Value::Array(a), Value::Array(b)) => a.cmp(b),
            (Value::Map(a), Value::Map(b)) => a.cmp(b),
            (
                Value::Struct {
                    name: an,
                    fields: af,
                },
                Value::Struct {
                    name: bn,
                    fields: bf,
                },
            ) => an.cmp(bn).then_with(|| af.cmp(bf)),
            (
                Value::Variant {
                    enum_name: ae,
                    index: ai,
                    payload: ap,
                    ..
                },
                Value::Variant {
                    enum_name: be,
                    index: bi,
                    payload: bp,
                    ..
                },
            ) => ae.cmp(be).then_with(|| ai.cmp(bi)).then_with(|| {
                // Same variant: compare payloads structurally.
                match (ap, bp) {
                    (Payload::Unit, Payload::Unit) => Ordering::Equal,
                    (Payload::Tuple(a), Payload::Tuple(b)) => a.cmp(b),
                    (Payload::Record(a), Payload::Record(b)) => a.cmp(b),
                    _ => Ordering::Equal, // same variant index implies same shape
                }
            }),
            (Value::Fn { hash: a, .. }, Value::Fn { hash: b, .. }) => a.cmp(b),
            (Value::Closure { .. }, Value::Closure { .. })
            | (Value::Partial { .. }, Value::Partial { .. }) => {
                self.canon_hash().cmp(&other.canon_hash())
            }
            (
                Value::Tree(_) | Value::PendingTree { .. },
                Value::Tree(_) | Value::PendingTree { .. },
            ) => {
                let a = self.forced_tree().expect("tree rank");
                let b = other.forced_tree().expect("tree rank");
                a.entries.cmp(&b.entries)
            }
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical AST identity: the generated `strip_spans` zeroes every span
// (comments/whitespace never reach the AST; spans are the only position
// leak), then PHON bytes ARE the content address — the same bytes that ship
// a closure to an executor. One codec (schema'd, evolvable) for identity,
// transport, and eventually the journal.
// ---------------------------------------------------------------------------

fn canon_expr_hash(expr: &Expr) -> u64 {
    let bytes = phon::api::encode(expr).expect("AST serializes");
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Serialize a value for transport — the exec primitive's payload format.
/// Shipping is an IDENTITY use: pending trees force first.
pub fn ship(value: &Value) -> Result<Vec<u8>, String> {
    let forced = deep_force(value.clone())?;
    phon::api::encode(&forced).map_err(|e| format!("ship: {e}"))
}

/// Replace every pending tree in a value with its flushed tree (identity
/// forces, recursively). The edge of the graph — `Oracle::call` — does this
/// to its result: DEMAND ENTERS AT THE EDGE.
pub fn deep_force(value: Value) -> Result<Value, String> {
    deep_force_with(value, &|id| force_run(id).map(|(t, _)| t))
}

fn deep_force_with(
    value: Value,
    force: &impl Fn(u64) -> Result<crate::exec::Tree, String>,
) -> Result<Value, String> {
    Ok(match value {
        Value::PendingTree { run } => Value::Tree(force(run)?),
        Value::Tuple(vs) => Value::Tuple(
            vs.into_iter()
                .map(|v| deep_force_with(v, force))
                .collect::<Result<_, _>>()?,
        ),
        Value::Array(vs) => Value::Array(
            vs.into_iter()
                .map(|v| deep_force_with(v, force))
                .collect::<Result<_, _>>()?,
        ),
        Value::Map(m) => Value::Map(
            m.into_iter()
                .map(|(k, v)| {
                    Ok::<_, String>((deep_force_with(k, force)?, deep_force_with(v, force)?))
                })
                .collect::<Result<_, _>>()?,
        ),
        Value::Struct { name, fields } => Value::Struct {
            name,
            fields: fields
                .into_iter()
                .map(|(n, v)| Ok::<_, String>((n, deep_force_with(v, force)?)))
                .collect::<Result<_, _>>()?,
        },
        Value::Variant {
            enum_name,
            index,
            name,
            payload,
        } => Value::Variant {
            enum_name,
            index,
            name,
            payload: match payload {
                Payload::Unit => Payload::Unit,
                Payload::Tuple(vs) => Payload::Tuple(
                    vs.into_iter()
                        .map(|v| deep_force_with(v, force))
                        .collect::<Result<_, _>>()?,
                ),
                Payload::Record(fs) => Payload::Record(
                    fs.into_iter()
                        .map(|(n, v)| Ok::<_, String>((n, deep_force_with(v, force)?)))
                        .collect::<Result<_, _>>()?,
                ),
            },
        },
        other => other,
    })
}

/// Reconstitute a shipped value on the receiving side.
pub fn receive(bytes: &[u8]) -> Result<Value, String> {
    phon::api::decode(bytes).map_err(|e| format!("receive: {e}"))
}

// ---------------------------------------------------------------------------
// The oracle.
// ---------------------------------------------------------------------------

/// Observable evaluation events — the oracle's whole point. Each carries
/// enough IDENTITY to reconstruct the build graph and enough SOURCE (spans)
/// to link back into the editor: run ids pair Spawn↔Exec exactly; `caller`/
/// `in_fn` give demand ancestry; spans are byte ranges into the module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A memoized call ran (cold). `span` is the fn's declaration; `args`
    /// renders each bound argument SHORTLY — what is being computed.
    Miss {
        func: String,
        span: crate::support::Span,
        caller: Option<String>,
        args: Vec<(String, String)>,
    },
    /// A memoized call was served from cache.
    Hit {
        func: String,
        span: crate::support::Span,
        caller: Option<String>,
        args: Vec<(String, String)>,
    },
    /// THUNK CREATED: the `cmd! { … }` block evaluated to a pending tree.
    /// Nothing has run, nothing has been demanded — a value exists. `describe`
    /// is the command grammar's important-first description (level 0 = verb +
    /// object; deeper = modifiers; last = full argv).
    Created {
        command: String,
        run: u64,
        span: crate::support::Span,
        in_fn: Option<String>,
        argv: Vec<String>,
        describe: Vec<String>,
    },
    /// EXECUTION SCHEDULED: the first demand touched this run — a path
    /// projection or an identity force. This is when we start PAYING. Fires
    /// at most once per run. (Locally execution is synchronous so Scheduled
    /// and Finished are adjacent; on the wire they have real extent — this
    /// pair is the rectangle in the lanes view.)
    Scheduled {
        command: String,
        run: u64,
        span: crate::support::Span,
    },
    /// A primitive observed the outside world (cold) or replayed its pin.
    Observation { key: String, replayed: bool },
    /// EXECUTION FINISHED: the run resolved through the two-tier exec cache
    /// (the language-level memo and exec-level cache COMPOSE: a fn-level miss
    /// can still cut off at tier-2 below). `outputs` is the produced tree —
    /// the artifacts, observable path by path. A run that was only ever
    /// projected may never log Finished even though it was Scheduled.
    Finished {
        command: String,
        run: u64,
        span: crate::support::Span,
        event: crate::exec::ExecEvent,
        outputs: Vec<(String, String)>,
    },
}

/// A live tap on [`Event`]s as they happen (see [`Oracle::with_sink`]). The
/// first argument is the eval-relative timestamp in microseconds — timing is
/// an OBSERVATION about evaluation, not part of the pure event, so it rides
/// beside the event instead of inside it (the post-hoc log stays timeless).
pub type EventSink = Box<dyn Fn(u64, &Event) + Send>;

type Frame = Vec<(String, Value)>;

/// Where command blocks actually run. The oracle's local two-tier cache is
/// the default; a fleet of wire executors plugs in here (sync on purpose —
/// the oracle is sync and wasm-clean; the async wire bridges on its side).
///
/// DEMAND-DRIVEN: spawn returns immediately with a live run. The language
/// rule is "projection doesn't force; identity does" — demanding one path
/// blocks only until the producer writes it; hashing/comparing/shipping/
/// mounting a pending tree forces its flush.
pub trait ExecBackend: Send + Sync {
    fn spawn(
        &self,
        command: &str,
        plan: &crate::exec::ExecPlan,
        capability: u64,
        mounts: &[crate::exec::Mount],
    ) -> Result<std::sync::Arc<dyn PendingRun>, String>;
}

/// A live (or completed) run. Values hold these through the run REGISTRY so
/// Value stays plain facet data (a run id), and context-free forcing (Ord,
/// Hash, ship) still works.
pub trait PendingRun: Send + Sync {
    /// Block until the producer writes this path (or fails/finishes without it).
    fn demand_path(&self, path: &str) -> Result<String, String>;
    /// Block until completion; idempotent. Returns the flushed tree and how
    /// the run was served.
    fn flush(&self) -> Result<(crate::exec::Tree, crate::exec::ExecEvent), String>;
}

/// The process-global run registry: PendingTree values carry a u64 into this
/// table. Oracle-grade shortcut (the real engine owns runs in its graph);
/// std-only so the crate stays wasm-clean — blocking lives in backends.
pub(crate) mod runs {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    use super::PendingRun;

    fn table() -> &'static Mutex<HashMap<u64, Arc<dyn PendingRun>>> {
        static TABLE: OnceLock<Mutex<HashMap<u64, Arc<dyn PendingRun>>>> = OnceLock::new();
        TABLE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn register(run: Arc<dyn PendingRun>) -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        table().lock().unwrap().insert(id, run);
        id
    }

    pub fn get(id: u64) -> Option<Arc<dyn PendingRun>> {
        table().lock().unwrap().get(&id).cloned()
    }
}

/// Force a pending tree to its flushed value ("identity forces").
fn force_run(id: u64) -> Result<(crate::exec::Tree, crate::exec::ExecEvent), String> {
    runs::get(id)
        .ok_or_else(|| format!("pending run {id} not in the registry"))?
        .flush()
}

/// The local cache's "pending" run: already complete at spawn time (the
/// local path is the caching oracle; the wire is where demand overlaps).
pub(crate) struct LocalRun {
    pub(crate) outputs: crate::exec::Tree,
    pub(crate) event: crate::exec::ExecEvent,
}

impl PendingRun for LocalRun {
    fn demand_path(&self, path: &str) -> Result<String, String> {
        self.outputs
            .entries
            .get(path)
            .cloned()
            .ok_or_else(|| format!("no `{path}` in outputs"))
    }

    fn flush(&self) -> Result<(crate::exec::Tree, crate::exec::ExecEvent), String> {
        Ok((self.outputs.clone(), self.event.clone()))
    }
}

pub struct Oracle {
    fns: HashMap<String, ast::FnItem>,
    fn_hashes: HashMap<String, u64>,
    enums: HashMap<String, EnumInfo>,
    structs: HashMap<String, StructInfo>,
    memo: RefCell<HashMap<(u64, u64), Value>>,
    journal: RefCell<BTreeMap<String, Value>>,
    events: RefCell<Vec<Event>>,
    /// A LIVE sink invoked on every event as it happens — the daemon uses this
    /// to STREAM demand events to an IDE and to GATE them (block the oracle
    /// thread until the client steps). `events` above is the post-hoc log; the
    /// sink is the real-time tap. Both are fed by `emit`.
    sink: Option<EventSink>,
    exec_cache: RefCell<crate::exec::ExecCache>,
    backend: Option<Box<dyn ExecBackend>>,
    /// Run ids this oracle spawned that haven't logged their Exec event yet
    /// (logged on the first oracle-side force; identity-side forces from
    /// Ord/Hash are silent — they have no oracle in scope).
    unlogged_runs: RefCell<HashMap<u64, (String, crate::support::Span)>>,
    /// Runs whose Scheduled event already fired (first-demand latch).
    scheduled: RefCell<std::collections::HashSet<u64>>,
    /// The stack of fn names currently evaluating (cold path only) — demand
    /// ancestry for events: who demanded this call, whose body spawned this.
    fn_stack: RefCell<Vec<String>>,
    /// When this oracle was created — sink timestamps are relative to it.
    epoch: std::time::Instant,
}

type EvalResult = Result<Value, String>;

impl Oracle {
    pub fn load(source: &str) -> Result<Oracle, String> {
        let ModuleTables {
            fns,
            fn_hashes,
            enums,
            structs,
        } = load_module_tables(source)?;
        Ok(Oracle {
            fns,
            fn_hashes,
            enums,
            structs,
            memo: RefCell::new(HashMap::new()),
            journal: RefCell::new(BTreeMap::new()),
            events: RefCell::new(Vec::new()),
            sink: None,
            exec_cache: RefCell::new(crate::exec::ExecCache::new()),
            backend: None,
            unlogged_runs: RefCell::new(HashMap::new()),
            scheduled: RefCell::new(std::collections::HashSet::new()),
            fn_stack: RefCell::new(Vec::new()),
            epoch: std::time::Instant::now(),
        })
    }

    /// Reload the module tables while preserving warm state. Memo entries are
    /// keyed by canonical function hash plus argument hash, so unchanged
    /// functions keep hitting and edited functions miss under their new hash.
    /// The event log, live sink, call stack, and timestamp epoch are per eval.
    pub fn reload(&mut self, source: &str) -> Result<(), String> {
        let ModuleTables {
            fns,
            fn_hashes,
            enums,
            structs,
        } = load_module_tables(source)?;
        self.fns = fns;
        self.fn_hashes = fn_hashes;
        self.enums = enums;
        self.structs = structs;
        self.events.borrow_mut().clear();
        self.sink = None;
        self.fn_stack.borrow_mut().clear();
        self.epoch = std::time::Instant::now();
        Ok(())
    }

    /// Route command blocks through a fleet instead of the local cache.
    pub fn with_backend(mut self, backend: Box<dyn ExecBackend>) -> Self {
        self.backend = Some(backend);
        self
    }

    /// Install a live event sink (the daemon streams + gates through it).
    pub fn with_sink(mut self, sink: EventSink) -> Self {
        self.sink = Some(sink);
        self
    }

    /// Replace or remove the live event sink for the next evaluation.
    pub fn set_sink(&mut self, sink: Option<EventSink>) {
        self.sink = sink;
    }

    /// Record an event: append to the log AND tap the live sink (in that
    /// order, so the log is consistent when the sink blocks for a step).
    fn emit(&self, event: Event) {
        self.events.borrow_mut().push(event.clone());
        if let Some(sink) = &self.sink {
            let at_micros = self.epoch.elapsed().as_micros() as u64;
            sink(at_micros, &event);
        }
    }

    /// The canonical identity of a top-level function (AST modulo spans).
    pub fn fn_hash(&self, name: &str) -> Option<u64> {
        self.fn_hashes.get(name).copied()
    }

    /// Whether `func`'s single parameter is typed `Target` (so a caller can
    /// supply a canned target). True only for a one-param fn whose type path
    /// ends in `Target`.
    pub fn fn_param_is_target(&self, func: &str) -> bool {
        self.fns.get(func).is_some_and(|f| {
            f.params.params.len() == 1
                && matches!(&f.params.params[0].ty, ast::Type::Path(p)
                    if p.segments.last().map(|s| s.value.as_str()) == Some("Target"))
        })
    }

    pub fn events(&self) -> Vec<Event> {
        self.events.borrow().clone()
    }

    /// Build a variant value using the file's declarations (for tests/callers).
    pub fn variant(&self, enum_name: &str, variant: &str, payload: Vec<Value>) -> EvalResult {
        let info = self
            .enums
            .get(enum_name)
            .ok_or_else(|| format!("unknown enum `{enum_name}`"))?;
        let (index, (name, shape)) = info
            .variants
            .iter()
            .enumerate()
            .find(|(_, (n, _))| n == variant)
            .ok_or_else(|| format!("unknown variant `{enum_name}::{variant}`"))?;
        let payload = match shape {
            VariantShape::Unit if payload.is_empty() => Payload::Unit,
            VariantShape::Tuple(n) if payload.len() == *n => Payload::Tuple(payload),
            _ => {
                return Err(format!(
                    "payload shape mismatch for `{enum_name}::{variant}`"
                ));
            }
        };
        Ok(Value::Variant {
            enum_name: enum_name.to_string(),
            index,
            name: name.clone(),
            payload,
        })
    }

    /// Call a top-level function with named arguments. This is the EDGE:
    /// demand enters here, so the result is deep-forced (every pending tree
    /// resolves) — inner evaluation stays lazy.
    pub fn call(&self, func: &str, args: &[(&str, Value)]) -> EvalResult {
        let given = args
            .iter()
            .map(|(n, v)| (Some(n.to_string()), v.clone()))
            .collect();
        let result = self.call_fn(func, given, false)?;
        self.deep_force_value(result)
    }

    /// Invoke a callable VALUE (closure, fn, partial) — including one that
    /// arrived over the wire via ship/receive. Also an edge: deep-forced.
    pub fn invoke(&self, callee: Value, args: Vec<Value>) -> EvalResult {
        let result = self.call_value(callee, args.into_iter().map(|v| (None, v)).collect())?;
        self.deep_force_value(result)
    }

    /// Deep force with event logging (the oracle-side twin of `deep_force`).
    fn deep_force_value(&self, value: Value) -> Result<Value, String> {
        deep_force_with(value, &|id| self.force_and_note(id))
    }

    // -- calls & memo --------------------------------------------------------

    fn call_fn(&self, func: &str, args: Vec<(Option<String>, Value)>, partial: bool) -> EvalResult {
        let f = self
            .fns
            .get(func)
            .ok_or_else(|| format!("unknown function `{func}`"))?;
        let params: Vec<&str> = f
            .params
            .params
            .iter()
            .map(|p| p.name.value.as_str())
            .collect();

        // Positional args fill params in order; kwargs fill by name.
        let mut bound: Vec<(String, Value)> = Vec::new();
        let mut positional = 0usize;
        for (name, value) in args {
            match name {
                Some(name) => {
                    if !params.contains(&name.as_str()) {
                        return Err(format!("`{func}` has no parameter `{name}`"));
                    }
                    if bound.iter().any(|(b, _)| b == &name) {
                        return Err(format!("`{func}` got duplicate argument `{name}`"));
                    }
                    bound.push((name, value));
                }
                None => {
                    let param = params
                        .iter()
                        .find(|p| {
                            !bound.iter().any(|(b, _)| b == **p)
                                && params.iter().position(|q| q == *p) >= Some(positional)
                        })
                        .ok_or_else(|| format!("too many arguments for `{func}`"))?;
                    if bound.iter().any(|(b, _)| b == *param) {
                        return Err(format!("`{func}` got duplicate argument `{param}`"));
                    }
                    positional += 1;
                    bound.push((param.to_string(), value));
                }
            }
        }

        let missing: Vec<&str> = params
            .iter()
            .filter(|p| !bound.iter().any(|(b, _)| b == **p))
            .copied()
            .collect();
        if !missing.is_empty() {
            if partial {
                // A partial application is a DECISION (`..`), not an accident.
                return Ok(Value::Partial {
                    func: func.to_string(),
                    given: bound,
                });
            }
            return Err(format!("`{func}` missing argument(s): {missing:?}"));
        }

        // Memo: canonical fn identity × canonical args. Aggregation unit v0.
        let mut h = DefaultHasher::new();
        for param in &params {
            let value = &bound.iter().find(|(b, _)| b == *param).unwrap().1;
            value.hash_into(&mut h);
        }
        let key = (self.fn_hashes[func], h.finish());
        let caller = self.fn_stack.borrow().last().cloned();
        let args: Vec<(String, String)> = params
            .iter()
            .map(|param| {
                let value = &bound.iter().find(|(b, _)| b == *param).unwrap().1;
                (param.to_string(), value.short())
            })
            .collect();
        if let Some(hit) = self.memo.borrow().get(&key) {
            self.emit(Event::Hit {
                func: func.to_string(),
                span: f.span,
                caller,
                args,
            });
            return Ok(hit.clone());
        }
        self.emit(Event::Miss {
            func: func.to_string(),
            span: f.span,
            caller,
            args,
        });

        let mut frames = vec![bound];
        self.fn_stack.borrow_mut().push(func.to_string());
        let result = self.block(&f.body, &mut frames);
        self.fn_stack.borrow_mut().pop();
        let result = result?;
        self.memo.borrow_mut().insert(key, result.clone());
        Ok(result)
    }

    fn call_value(&self, callee: Value, args: Vec<(Option<String>, Value)>) -> EvalResult {
        match callee {
            Value::Fn { name, .. } => self.call_fn(&name, args, false),
            Value::Partial { func, given } => {
                let mut merged: Vec<(Option<String>, Value)> =
                    given.into_iter().map(|(n, v)| (Some(n), v)).collect();
                merged.extend(args);
                self.call_fn(&func, merged, false)
            }
            Value::Closure {
                params, body, env, ..
            } => {
                if args.len() != params.len() {
                    return Err(format!(
                        "closure expects {} argument(s), got {}",
                        params.len(),
                        args.len()
                    ));
                }
                let mut frame: Frame = env;
                for (param, (_, value)) in params.iter().zip(args) {
                    frame.push((param.clone(), value));
                }
                let mut frames = vec![frame];
                self.eval(&body, &mut frames)
            }
            other => Err(format!("`{other:?}` is not callable")),
        }
    }

    // -- evaluation ----------------------------------------------------------

    fn block(&self, block: &Block, frames: &mut Vec<Frame>) -> EvalResult {
        frames.push(Vec::new());
        let mut result = Ok(Value::Tuple(Vec::new()));
        for stmt in &block.stmts {
            match stmt {
                Stmt::Let(l) => {
                    let value = match self.eval(&l.value, frames) {
                        Ok(v) => v,
                        e @ Err(_) => {
                            frames.pop();
                            return e;
                        }
                    };
                    frames
                        .last_mut()
                        .unwrap()
                        .push((l.name.value.clone(), value));
                }
                Stmt::Expr(e) => {
                    if let Err(err) = self.eval(&e.expr, frames) {
                        frames.pop();
                        return Err(err);
                    }
                }
            }
        }
        if let Some(tail) = &block.tail {
            result = self.eval(tail, frames);
        }
        frames.pop();
        result
    }

    fn lookup(&self, frames: &[Frame], name: &str) -> Option<Value> {
        for frame in frames.iter().rev() {
            if let Some((_, v)) = frame.iter().rev().find(|(n, _)| n == name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn eval(&self, expr: &Expr, frames: &mut Vec<Frame>) -> EvalResult {
        match expr {
            Expr::Identifier(name) => self.eval_name(name, frames),
            Expr::Number(n) => Ok(parse_number(&n.value)),
            Expr::Str(s) => Ok(Value::Str(s.value.clone())),
            Expr::Path(p) => Ok(Value::Path(p.value.clone())),
            Expr::Bool(b) => Ok(Value::Bool(b.value)),
            Expr::Paren(p) => self.eval(&p.inner, frames),
            Expr::Tuple(t) => Ok(Value::Tuple(
                t.elems
                    .iter()
                    .map(|e| self.eval(e, frames))
                    .collect::<Result<_, _>>()?,
            )),
            Expr::Array(a) => Ok(Value::Array(
                a.elems
                    .iter()
                    .map(|e| match e {
                        ArrayElem::Flag(f) => Ok(Value::Flag(f.value.clone())),
                        ArrayElem::Expr(e) => self.eval(e, frames),
                    })
                    .collect::<Result<_, _>>()?,
            )),
            Expr::Map(m) => {
                let mut map = BTreeMap::new();
                for entry in &m.entries {
                    let k = self.eval(&entry.key, frames)?;
                    let v = self.eval(&entry.value, frames)?;
                    map.insert(k, v);
                }
                Ok(Value::Map(map))
            }
            Expr::Unary(u) => {
                let v = self.eval(&u.operand, frames)?;
                match (u.op.as_str(), v) {
                    ("-", Value::Int(i)) => Ok(Value::Int(-i)),
                    ("-", Value::Float(f)) => Ok(Value::Float(-f)),
                    ("!", Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (op, v) => Err(format!("unary `{op}` not defined on {v:?}")),
                }
            }
            Expr::Binary(b) => self.binary(b, frames),
            Expr::Field(f) => {
                let recv = self.eval(&f.receiver, frames)?;
                match (&f.name, recv) {
                    (Member::Identifier(name), Value::Struct { fields, .. }) => fields
                        .iter()
                        .find(|(n, _)| n == &name.value)
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| format!("no field `{}`", name.value)),
                    (Member::Index(i), Value::Tuple(vs)) => {
                        let idx: usize = i.value.parse().map_err(|_| "bad tuple index")?;
                        vs.get(idx)
                            .cloned()
                            .ok_or_else(|| format!("tuple has no element {idx}"))
                    }
                    (m, recv) => Err(format!("cannot project {m:?} out of {recv:?}")),
                }
            }
            Expr::Call(c) => {
                let args = self.eval_args(&c.args, frames)?;
                let partial = c.args.args.iter().any(|a| matches!(a, Arg::Partial(_)));
                match &c.callee {
                    PathRef::Identifier(name) => {
                        if self.fns.contains_key(&name.value) {
                            self.call_fn(&name.value, args, partial)
                        } else if let Some(v) = self.lookup(frames, &name.value) {
                            self.call_value(v, args)
                        } else if name.value == "fetch" {
                            self.fetch(args)
                        } else if name.value == "extract" {
                            self.extract(args)
                        } else {
                            Err(format!("unknown callable `{}`", name.value))
                        }
                    }
                    PathRef::Scoped(s) => {
                        let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                        match segs.as_slice() {
                            [enum_name, variant] if self.enums.contains_key(*enum_name) => {
                                let payload = args.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
                                self.variant(enum_name, variant, payload)
                            }
                            [kind, "acquire"] => self.acquire(kind, args),
                            [ty, "new"] if *ty == "Map" => Ok(Value::Map(BTreeMap::new())),
                            other => Err(format!("unknown callable `{}`", other.join("::"))),
                        }
                    }
                }
            }
            Expr::MethodCall(m) => {
                let recv = self.eval(&m.receiver, frames)?;
                let args = self.eval_args(&m.args, frames)?;
                self.method(recv, &m.name.value, args)
            }
            Expr::Scoped(s) => {
                let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                match segs.as_slice() {
                    [enum_name, variant] if self.enums.contains_key(*enum_name) => {
                        self.variant(enum_name, variant, Vec::new())
                    }
                    other => Err(format!("unknown path `{}`", other.join("::"))),
                }
            }
            Expr::StructLit(lit) => self.struct_literal(lit, frames),
            Expr::Match(m) => {
                let scrutinee = self.eval(&m.scrutinee, frames)?;
                for arm in &m.arms {
                    if let Some(bindings) = self.pattern(&arm.pattern, &scrutinee, true)? {
                        frames.push(bindings);
                        let take = match &arm.guard {
                            Some(guard) => match self.eval(guard, frames)? {
                                Value::Bool(b) => b,
                                other => {
                                    frames.pop();
                                    return Err(format!("guard evaluated to {other:?}"));
                                }
                            },
                            None => true,
                        };
                        if take {
                            let out = self.eval(&arm.value, frames);
                            frames.pop();
                            return out;
                        }
                        frames.pop();
                    }
                }
                Err("no match arm matched".to_string())
            }
            Expr::Closure(c) => {
                // Capture the visible environment, flattened (inner shadows outer).
                let mut env: Frame = Vec::new();
                for frame in frames.iter() {
                    for (n, v) in frame {
                        env.retain(|(m, _)| m != n);
                        env.push((n.clone(), v.clone()));
                    }
                }
                // The stored body IS the canonical form (spans zeroed): the
                // closure's identity and its wire format are the same bytes.
                let mut body = c.body.clone();
                body.strip_spans();
                Ok(Value::Closure {
                    hash: canon_expr_hash(&body),
                    params: c.params.iter().map(|p| p.value.clone()).collect(),
                    body: Box::new(body),
                    env,
                })
            }
            Expr::Command(c) => self.command_block(c, frames),
        }
    }

    fn eval_name(&self, name: &Spanned<String>, frames: &[Frame]) -> EvalResult {
        if let Some(v) = self.lookup(frames, &name.value) {
            return Ok(v);
        }
        if let Some(hash) = self.fn_hashes.get(&name.value) {
            return Ok(Value::Fn {
                name: name.value.clone(),
                hash: *hash,
            });
        }
        if let Some(info) = self.structs.get(&name.value)
            && info.is_unit
        {
            return Ok(Value::Struct {
                name: name.value.clone(),
                fields: Vec::new(),
            });
        }
        Err(format!("unbound name `{}`", name.value))
    }

    fn eval_args(
        &self,
        args: &ast::ArgList,
        frames: &mut Vec<Frame>,
    ) -> Result<Vec<(Option<String>, Value)>, String> {
        let mut out = Vec::new();
        for arg in &args.args {
            match arg {
                Arg::Kwarg(k) => {
                    out.push((Some(k.name.value.clone()), self.eval(&k.value, frames)?))
                }
                Arg::Expr(e) => out.push((None, self.eval(e, frames)?)),
                Arg::Partial(_) => {}
            }
        }
        Ok(out)
    }

    fn binary(&self, b: &ast::Binary, frames: &mut Vec<Frame>) -> EvalResult {
        // Short-circuit forms first.
        if b.op == "&&" || b.op == "||" {
            let Value::Bool(left) = self.eval(&b.left, frames)? else {
                return Err("logical op on non-bool".to_string());
            };
            if (b.op == "&&") != left {
                return Ok(Value::Bool(left));
            }
            return self.eval(&b.right, frames);
        }
        let left = self.eval(&b.left, frames)?;
        let right = self.eval(&b.right, frames)?;
        match b.op.as_str() {
            "==" => Ok(Value::Bool(left == right)),
            "!=" => Ok(Value::Bool(left != right)),
            "<" => Ok(Value::Bool(left < right)),
            "<=" => Ok(Value::Bool(left <= right)),
            ">" => Ok(Value::Bool(left > right)),
            ">=" => Ok(Value::Bool(left >= right)),
            op => match (left, right) {
                (Value::Int(a), Value::Int(b)) => match op {
                    "+" => Ok(Value::Int(a + b)),
                    "-" => Ok(Value::Int(a - b)),
                    "*" => Ok(Value::Int(a * b)),
                    "/" => Ok(Value::Int(a / b)),
                    "%" => Ok(Value::Int(a % b)),
                    _ => Err(format!("unknown operator `{op}`")),
                },
                (Value::Float(a), Value::Float(b)) => match op {
                    "+" => Ok(Value::Float(a + b)),
                    "-" => Ok(Value::Float(a - b)),
                    "*" => Ok(Value::Float(a * b)),
                    "/" => Ok(Value::Float(a / b)),
                    "%" => Ok(Value::Float(a % b)),
                    _ => Err(format!("unknown operator `{op}`")),
                },
                // `/` joins paths; Int/Float never mix implicitly (strictness
                // is a design probe — the oracle reports, we decide).
                (Value::Path(a), Value::Path(b)) if op == "/" => {
                    Ok(Value::Path(format!("{a}/{b}")))
                }
                // Tree / Path = subtree selection: a directory re-rooted, or a
                // single file cut out (keyed by its basename).
                (Value::Tree(t), Value::Path(p)) if op == "/" => subtree(&t, &p).map(Value::Tree),
                // PROJECTION DOESN'T FORCE: cutting one path out of a tree
                // still being produced waits only for THAT path (the language-
                // level rmeta move). Directory cuts fall back to the flush.
                (Value::PendingTree { run }, Value::Path(p)) if op == "/" => {
                    // Projection doesn't FORCE — but it does SCHEDULE: the
                    // run must execute enough to serve the path.
                    self.note_scheduled(run);
                    let live = runs::get(run)
                        .ok_or_else(|| format!("pending run {run} not registered"))?;
                    match live.demand_path(&p) {
                        Ok(contents) => {
                            let base = p.rsplit_once('/').map(|(_, b)| b).unwrap_or(&p);
                            Ok(Value::Tree(crate::exec::Tree::of(&[(
                                base,
                                contents.as_str(),
                            )])))
                        }
                        Err(_) => subtree(&self.force_and_note(run)?, &p).map(Value::Tree),
                    }
                }
                (l, r) => Err(format!("`{op}` not defined on {l:?} and {r:?}")),
            },
        }
    }

    fn struct_literal(&self, lit: &ast::StructLiteral, frames: &mut Vec<Frame>) -> EvalResult {
        // `Enum::Variant { … }` (scoped path) constructs a record VARIANT.
        let name = match &lit.path {
            PathRef::Scoped(s) => {
                let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                let [enum_name, variant] = segs.as_slice() else {
                    return Err(format!("unsupported literal path `{}`", segs.join("::")));
                };
                return self.scoped_record_variant(enum_name, variant, lit, frames);
            }
            PathRef::Identifier(name) => name,
        };
        let Some(sinfo) = self.structs.get(&name.value) else {
            return Err(format!("`{}` is not a struct", name.value));
        };

        let mut given: Vec<(String, Value)> = Vec::new();
        for f in &lit.fields {
            given.push((f.name.value.clone(), self.eval(&f.value, frames)?));
        }
        let mut base: Option<Value> = None;
        for spread in &lit.spreads {
            match &spread.base {
                Some(expr) => base = Some(self.eval(expr, frames)?),
                None => return Err("partial struct construction: not in the oracle yet".into()),
            }
        }

        let mut fields = Vec::new();
        for (fname, default) in &sinfo.fields {
            let value = if let Some((_, v)) = given.iter().find(|(n, _)| n == fname) {
                v.clone()
            } else if let Value::Struct { fields: bf, .. } =
                base.as_ref().unwrap_or(&Value::Bool(false))
                && let Some((_, v)) = bf.iter().find(|(n, _)| n == fname)
            {
                v.clone()
            } else if let Some(default) = default {
                let mut fresh = vec![Vec::new()];
                self.eval(default, &mut fresh)?
            } else {
                return Err(format!("missing field `{fname}` on `{}`", name.value));
            };
            fields.push((fname.clone(), value));
        }
        Ok(Value::Struct {
            name: name.value.clone(),
            fields,
        })
    }

    /// `Enum::Variant { field: value, … }` — record-variant construction.
    fn scoped_record_variant(
        &self,
        enum_name: &str,
        variant: &str,
        lit: &ast::StructLiteral,
        frames: &mut Vec<Frame>,
    ) -> EvalResult {
        let info = self
            .enums
            .get(enum_name)
            .ok_or_else(|| format!("unknown enum `{enum_name}`"))?;
        let (index, (vname, shape)) = info
            .variants
            .iter()
            .enumerate()
            .find(|(_, (n, _))| n == variant)
            .ok_or_else(|| format!("unknown variant `{enum_name}::{variant}`"))?;
        let VariantShape::Record(field_names) = shape else {
            return Err(format!("`{enum_name}::{variant}` is not a record variant"));
        };
        let mut given: Vec<(String, Value)> = Vec::new();
        for f in &lit.fields {
            given.push((f.name.value.clone(), self.eval(&f.value, frames)?));
        }
        let mut fields = Vec::new();
        for fname in field_names {
            let value = given
                .iter()
                .find(|(n, _)| n == fname)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| format!("missing field `{fname}`"))?;
            fields.push((fname.clone(), value));
        }
        Ok(Value::Variant {
            enum_name: enum_name.to_string(),
            index,
            name: vname.clone(),
            payload: Payload::Record(fields),
        })
    }

    // -- patterns ------------------------------------------------------------

    /// Returns the bindings if the pattern matches. `top` = top of a match arm,
    /// where a bare identifier is TYPE-DIRECTED: if it names a variant of the
    /// scrutinee's enum it matches that variant; otherwise it binds. (The
    /// binder's static rule approximates this; the oracle does it exactly.)
    fn pattern(&self, p: &Pattern, v: &Value, top: bool) -> Result<Option<Frame>, String> {
        Ok(match (p, v) {
            (Pattern::Wildcard(_), _) => Some(Vec::new()),
            (Pattern::Str(s), Value::Str(x)) => (s.value == *x).then(Vec::new),
            (Pattern::Number(n), x) => (&parse_number(&n.value) == x).then(Vec::new),
            (Pattern::Identifier(name), scrutinee) => {
                if top && let Value::Variant { enum_name, .. } = scrutinee {
                    let is_variant = self
                        .enums
                        .get(enum_name)
                        .is_some_and(|e| e.variants.iter().any(|(n, _)| n == &name.value));
                    if is_variant {
                        return Ok(match scrutinee {
                            Value::Variant {
                                name: vn,
                                payload: Payload::Unit,
                                ..
                            } if *vn == name.value => Some(Vec::new()),
                            _ => None,
                        });
                    }
                }
                Some(vec![(name.value.clone(), scrutinee.clone())])
            }
            (
                Pattern::Scoped(s),
                Value::Variant {
                    enum_name,
                    name,
                    payload,
                    ..
                },
            ) => {
                let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                (segs == [enum_name.as_str(), name.as_str()] && matches!(payload, Payload::Unit))
                    .then(Vec::new)
            }
            (
                Pattern::Variant(vp),
                Value::Variant {
                    enum_name,
                    name,
                    payload,
                    ..
                },
            ) => {
                if !path_names_variant(&vp.path, enum_name, name) {
                    return Ok(None);
                }
                let Payload::Tuple(values) = payload else {
                    return Ok(None);
                };
                if vp.args.len() != values.len() {
                    return Ok(None);
                }
                let mut bindings = Vec::new();
                for (arg, value) in vp.args.iter().zip(values) {
                    match self.pattern(arg, value, false)? {
                        Some(inner) => bindings.extend(inner),
                        None => return Ok(None),
                    }
                }
                Some(bindings)
            }
            (Pattern::Struct(sp), value) => {
                let fields: &[(String, Value)] = match value {
                    Value::Variant {
                        enum_name,
                        name,
                        payload: Payload::Record(fields),
                        ..
                    } => {
                        if !path_names_variant(&sp.path, enum_name, name) {
                            return Ok(None);
                        }
                        fields
                    }
                    Value::Struct { name, fields } => {
                        if !path_names_struct(&sp.path, name) {
                            return Ok(None);
                        }
                        fields
                    }
                    _ => return Ok(None),
                };
                let mut bindings = Vec::new();
                for fp in &sp.fields {
                    let Some((_, value)) = fields.iter().find(|(n, _)| n == &fp.name.value) else {
                        return Ok(None);
                    };
                    match &fp.pattern {
                        Some(inner) => match self.pattern(inner, value, false)? {
                            Some(b) => bindings.extend(b),
                            None => return Ok(None),
                        },
                        None => bindings.push((fp.name.value.clone(), value.clone())),
                    }
                }
                if sp.rests.is_empty() && sp.fields.len() != fields.len() {
                    return Ok(None);
                }
                Some(bindings)
            }
            (Pattern::Tuple(tp), Value::Tuple(values)) => {
                if tp.elems.len() != values.len() {
                    return Ok(None);
                }
                let mut bindings = Vec::new();
                for (elem, value) in tp.elems.iter().zip(values) {
                    match self.pattern(elem, value, false)? {
                        Some(b) => bindings.extend(b),
                        None => return Ok(None),
                    }
                }
                Some(bindings)
            }
            _ => None,
        })
    }

    // -- command blocks: the exec seam, composed -----------------------------

    /// `cc! { -O2 {defines} -I {src} -c {src / unit} -o {out} }` — the command
    /// name resolves to a capability VALUE in scope (its fingerprint keys the
    /// cache); splices evaluate; Trees become MOUNTS at role-stable paths
    /// (/m/0, /m/1 — never content-derived, or tier-2 could never match);
    /// the toy role table stands in for the snark command grammar; the
    /// two-tier exec cache does the rest.
    fn command_block(&self, c: &ast::CommandBlock, frames: &mut Vec<Frame>) -> EvalResult {
        let capability = self
            .lookup(frames, &c.command.value)
            .ok_or_else(|| format!("no capability `{}` in scope", c.command.value))?;
        let cap_fp = capability.canon_hash();

        let mut argv: Vec<String> = Vec::new();
        let mut mounts: Vec<crate::exec::Mount> = Vec::new();
        for part in &c.parts {
            match part {
                CommandPart::Token(t) => argv.push(t.value.clone()),
                CommandPart::Splice(s) => {
                    // Splicing INTO a command is a mount (whole-tree use):
                    // pending trees force here. Note the asymmetry with
                    // `pending / path`, which only waits for the one path.
                    let v = self.eval(&s.expr, frames)?;
                    let v = self.force_value(v)?;
                    splice_into(v, &mut argv, &mut mounts)?;
                }
            }
        }

        let plan = assign_roles(&c.command.value, &argv)?;

        // DEMAND-DRIVEN: dispatch and return a PENDING tree immediately.
        // Evaluation continues; whoever projects a path waits only for that
        // path; whoever needs identity forces the flush.
        let run: std::sync::Arc<dyn PendingRun> = if let Some(backend) = &self.backend {
            backend.spawn(&c.command.value, &plan, cap_fp, &mounts)?
        } else {
            // The local cache is synchronous: the "pending" run is already
            // complete (the design oracle for caching; the wire is where
            // demand overlaps in time).
            let tool = tool_for(&c.command.value)?;
            let outcome = self
                .exec_cache
                .borrow_mut()
                .exec(&plan, cap_fp, &mounts, tool)?;
            let event = self
                .exec_cache
                .borrow()
                .events
                .last()
                .cloned()
                .expect("exec pushed an event");
            std::sync::Arc::new(LocalRun {
                outputs: outcome.outputs,
                event,
            })
        };
        let id = runs::register(run);
        self.unlogged_runs
            .borrow_mut()
            .insert(id, (c.command.value.clone(), c.span));
        self.emit(Event::Created {
            command: c.command.value.clone(),
            run: id,
            span: c.span,
            in_fn: self.fn_stack.borrow().last().cloned(),
            argv: argv.clone(),
            describe: crate::exec::describe(&c.command.value, &plan),
        });
        Ok(Value::PendingTree { run: id })
    }

    /// First demand on a run: emit Scheduled exactly once. Execution starts
    /// being PAID FOR here — a projection schedules just like a force.
    fn note_scheduled(&self, id: u64) {
        if !self.scheduled.borrow_mut().insert(id) {
            return;
        }
        let info = self.unlogged_runs.borrow().get(&id).cloned();
        if let Some((command, span)) = info {
            self.emit(Event::Scheduled {
                command,
                run: id,
                span,
            });
        }
    }

    /// Oracle-side force: flush the run and log its Finished event exactly
    /// once (identity demanded ⇒ the whole tree, so completion is knowable).
    fn force_and_note(&self, id: u64) -> Result<crate::exec::Tree, String> {
        self.note_scheduled(id);
        let (tree, event) = force_run(id)?;
        if let Some((command, span)) = self.unlogged_runs.borrow_mut().remove(&id) {
            let outputs = tree
                .entries
                .iter()
                .map(|(p, h)| (p.clone(), h.clone()))
                .collect();
            self.emit(Event::Finished {
                command,
                run: id,
                span,
                event,
                outputs,
            });
        }
        Ok(tree)
    }

    /// Whole-tree uses (methods, splices, merges) force pending trees.
    fn force_value(&self, value: Value) -> Result<Value, String> {
        match value {
            Value::PendingTree { run } => Ok(Value::Tree(self.force_and_note(run)?)),
            other => Ok(other),
        }
    }

    // -- primitives: observations pinned in the journal ----------------------

    fn observe(&self, key: String, produce: impl FnOnce() -> Value) -> Value {
        if let Some(pinned) = self.journal.borrow().get(&key) {
            self.emit(Event::Observation {
                key,
                replayed: true,
            });
            return pinned.clone();
        }
        let value = produce();
        self.journal.borrow_mut().insert(key.clone(), value.clone());
        self.emit(Event::Observation {
            key,
            replayed: false,
        });
        value
    }

    /// `fetch(url: …, sha256: …)` — pure BECAUSE the checksum is the identity.
    fn fetch(&self, args: Vec<(Option<String>, Value)>) -> EvalResult {
        let sha = args
            .iter()
            .find(|(n, _)| n.as_deref() == Some("sha256"))
            .map(|(_, v)| v.clone())
            .ok_or("fetch requires a sha256: the checksum IS the identity")?;
        let Value::Str(sha) = sha else {
            return Err("sha256 must be a string".to_string());
        };
        Ok(self.observe(format!("fetch:{sha}"), || {
            Value::Tree(crate::exec::Tree::of(&[("tarball", sha.as_str())]))
        }))
    }

    /// `extract(tar)` — pure function of the tarball's content. The oracle
    /// fabricates a lua-shaped source tree (deterministic in the tar hash) so
    /// the whole build pipeline has something real to chew.
    fn extract(&self, args: Vec<(Option<String>, Value)>) -> EvalResult {
        let Some((_, tar)) = args.first() else {
            return Err("extract takes a tree".to_string());
        };
        let tar_hash = tar.canon_hash();
        let header = format!("// lua.h api ({tar_hash:016x})");
        Ok(Value::Tree(crate::exec::Tree::of(&[
            ("lua-5.4.8/src/lua.h", header.as_str()),
            (
                "lua-5.4.8/src/lua.c",
                "#include \"lua.h\"\n// interpreter main",
            ),
            ("lua-5.4.8/src/lapi.c", "#include \"lua.h\"\n// api impl"),
            ("lua-5.4.8/src/lauxlib.c", "#include \"lua.h\"\n// aux lib"),
            (
                "lua-5.4.8/src/luac.c",
                "#include \"lua.h\"\n// compiler main",
            ),
        ])))
    }

    /// `Cc::acquire(target)` — capability acquisition IS an observation; the
    /// fingerprint rides into every closure that captures the value.
    fn acquire(&self, kind: &str, args: Vec<(Option<String>, Value)>) -> EvalResult {
        let target_hash = args.first().map(|(_, v)| v.canon_hash()).unwrap_or(0);
        let key = format!("acquire:{kind}:{target_hash:x}");
        Ok(self.observe(key.clone(), || Value::Struct {
            name: kind.to_string(),
            fields: vec![("fingerprint".to_string(), Value::Str(key.clone()))],
        }))
    }

    // -- builtin methods ------------------------------------------------------

    fn method(&self, recv: Value, name: &str, args: Vec<(Option<String>, Value)>) -> EvalResult {
        // Method calls are whole-value uses: a pending receiver forces.
        let recv = self.force_value(recv)?;
        let positional: Vec<Value> = args.into_iter().map(|(_, v)| v).collect();
        match (recv, name) {
            (Value::Map(m), "get") => {
                let [key] = positional.as_slice() else {
                    return Err("get takes one key".to_string());
                };
                Ok(match m.get(key) {
                    Some(v) => option_some(v.clone()),
                    None => option_none(),
                })
            }
            (Value::Map(mut m), "insert") => {
                let [k, v] = positional.as_slice() else {
                    return Err("insert takes key and value".to_string());
                };
                // Persistent semantics: insert returns a NEW map.
                m.insert(k.clone(), v.clone());
                Ok(Value::Map(m))
            }
            (
                Value::Variant {
                    enum_name,
                    name,
                    payload,
                    ..
                },
                "unwrap",
            ) if enum_name == "Option" => match (name.as_str(), payload) {
                ("Some", Payload::Tuple(mut vs)) => Ok(vs.remove(0)),
                _ => Err("unwrap on None".to_string()),
            },
            (Value::Array(vs), "map") => {
                let [f] = positional.as_slice() else {
                    return Err("map takes a function".to_string());
                };
                Ok(Value::Array(
                    vs.into_iter()
                        .map(|v| self.call_value(f.clone(), vec![(None, v)]))
                        .collect::<Result<_, _>>()?,
                ))
            }
            (Value::Array(vs), "filter") => {
                let [f] = positional.as_slice() else {
                    return Err("filter takes a predicate".to_string());
                };
                let mut out = Vec::new();
                for v in vs {
                    match self.call_value(f.clone(), vec![(None, v.clone())])? {
                        Value::Bool(true) => out.push(v),
                        Value::Bool(false) => {}
                        other => return Err(format!("predicate returned {other:?}")),
                    }
                }
                Ok(Value::Array(out))
            }
            // Collect is CANONICAL total order, never arrival order — and an
            // array of TREES collects into their merge (the aggregation that
            // makes `units.map(object).collect()` a Tree). Merging is a
            // whole-tree use: pending elements force HERE — which is the
            // demand-driven payoff: map spawned them all, collect awaits them
            // all (parallel fan-out, sequential-looking source).
            (Value::Array(mut vs), "collect") => {
                if !vs.is_empty()
                    && vs
                        .iter()
                        .all(|v| matches!(v, Value::Tree(_) | Value::PendingTree { .. }))
                {
                    let mut vs = vs
                        .into_iter()
                        .map(|v| self.force_value(v))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut merged = crate::exec::Tree::default();
                    vs.sort();
                    for v in vs {
                        let Value::Tree(t) = v else { unreachable!() };
                        for (path, contents) in t.entries {
                            if let Some(prior) = merged.entries.get(&path)
                                && *prior != contents
                            {
                                return Err(format!("collect: conflicting entry `{path}`"));
                            }
                            merged.entries.insert(path, contents);
                        }
                    }
                    return Ok(Value::Tree(merged));
                }
                vs.sort();
                Ok(Value::Array(vs))
            }
            (Value::Tree(t), "glob") => {
                let [Value::Str(pattern)] = positional.as_slice() else {
                    return Err("glob takes a pattern string".to_string());
                };
                let suffix = pattern
                    .strip_prefix('*')
                    .ok_or("glob v0 supports `*.ext` patterns")?;
                // Top level only, canonical (sorted) order.
                let mut paths: Vec<Value> = t
                    .entries
                    .keys()
                    .filter(|k| !k.contains('/') && k.ends_with(suffix))
                    .map(|k| Value::Path(k.clone()))
                    .collect();
                paths.sort();
                Ok(Value::Array(paths))
            }
            (Value::Path(p), "with_ext") => {
                let [Value::Str(ext)] = positional.as_slice() else {
                    return Err("with_ext takes a string".to_string());
                };
                let stem = p.rsplit_once('.').map(|(s, _)| s).unwrap_or(&p);
                Ok(Value::Path(format!("{stem}.{ext}")))
            }
            (recv, name) => Err(format!("no method `{name}` on {recv:?}")),
        }
    }
}

fn path_names_variant(path: &PathRef, enum_name: &str, variant: &str) -> bool {
    match path {
        PathRef::Identifier(n) => n.value == variant,
        PathRef::Scoped(s) => {
            let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
            segs == [enum_name, variant]
        }
    }
}

fn path_names_struct(path: &PathRef, name: &str) -> bool {
    matches!(path, PathRef::Identifier(n) if n.value == name)
}

/// Tree / Path: a directory re-rooted, or one file cut out by basename.
pub(crate) fn subtree(tree: &crate::exec::Tree, path: &str) -> Result<crate::exec::Tree, String> {
    if let Some(contents) = tree.entries.get(path) {
        let base = path.rsplit_once('/').map(|(_, b)| b).unwrap_or(path);
        return Ok(crate::exec::Tree::of(&[(base, contents.as_str())]));
    }
    let prefix = format!("{path}/");
    let entries: BTreeMap<String, String> = tree
        .entries
        .iter()
        .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|r| (r.to_string(), v.clone())))
        .collect();
    if entries.is_empty() {
        return Err(format!("no `{path}` in tree"));
    }
    Ok(crate::exec::Tree { entries })
}

/// Splice a value into a command: paths/strings/flags become argv text,
/// arrays flatten, and TREES become mounts. Mount paths are ROLE-STABLE
/// (/m/0, /m/1 by splice order): the plan is the identity, the world is what
/// varies — a content-derived path would leak the world into the identity
/// and tier-2 could never match. A single-file tree splices as the file's
/// path inside its mount; a directory splices as the mount root.
pub(crate) fn splice_into(
    value: Value,
    argv: &mut Vec<String>,
    mounts: &mut Vec<crate::exec::Mount>,
) -> Result<(), String> {
    match value {
        Value::Path(p) | Value::Str(p) | Value::Flag(p) => argv.push(p),
        Value::Int(i) => argv.push(i.to_string()),
        Value::Float(f) => argv.push(f.to_string()),
        Value::Array(vs) => {
            for v in vs {
                splice_into(v, argv, mounts)?;
            }
        }
        Value::Tree(t) => {
            let root = format!("/m/{}", mounts.len());
            let text = if t.entries.len() == 1 {
                format!("{root}/{}", t.entries.keys().next().unwrap())
            } else {
                root.clone()
            };
            mounts.push(crate::exec::Mount { at: root, tree: t });
            argv.push(text);
        }
        other => return Err(format!("cannot splice {other:?} into a command")),
    }
    Ok(())
}

/// The toy role tables — stand-ins for snark command grammars (which will
/// assign these roles from real grammar productions, versioned with the
/// capability).
pub(crate) fn assign_roles(
    command: &str,
    argv: &[String],
) -> Result<crate::exec::ExecPlan, String> {
    use crate::exec::Role;
    let mut out = Vec::new();
    match command {
        "cc" => {
            let mut prev: Option<&str> = None;
            for arg in argv {
                let role = match prev {
                    Some("-o") => Role::Output,
                    Some("-I") => Role::SearchDir,
                    _ if arg.starts_with("/m/") => Role::Input,
                    _ => Role::Flag,
                };
                out.push((arg.clone(), role));
                prev = Some(arg.as_str());
            }
        }
        "ar" => {
            for arg in argv {
                let role = if arg.starts_with("/m/") {
                    crate::exec::Role::Input
                } else if arg == "rcs" {
                    Role::Flag
                } else {
                    Role::Output
                };
                out.push((arg.clone(), role));
            }
        }
        "rustc" => {
            for arg in argv {
                let role = if arg.starts_with("/m/") {
                    Role::Input
                } else {
                    Role::Flag
                };
                out.push((arg.clone(), role));
            }
        }
        other => return Err(format!("no command grammar for `{other}`")),
    }
    Ok(crate::exec::ExecPlan { argv: out })
}

pub(crate) fn tool_for(command: &str) -> Result<&'static dyn crate::exec::Tool, String> {
    match command {
        "cc" => Ok(&crate::exec::FakeCc),
        "ar" => Ok(&crate::exec::FakeAr),
        other => Err(format!("no tool for `{other}`")),
    }
}

fn parse_number(text: &str) -> Value {
    if text.contains('.') {
        Value::Float(text.parse().unwrap_or(f64::NAN))
    } else {
        Value::Int(text.parse().unwrap_or(0))
    }
}

fn option_some(v: Value) -> Value {
    Value::Variant {
        enum_name: "Option".to_string(),
        index: 0,
        name: "Some".to_string(),
        payload: Payload::Tuple(vec![v]),
    }
}

fn option_none() -> Value {
    Value::Variant {
        enum_name: "Option".to_string(),
        index: 1,
        name: "None".to_string(),
        payload: Payload::Unit,
    }
}
