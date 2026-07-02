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

use crate::ast::{
    self, Arg, ArrayElem, Block, Expr, Item, Member, PathRef, Pattern, SourceFile, Stmt,
};
use crate::{VixParser, ast::Spanned};

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
    Fn { name: String, hash: u64 },
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
                Value::Struct { name: an, fields: af },
                Value::Struct { name: bn, fields: bf },
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
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical AST identity: the generated `strip_spans` zeroes every span
// (comments/whitespace never reach the AST; spans are the only position
// leak), then facet-postcard bytes ARE the content address — the same bytes
// that ship a closure to an executor. One canonical form for identity and
// transport.
// ---------------------------------------------------------------------------

fn canon_ast_hash(item: &ast::FnItem) -> u64 {
    let mut canonical = item.clone();
    canonical.strip_spans();
    let bytes = facet_postcard::to_vec(&canonical).expect("AST serializes");
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn canon_expr_hash(expr: &Expr) -> u64 {
    let bytes = facet_postcard::to_vec(expr).expect("AST serializes");
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Serialize a value for transport — the exec primitive's payload format.
pub fn ship(value: &Value) -> Result<Vec<u8>, String> {
    facet_postcard::to_vec(value).map_err(|e| format!("ship: {e}"))
}

/// Reconstitute a shipped value on the receiving side.
pub fn receive(bytes: &[u8]) -> Result<Value, String> {
    facet_postcard::from_slice(bytes).map_err(|e| format!("receive: {e}"))
}

// ---------------------------------------------------------------------------
// The oracle.
// ---------------------------------------------------------------------------

/// Observable evaluation events — the oracle's whole point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A memoized call ran (cold).
    Miss { func: String },
    /// A memoized call was served from cache.
    Hit { func: String },
    /// A primitive observed the outside world (cold) or replayed its pin.
    Observation { key: String, replayed: bool },
}

struct EnumInfo {
    variants: Vec<(String, VariantShape)>,
}

#[derive(Clone)]
enum VariantShape {
    Unit,
    Tuple(usize),
    Record(Vec<String>),
}

struct StructInfo {
    /// Field names in declaration order, with optional default exprs.
    fields: Vec<(String, Option<Expr>)>,
    is_unit: bool,
}

type Frame = Vec<(String, Value)>;

pub struct Oracle {
    fns: HashMap<String, ast::FnItem>,
    fn_hashes: HashMap<String, u64>,
    enums: HashMap<String, EnumInfo>,
    structs: HashMap<String, StructInfo>,
    memo: RefCell<HashMap<(u64, u64), Value>>,
    journal: RefCell<BTreeMap<String, Value>>,
    events: RefCell<Vec<Event>>,
}

type EvalResult = Result<Value, String>;

impl Oracle {
    pub fn load(source: &str) -> Result<Oracle, String> {
        let file: SourceFile = VixParser::new().parse(source).map_err(|e| e.message)?;
        let mut fns = HashMap::new();
        let mut fn_hashes = HashMap::new();
        let mut enums = HashMap::new();
        let mut structs = HashMap::new();
        for item in &file.items {
            match item {
                Item::Fn(f) => {
                    fn_hashes.insert(f.name.value.clone(), canon_ast_hash(f));
                    fns.insert(f.name.value.clone(), (**f).clone());
                }
                Item::Enum(e) => {
                    let variants = e
                        .variants
                        .iter()
                        .map(|v| {
                            let shape = if let Some(t) = &v.tuple {
                                VariantShape::Tuple(t.types.len())
                            } else if let Some(fl) = &v.fields {
                                VariantShape::Record(
                                    fl.fields.iter().map(|f| f.name.value.clone()).collect(),
                                )
                            } else {
                                VariantShape::Unit
                            };
                            (v.name.value.clone(), shape)
                        })
                        .collect();
                    enums.insert(e.name.value.clone(), EnumInfo { variants });
                }
                Item::Struct(s) => {
                    let fields = s
                        .fields
                        .iter()
                        .flat_map(|fl| &fl.fields)
                        .map(|f| (f.name.value.clone(), f.default.clone()))
                        .collect();
                    structs.insert(
                        s.name.value.clone(),
                        StructInfo {
                            fields,
                            is_unit: s.fields.is_none() && s.tuple.is_none(),
                        },
                    );
                }
                Item::Use(_) => {}
            }
        }
        Ok(Oracle {
            fns,
            fn_hashes,
            enums,
            structs,
            memo: RefCell::new(HashMap::new()),
            journal: RefCell::new(BTreeMap::new()),
            events: RefCell::new(Vec::new()),
        })
    }

    /// The canonical identity of a top-level function (AST modulo spans).
    pub fn fn_hash(&self, name: &str) -> Option<u64> {
        self.fn_hashes.get(name).copied()
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
            _ => return Err(format!("payload shape mismatch for `{enum_name}::{variant}`")),
        };
        Ok(Value::Variant {
            enum_name: enum_name.to_string(),
            index,
            name: name.clone(),
            payload,
        })
    }

    /// Call a top-level function with named arguments.
    pub fn call(&self, func: &str, args: &[(&str, Value)]) -> EvalResult {
        let given = args
            .iter()
            .map(|(n, v)| (Some(n.to_string()), v.clone()))
            .collect();
        self.call_fn(func, given, false)
    }

    /// Invoke a callable VALUE (closure, fn, partial) — including one that
    /// arrived over the wire via ship/receive.
    pub fn invoke(&self, callee: Value, args: Vec<Value>) -> EvalResult {
        self.call_value(callee, args.into_iter().map(|v| (None, v)).collect())
    }

    // -- calls & memo --------------------------------------------------------

    fn call_fn(
        &self,
        func: &str,
        args: Vec<(Option<String>, Value)>,
        partial: bool,
    ) -> EvalResult {
        let f = self
            .fns
            .get(func)
            .ok_or_else(|| format!("unknown function `{func}`"))?;
        let params: Vec<&str> = f.params.params.iter().map(|p| p.name.value.as_str()).collect();

        // Positional args fill params in order; kwargs fill by name.
        let mut bound: Vec<(String, Value)> = Vec::new();
        let mut positional = 0usize;
        for (name, value) in args {
            match name {
                Some(name) => {
                    if !params.contains(&name.as_str()) {
                        return Err(format!("`{func}` has no parameter `{name}`"));
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
        if let Some(hit) = self.memo.borrow().get(&key) {
            self.events.borrow_mut().push(Event::Hit {
                func: func.to_string(),
            });
            return Ok(hit.clone());
        }
        self.events.borrow_mut().push(Event::Miss {
            func: func.to_string(),
        });

        let mut frames = vec![bound];
        let result = self.block(&f.body, &mut frames)?;
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
                    frames.last_mut().unwrap().push((l.name.value.clone(), value));
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
                        let segs: Vec<&str> =
                            s.segments.iter().map(|x| x.value.as_str()).collect();
                        match segs.as_slice() {
                            [enum_name, variant] if self.enums.contains_key(*enum_name) => {
                                let payload =
                                    args.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
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
            Expr::Command(c) => Err(format!(
                "command blocks (`{}!`) need the exec layer — not in the oracle yet",
                c.command.value
            )),
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
                            Value::Variant { name: vn, payload: Payload::Unit, .. }
                                if *vn == name.value =>
                            {
                                Some(Vec::new())
                            }
                            _ => None,
                        });
                    }
                }
                Some(vec![(name.value.clone(), scrutinee.clone())])
            }
            (Pattern::Scoped(s), Value::Variant { enum_name, name, payload, .. }) => {
                let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                (segs == [enum_name.as_str(), name.as_str()]
                    && matches!(payload, Payload::Unit))
                .then(Vec::new)
            }
            (Pattern::Variant(vp), Value::Variant { enum_name, name, payload, .. }) => {
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
                    Value::Variant { enum_name, name, payload: Payload::Record(fields), .. } => {
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
                    let Some((_, value)) = fields.iter().find(|(n, _)| n == &fp.name.value)
                    else {
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

    // -- primitives: observations pinned in the journal ----------------------

    fn observe(&self, key: String, produce: impl FnOnce() -> Value) -> Value {
        if let Some(pinned) = self.journal.borrow().get(&key) {
            self.events.borrow_mut().push(Event::Observation {
                key,
                replayed: true,
            });
            return pinned.clone();
        }
        let value = produce();
        self.journal.borrow_mut().insert(key.clone(), value.clone());
        self.events.borrow_mut().push(Event::Observation {
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
            Value::Struct {
                name: "Tree".to_string(),
                fields: vec![("hash".to_string(), Value::Str(sha.clone()))],
            }
        }))
    }

    fn extract(&self, args: Vec<(Option<String>, Value)>) -> EvalResult {
        let Some((_, tree)) = args.first() else {
            return Err("extract takes a tree".to_string());
        };
        let hash = tree.canon_hash();
        Ok(Value::Struct {
            name: "Tree".to_string(),
            fields: vec![("hash".to_string(), Value::Str(format!("extract:{hash:x}")))],
        })
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

    fn method(
        &self,
        recv: Value,
        name: &str,
        args: Vec<(Option<String>, Value)>,
    ) -> EvalResult {
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
            (Value::Variant { enum_name, name, payload, .. }, "unwrap")
                if enum_name == "Option" =>
            {
                match (name.as_str(), payload) {
                    ("Some", Payload::Tuple(mut vs)) => Ok(vs.remove(0)),
                    _ => Err("unwrap on None".to_string()),
                }
            }
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
            // Collect is CANONICAL total order, never arrival order.
            (Value::Array(mut vs), "collect") => {
                vs.sort();
                Ok(Value::Array(vs))
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
