use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{DefaultHasher, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use crate::ast::{self, Arg, ArrayElem, Block, CommandPart, Expr, Member, PathRef, Pattern, Stmt};
use crate::exec::ExecCache;
use crate::fetch::{FetchBackend, NoFetchBackend};
use crate::module::{ModuleTables, VariantShape, load_module_tables};
use crate::oracle::{Event, LocalRun, PathDemand, PathMissing, Payload, PendingRun, Value};
use crate::oracle::{assign_roles, runs, splice_into, subtree, tool_for};
use crate::support::Spanned;

type NodeId = usize;

#[derive(Clone)]
enum NodeState {
    Thunk { expr: Rc<Expr>, env: Env },
    Project { subject: NodeId, path: String },
    Forced(Value),
    InProgress,
}

#[derive(Clone, Default)]
struct Env {
    frames: Rc<Vec<Vec<(String, NodeId)>>>,
}

impl Env {
    fn empty() -> Self {
        Self::default()
    }

    fn with_frame(&self) -> Self {
        let mut frames = (*self.frames).clone();
        frames.push(Vec::new());
        Self {
            frames: Rc::new(frames),
        }
    }

    fn with_binding(&self, name: String, node: NodeId) -> Self {
        let mut frames = (*self.frames).clone();
        if frames.is_empty() {
            frames.push(Vec::new());
        }
        frames.last_mut().unwrap().push((name, node));
        Self {
            frames: Rc::new(frames),
        }
    }

    fn lookup(&self, name: &str) -> Option<NodeId> {
        for frame in self.frames.iter().rev() {
            if let Some((_, node)) = frame.iter().rev().find(|(n, _)| n == name) {
                return Some(*node);
            }
        }
        None
    }

    fn visible_bindings(&self) -> Vec<(String, NodeId)> {
        let mut out: Vec<(String, NodeId)> = Vec::new();
        for frame in self.frames.iter() {
            for (name, node) in frame {
                out.retain(|(prior, _)| prior != name);
                out.push((name.clone(), *node));
            }
        }
        out
    }
}

#[derive(Clone)]
struct CallArg {
    name: Option<String>,
    node: NodeId,
}

#[derive(Clone)]
enum Demand {
    Identity,
    Path(String),
}

fn join_tree_path(base: &str, tail: &str) -> String {
    if base.is_empty() {
        tail.to_string()
    } else if tail.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{tail}")
    }
}

pub struct Engine {
    tables: ModuleTables,
    arena: Vec<NodeState>,
    memo: HashMap<(u64, u64), Value>,
    journal: BTreeMap<String, Value>,
    fetch_backend: Arc<dyn FetchBackend>,
    exec_cache: ExecCache,
    unlogged_runs: HashMap<u64, (String, crate::support::Span)>,
    scheduled: HashSet<u64>,
    events: Vec<Event>,
    fn_stack: Vec<String>,
}

type EvalResult = Result<Value, String>;

impl Engine {
    pub fn load(source: &str) -> Result<Engine, String> {
        Ok(Engine {
            tables: load_module_tables(source)?,
            arena: Vec::new(),
            memo: HashMap::new(),
            journal: BTreeMap::new(),
            fetch_backend: Arc::new(NoFetchBackend),
            exec_cache: ExecCache::new(),
            unlogged_runs: HashMap::new(),
            scheduled: HashSet::new(),
            events: Vec::new(),
            fn_stack: Vec::new(),
        })
    }

    /// The EDGE: demand enters here, then deep-forces with engine-side logging.
    pub fn call(&mut self, func: &str, args: &[(&str, Value)]) -> Result<Value, String> {
        let args = args
            .iter()
            .map(|(name, value)| CallArg {
                name: Some((*name).to_string()),
                node: self.alloc_forced(value.clone()),
            })
            .collect();
        let result = self.call_fn(func, args, false)?;
        self.deep_force_value(result)
    }

    pub fn events(&self) -> Vec<Event> {
        self.events.clone()
    }

    pub fn with_fetch_backend(mut self, backend: impl FetchBackend + 'static) -> Self {
        self.fetch_backend = Arc::new(backend);
        self
    }

    pub fn journal(&self) -> BTreeMap<String, Value> {
        self.journal.clone()
    }

    fn alloc_thunk(&mut self, expr: Expr, env: Env) -> NodeId {
        let node = self.arena.len();
        self.arena.push(NodeState::Thunk {
            expr: Rc::new(expr),
            env,
        });
        node
    }

    fn alloc_forced(&mut self, value: Value) -> NodeId {
        let node = self.arena.len();
        self.arena.push(NodeState::Forced(value));
        node
    }

    fn alloc_project(&mut self, subject: NodeId, path: String) -> NodeId {
        let node = self.arena.len();
        self.arena.push(NodeState::Project { subject, path });
        node
    }

    fn demand(&mut self, node: NodeId, kind: Demand) -> EvalResult {
        let state = std::mem::replace(&mut self.arena[node], NodeState::InProgress);
        match state {
            NodeState::Forced(value) => {
                let result = self.apply_demand(value.clone(), kind.clone())?;
                let stored = match kind {
                    Demand::Identity => result.clone(),
                    Demand::Path(_) => value,
                };
                self.arena[node] = NodeState::Forced(stored);
                Ok(result)
            }
            NodeState::Project { subject, path } => {
                let demanded = match &kind {
                    Demand::Identity => Demand::Path(path.clone()),
                    Demand::Path(tail) => Demand::Path(join_tree_path(&path, tail)),
                };
                let result = self.demand(subject, demanded);
                self.arena[node] = NodeState::Project { subject, path };
                result
            }
            NodeState::InProgress => {
                self.arena[node] = NodeState::InProgress;
                if let Some(func) = self.fn_stack.last() {
                    Err(format!("demand cycle in `{func}`"))
                } else {
                    Err("demand cycle".to_string())
                }
            }
            NodeState::Thunk { expr, env } => {
                if let Demand::Path(path) = &kind
                    && let Expr::Identifier(name) = expr.as_ref()
                    && let Some(target) = env.lookup(&name.value)
                {
                    let result = self.demand(target, Demand::Path(path.clone()));
                    self.arena[node] = NodeState::Thunk { expr, env };
                    return result;
                }
                if let Demand::Path(path) = &kind
                    && let Expr::Paren(paren) = expr.as_ref()
                {
                    let inner = self.alloc_thunk(paren.inner.clone(), env.clone());
                    let result = self.demand(inner, Demand::Path(path.clone()));
                    self.arena[node] = NodeState::Thunk { expr, env };
                    return result;
                }
                if let Demand::Path(tail) = &kind
                    && let Expr::Binary(binary) = expr.as_ref()
                    && binary.op == "/"
                {
                    let right = {
                        let right_node = self.alloc_thunk(binary.right.clone(), env.clone());
                        self.demand(right_node, Demand::Identity)
                    };
                    match right {
                        Ok(Value::Path(base)) => {
                            let subject = self.alloc_thunk(binary.left.clone(), env.clone());
                            let result =
                                self.demand(subject, Demand::Path(join_tree_path(&base, tail)));
                            self.arena[node] = NodeState::Thunk { expr, env };
                            return result;
                        }
                        Ok(_) => {}
                        Err(err) => {
                            self.arena[node] = NodeState::Thunk { expr, env };
                            return Err(err);
                        }
                    }
                }
                match self.eval(&expr, env.clone()) {
                    Ok(value) => {
                        let result = self.apply_demand(value.clone(), kind.clone())?;
                        let stored = match kind {
                            Demand::Identity => result.clone(),
                            Demand::Path(_) => value,
                        };
                        self.arena[node] = NodeState::Forced(stored);
                        Ok(result)
                    }
                    Err(err) => {
                        self.arena[node] = NodeState::Thunk { expr, env };
                        Err(err)
                    }
                }
            }
        }
    }

    fn emit(&mut self, event: Event) {
        self.events.push(event);
    }

    fn apply_demand(&mut self, value: Value, kind: Demand) -> EvalResult {
        match kind {
            Demand::Identity => self.deep_force_value(value),
            Demand::Path(path) => self.project_value(value, &path),
        }
    }

    fn project_value(&mut self, value: Value, path: &str) -> EvalResult {
        match value {
            Value::Tree(tree) => subtree(&tree, path).map(Value::Tree),
            Value::PendingTree { run } => {
                self.note_scheduled(run);
                let live =
                    runs::get(run).ok_or_else(|| format!("pending run {run} not registered"))?;
                match live.demand_path(path)? {
                    PathDemand::File(contents) => {
                        let base = path.rsplit_once('/').map(|(_, b)| b).unwrap_or(path);
                        Ok(Value::Tree(crate::exec::Tree::of(&[(
                            base,
                            contents.as_str(),
                        )])))
                    }
                    PathDemand::FinishRequired(_) => {
                        subtree(&self.force_and_note(run)?, path).map(Value::Tree)
                    }
                    PathDemand::Missing(missing) => {
                        let _ = self.force_and_note(run)?;
                        Err(missing.diagnostic())
                    }
                }
            }
            Value::MergedTree(values) => self.project_merged_tree(values, path),
            Value::Path(base) => Ok(Value::Path(format!("{base}/{path}"))),
            other => Ok(other),
        }
    }

    fn project_merged_tree(&mut self, values: Vec<Value>, path: &str) -> EvalResult {
        match self.project_merged_tree_candidate(values, path)? {
            Some(found) => Ok(found),
            None => Err(PathMissing {
                path: path.to_string(),
            }
            .diagnostic()),
        }
    }

    fn project_merged_tree_candidate(
        &mut self,
        values: Vec<Value>,
        path: &str,
    ) -> Result<Option<Value>, String> {
        for value in values.into_iter().rev() {
            match self.project_merge_candidate(value, path)? {
                Some(found) => return Ok(Some(found)),
                None => continue,
            }
        }
        Ok(None)
    }

    fn project_merge_candidate(
        &mut self,
        value: Value,
        path: &str,
    ) -> Result<Option<Value>, String> {
        match value {
            Value::Tree(tree) => match subtree(&tree, path) {
                Ok(tree) => Ok(Some(Value::Tree(tree))),
                Err(_) => Ok(None),
            },
            Value::PendingTree { run } => {
                self.note_scheduled(run);
                let live =
                    runs::get(run).ok_or_else(|| format!("pending run {run} not registered"))?;
                match live.demand_path(path)? {
                    PathDemand::File(contents) => {
                        let base = path.rsplit_once('/').map(|(_, b)| b).unwrap_or(path);
                        Ok(Some(Value::Tree(crate::exec::Tree::of(&[(
                            base,
                            contents.as_str(),
                        )]))))
                    }
                    PathDemand::FinishRequired(_) => subtree(&self.force_and_note(run)?, path)
                        .map(Value::Tree)
                        .map(Some),
                    PathDemand::Missing(_) => {
                        let _ = self.force_and_note(run)?;
                        Ok(None)
                    }
                }
            }
            Value::MergedTree(values) => self.project_merged_tree_candidate(values, path),
            other => Err(format!("collect: expected tree, got {other:?}")),
        }
    }

    fn deep_force_value(&mut self, value: Value) -> EvalResult {
        Ok(match value {
            Value::PendingTree { run } => Value::Tree(self.force_and_note(run)?),
            Value::MergedTree(values) => {
                let mut merged = crate::exec::Tree::default();
                for value in values {
                    let value = self.deep_force_value(value)?;
                    let Value::Tree(tree) = value else {
                        return Err(format!("collect: expected tree, got {value:?}"));
                    };
                    for (path, contents) in tree.entries {
                        merged.entries.insert(path, contents);
                    }
                }
                Value::Tree(merged)
            }
            Value::Tuple(values) => Value::Tuple(
                values
                    .into_iter()
                    .map(|value| self.deep_force_value(value))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|value| self.deep_force_value(value))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Map(map) => Value::Map(
                map.into_iter()
                    .map(|(key, value)| {
                        Ok::<_, String>((
                            self.deep_force_value(key)?,
                            self.deep_force_value(value)?,
                        ))
                    })
                    .collect::<Result<_, _>>()?,
            ),
            Value::Struct { name, fields } => Value::Struct {
                name,
                fields: fields
                    .into_iter()
                    .map(|(name, value)| Ok::<_, String>((name, self.deep_force_value(value)?)))
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
                    Payload::Tuple(values) => Payload::Tuple(
                        values
                            .into_iter()
                            .map(|value| self.deep_force_value(value))
                            .collect::<Result<_, _>>()?,
                    ),
                    Payload::Record(fields) => Payload::Record(
                        fields
                            .into_iter()
                            .map(|(name, value)| {
                                Ok::<_, String>((name, self.deep_force_value(value)?))
                            })
                            .collect::<Result<_, _>>()?,
                    ),
                },
            },
            other => other,
        })
    }

    fn note_scheduled(&mut self, id: u64) {
        if !self.scheduled.insert(id) {
            return;
        }
        if let Some((command, span)) = self.unlogged_runs.get(&id).cloned() {
            self.emit(Event::Scheduled {
                command,
                run: id,
                span,
            });
        }
    }

    fn force_and_note(&mut self, id: u64) -> Result<crate::exec::Tree, String> {
        self.note_scheduled(id);
        let (tree, event) = runs::get(id)
            .ok_or_else(|| format!("pending run {id} not in the registry"))?
            .flush()?;
        if let Some((command, span)) = self.unlogged_runs.remove(&id) {
            let outputs = tree
                .entries
                .iter()
                .map(|(path, contents)| (path.clone(), contents.clone()))
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

    fn force_value(&mut self, value: Value) -> EvalResult {
        self.deep_force_value(value)
    }

    fn call_fn(&mut self, func: &str, args: Vec<CallArg>, partial: bool) -> EvalResult {
        let f = self
            .tables
            .fns
            .get(func)
            .cloned()
            .ok_or_else(|| format!("unknown function `{func}`"))?;
        let params: Vec<String> = f
            .params
            .params
            .iter()
            .map(|p| p.name.value.clone())
            .collect();

        let mut bound: Vec<(String, NodeId)> = Vec::new();
        let mut positional = 0usize;
        for arg in args {
            match arg.name {
                Some(name) => {
                    if !params.iter().any(|p| p == &name) {
                        return Err(format!("`{func}` has no parameter `{name}`"));
                    }
                    if bound.iter().any(|(bound_name, _)| bound_name == &name) {
                        return Err(format!("`{func}` got duplicate argument `{name}`"));
                    }
                    bound.push((name, arg.node));
                }
                None => {
                    let param = params
                        .iter()
                        .find(|p| {
                            !bound.iter().any(|(b, _)| b == *p)
                                && params.iter().position(|q| q == *p) >= Some(positional)
                        })
                        .ok_or_else(|| format!("too many arguments for `{func}`"))?;
                    if bound.iter().any(|(bound_name, _)| bound_name == param) {
                        return Err(format!("`{func}` got duplicate argument `{param}`"));
                    }
                    positional += 1;
                    bound.push((param.clone(), arg.node));
                }
            }
        }

        let missing: Vec<&str> = params
            .iter()
            .filter(|p| !bound.iter().any(|(b, _)| b == *p))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            if partial {
                let mut given = Vec::new();
                for (name, node) in bound {
                    given.push((name, self.demand(node, Demand::Identity)?));
                }
                return Ok(Value::Partial {
                    func: func.to_string(),
                    given,
                });
            }
            return Err(format!("`{func}` missing argument(s): {missing:?}"));
        }

        let mut arg_values = Vec::new();
        let mut h = DefaultHasher::new();
        for param in &params {
            let node = bound.iter().find(|(b, _)| b == param).unwrap().1;
            let value = self.demand(node, Demand::Identity)?;
            value.hash_into(&mut h);
            arg_values.push((param.clone(), value));
        }
        let key = (self.tables.fn_hashes[func], h.finish());
        let caller = self.fn_stack.last().cloned();
        let event_args: Vec<(String, String)> = arg_values
            .iter()
            .map(|(param, value)| (param.clone(), value.short()))
            .collect();
        if let Some(value) = self.memo.get(&key).cloned() {
            self.emit(Event::Hit {
                func: func.to_string(),
                span: f.span,
                caller,
                args: event_args,
            });
            return Ok(value);
        }
        self.emit(Event::Miss {
            func: func.to_string(),
            span: f.span,
            caller,
            args: event_args,
        });

        let mut env = Env::empty().with_frame();
        for (name, node) in bound {
            env = env.with_binding(name, node);
        }
        self.fn_stack.push(func.to_string());
        let result = self.block(&f.body, env);
        self.fn_stack.pop();
        let result = result?;
        self.memo.insert(key, result.clone());
        Ok(result)
    }

    fn call_value(&mut self, callee: Value, args: Vec<CallArg>) -> EvalResult {
        match callee {
            Value::Fn { name, .. } => self.call_fn(&name, args, false),
            Value::Partial { func, given } => {
                let mut merged: Vec<CallArg> = given
                    .into_iter()
                    .map(|(name, value)| CallArg {
                        name: Some(name),
                        node: self.alloc_forced(value),
                    })
                    .collect();
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
                let mut closure_env = Env::empty().with_frame();
                for (name, value) in env {
                    let node = self.alloc_forced(value);
                    closure_env = closure_env.with_binding(name, node);
                }
                for (param, arg) in params.iter().zip(args) {
                    closure_env = closure_env.with_binding(param.clone(), arg.node);
                }
                self.eval(&body, closure_env)
            }
            other => Err(format!("`{other:?}` is not callable")),
        }
    }

    fn block(&mut self, block: &Block, env: Env) -> EvalResult {
        let mut env = env.with_frame();
        for stmt in &block.stmts {
            match stmt {
                Stmt::Let(l) => {
                    let node = self.alloc_thunk(l.value.clone(), env.clone());
                    env = env.with_binding(l.name.value.clone(), node);
                }
                Stmt::Expr(e) => {
                    self.eval(&e.expr, env.clone())?;
                }
            }
        }
        if let Some(tail) = &block.tail {
            self.eval(tail, env)
        } else {
            Ok(Value::Tuple(Vec::new()))
        }
    }

    fn eval(&mut self, expr: &Expr, env: Env) -> EvalResult {
        match expr {
            Expr::Identifier(name) => self.eval_name(name, &env),
            Expr::Number(n) => Ok(parse_number(&n.value)),
            Expr::Str(s) => Ok(Value::Str(s.value.clone())),
            Expr::Path(p) => Ok(Value::Path(p.value.clone())),
            Expr::Bool(b) => Ok(Value::Bool(b.value)),
            Expr::Paren(p) => {
                let node = self.alloc_thunk(p.inner.clone(), env);
                self.demand(node, Demand::Identity)
            }
            Expr::Tuple(t) => Ok(Value::Tuple(
                t.elems
                    .iter()
                    .map(|e| {
                        let node = self.alloc_thunk(e.clone(), env.clone());
                        self.demand(node, Demand::Identity)
                    })
                    .collect::<Result<_, _>>()?,
            )),
            Expr::Array(a) => Ok(Value::Array(
                a.elems
                    .iter()
                    .map(|e| match e {
                        ArrayElem::Flag(f) => Ok(Value::Flag(f.value.clone())),
                        ArrayElem::Expr(e) => {
                            let node = self.alloc_thunk(e.clone(), env.clone());
                            self.demand(node, Demand::Identity)
                        }
                    })
                    .collect::<Result<_, _>>()?,
            )),
            Expr::Map(m) => {
                let mut map = BTreeMap::new();
                for entry in &m.entries {
                    let key = {
                        let node = self.alloc_thunk(entry.key.clone(), env.clone());
                        self.demand(node, Demand::Identity)?
                    };
                    let value = {
                        let node = self.alloc_thunk(entry.value.clone(), env.clone());
                        self.demand(node, Demand::Identity)?
                    };
                    map.insert(key, value);
                }
                Ok(Value::Map(map))
            }
            Expr::Unary(u) => {
                let node = self.alloc_thunk(u.operand.clone(), env);
                let value = self.demand(node, Demand::Identity)?;
                match (u.op.as_str(), value) {
                    ("-", Value::Int(i)) => Ok(Value::Int(-i)),
                    ("-", Value::Float(f)) => Ok(Value::Float(-f)),
                    ("!", Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (op, v) => Err(format!("unary `{op}` not defined on {v:?}")),
                }
            }
            Expr::Binary(b) => self.binary(b, env),
            Expr::Field(f) => {
                let recv_node = self.alloc_thunk(f.receiver.clone(), env);
                let recv = self.demand(recv_node, Demand::Identity)?;
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
                let args = self.eval_args(&c.args, env.clone());
                let partial = c.args.args.iter().any(|a| matches!(a, Arg::Partial(_)));
                match &c.callee {
                    PathRef::Identifier(name) => {
                        if self.tables.fns.contains_key(&name.value) {
                            self.call_fn(&name.value, args, partial)
                        } else if let Some(node) = env.lookup(&name.value) {
                            let callee = self.demand(node, Demand::Identity)?;
                            self.call_value(callee, args)
                        } else if name.value == "fetch" {
                            self.fetch(args)
                        } else if name.value == "extract" {
                            self.extract(args)
                        } else if name.value == "toml" {
                            self.toml(args)
                        } else if name.value == "json" {
                            self.json(args)
                        } else {
                            Err(format!("unknown callable `{}`", name.value))
                        }
                    }
                    PathRef::Scoped(s) => {
                        let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                        match segs.as_slice() {
                            [enum_name, variant] if self.tables.enums.contains_key(*enum_name) => {
                                let payload = args
                                    .into_iter()
                                    .map(|arg| self.demand(arg.node, Demand::Identity))
                                    .collect::<Result<Vec<_>, _>>()?;
                                self.variant(enum_name, variant, payload)
                            }
                            [ty, "new"] if *ty == "Map" => Ok(Value::Map(BTreeMap::new())),
                            [kind, "acquire"] => self.acquire(kind, args),
                            other => Err(format!("unknown callable `{}`", other.join("::"))),
                        }
                    }
                }
            }
            Expr::MethodCall(m) => {
                let recv = if m.name.value == "collect" {
                    self.eval(&m.receiver, env.clone())?
                } else {
                    let recv_node = self.alloc_thunk(m.receiver.clone(), env.clone());
                    self.demand(recv_node, Demand::Identity)?
                };
                let args = self.eval_args(&m.args, env);
                self.method(recv, &m.name.value, args)
            }
            Expr::Scoped(s) => {
                let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                match segs.as_slice() {
                    [enum_name, variant] if self.tables.enums.contains_key(*enum_name) => {
                        self.variant(enum_name, variant, Vec::new())
                    }
                    other => Err(format!("unknown path `{}`", other.join("::"))),
                }
            }
            Expr::StructLit(lit) => self.struct_literal(lit, env),
            Expr::Match(m) => {
                let scrutinee_node = self.alloc_thunk(m.scrutinee.clone(), env.clone());
                let scrutinee = self.demand(scrutinee_node, Demand::Identity)?;
                for arm in &m.arms {
                    if let Some(bindings) = self.pattern(&arm.pattern, &scrutinee, true)? {
                        let mut arm_env = env.with_frame();
                        for (name, value) in bindings {
                            let node = self.alloc_forced(value);
                            arm_env = arm_env.with_binding(name, node);
                        }
                        let take = match &arm.guard {
                            Some(guard) => {
                                let guard_node = self.alloc_thunk(guard.clone(), arm_env.clone());
                                match self.demand(guard_node, Demand::Identity)? {
                                    Value::Bool(value) => value,
                                    other => return Err(format!("guard evaluated to {other:?}")),
                                }
                            }
                            None => true,
                        };
                        if take {
                            let value_node = self.alloc_thunk(arm.value.clone(), arm_env);
                            return self.demand(value_node, Demand::Identity);
                        }
                    }
                }
                Err("no match arm matched".to_string())
            }
            Expr::Closure(c) => {
                let mut captured = Vec::new();
                for (name, node) in env.visible_bindings() {
                    captured.push((name, self.demand(node, Demand::Identity)?));
                }
                let mut body = c.body.clone();
                body.strip_spans();
                Ok(Value::Closure {
                    hash: canon_expr_hash(&body),
                    params: c.params.iter().map(|p| p.value.clone()).collect(),
                    body: Box::new(body),
                    env: captured,
                })
            }
            Expr::Command(c) => self.command_block(c, env),
        }
    }

    fn eval_name(&mut self, name: &Spanned<String>, env: &Env) -> EvalResult {
        if let Some(node) = env.lookup(&name.value) {
            return self.demand(node, Demand::Identity);
        }
        if let Some(hash) = self.tables.fn_hashes.get(&name.value) {
            return Ok(Value::Fn {
                name: name.value.clone(),
                hash: *hash,
            });
        }
        if let Some(info) = self.tables.structs.get(&name.value)
            && info.is_unit
        {
            return Ok(Value::Struct {
                name: name.value.clone(),
                fields: Vec::new(),
            });
        }
        Err(format!("unbound name `{}`", name.value))
    }

    fn eval_args(&mut self, args: &ast::ArgList, env: Env) -> Vec<CallArg> {
        let mut out = Vec::new();
        for arg in &args.args {
            match arg {
                Arg::Kwarg(k) => out.push(CallArg {
                    name: Some(k.name.value.clone()),
                    node: self.alloc_thunk(k.value.clone(), env.clone()),
                }),
                Arg::Expr(e) => out.push(CallArg {
                    name: None,
                    node: self.alloc_thunk(e.clone(), env.clone()),
                }),
                Arg::Partial(_) => {}
            }
        }
        out
    }

    fn binary(&mut self, b: &ast::Binary, env: Env) -> EvalResult {
        if b.op == "&&" || b.op == "||" {
            let left_node = self.alloc_thunk(b.left.clone(), env.clone());
            let Value::Bool(left) = self.demand(left_node, Demand::Identity)? else {
                return Err("logical op on non-bool".to_string());
            };
            if (b.op == "&&") != left {
                return Ok(Value::Bool(left));
            }
            let right_node = self.alloc_thunk(b.right.clone(), env);
            return self.demand(right_node, Demand::Identity);
        }
        if b.op == "/" {
            let right = {
                let node = self.alloc_thunk(b.right.clone(), env.clone());
                self.demand(node, Demand::Identity)?
            };
            if let Value::Path(path) = right {
                let subject = self.alloc_thunk(b.left.clone(), env);
                let project = self.alloc_project(subject, path);
                return self.demand(project, Demand::Identity);
            }
            let left = {
                let node = self.alloc_thunk(b.left.clone(), env);
                self.demand(node, Demand::Identity)?
            };
            return match (left, right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
                (left, right) => Err(format!("`/` not defined on {left:?} and {right:?}")),
            };
        }
        let left = {
            let node = self.alloc_thunk(b.left.clone(), env.clone());
            self.demand(node, Demand::Identity)?
        };
        let right = {
            let node = self.alloc_thunk(b.right.clone(), env);
            self.demand(node, Demand::Identity)?
        };
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
                (Value::Path(a), Value::Path(b)) if op == "/" => {
                    Ok(Value::Path(format!("{a}/{b}")))
                }
                (Value::Tree(t), Value::Path(p)) if op == "/" => subtree(&t, &p).map(Value::Tree),
                (l, r) => Err(format!("`{op}` not defined on {l:?} and {r:?}")),
            },
        }
    }

    fn struct_literal(&mut self, lit: &ast::StructLiteral, env: Env) -> EvalResult {
        let name = match &lit.path {
            PathRef::Scoped(s) => {
                let segs: Vec<&str> = s.segments.iter().map(|x| x.value.as_str()).collect();
                let [enum_name, variant] = segs.as_slice() else {
                    return Err(format!("unsupported literal path `{}`", segs.join("::")));
                };
                return self.scoped_record_variant(enum_name, variant, lit, env);
            }
            PathRef::Identifier(name) => name,
        };
        let sinfo = self
            .tables
            .structs
            .get(&name.value)
            .cloned()
            .ok_or_else(|| format!("`{}` is not a struct", name.value))?;

        // v1 chunk choice: struct construction is strict in field values; the
        // per-field node representation is deferred to the tree-aware engine.
        let mut given: Vec<(String, Value)> = Vec::new();
        for field in &lit.fields {
            let node = self.alloc_thunk(field.value.clone(), env.clone());
            given.push((
                field.name.value.clone(),
                self.demand(node, Demand::Identity)?,
            ));
        }
        let mut base: Option<Value> = None;
        for spread in &lit.spreads {
            match &spread.base {
                Some(expr) => {
                    let node = self.alloc_thunk(expr.clone(), env.clone());
                    base = Some(self.demand(node, Demand::Identity)?);
                }
                None => return Err("partial struct construction: not in the engine yet".into()),
            }
        }

        let mut fields = Vec::new();
        for (fname, default) in &sinfo.fields {
            let value = if let Some((_, value)) = given.iter().find(|(n, _)| n == fname) {
                value.clone()
            } else if let Value::Struct { fields: bf, .. } =
                base.as_ref().unwrap_or(&Value::Bool(false))
                && let Some((_, value)) = bf.iter().find(|(n, _)| n == fname)
            {
                value.clone()
            } else if let Some(default) = default {
                let node = self.alloc_thunk(default.clone(), Env::empty().with_frame());
                self.demand(node, Demand::Identity)?
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

    fn scoped_record_variant(
        &mut self,
        enum_name: &str,
        variant: &str,
        lit: &ast::StructLiteral,
        env: Env,
    ) -> EvalResult {
        let info = self
            .tables
            .enums
            .get(enum_name)
            .cloned()
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
        for field in &lit.fields {
            let node = self.alloc_thunk(field.value.clone(), env.clone());
            given.push((
                field.name.value.clone(),
                self.demand(node, Demand::Identity)?,
            ));
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

    fn variant(&self, enum_name: &str, variant: &str, payload: Vec<Value>) -> EvalResult {
        let info = self
            .tables
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

    fn pattern(
        &self,
        p: &Pattern,
        v: &Value,
        top: bool,
    ) -> Result<Option<Vec<(String, Value)>>, String> {
        Ok(match (p, v) {
            (Pattern::Wildcard(_), _) => Some(Vec::new()),
            (Pattern::Str(s), Value::Str(x)) => (s.value == *x).then(Vec::new),
            (Pattern::Number(n), x) => (&parse_number(&n.value) == x).then(Vec::new),
            (Pattern::Identifier(name), scrutinee) => {
                if top && let Value::Variant { enum_name, .. } = scrutinee {
                    let is_variant = self
                        .tables
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

    fn command_block(&mut self, c: &ast::CommandBlock, env: Env) -> EvalResult {
        let capability_node = env
            .lookup(&c.command.value)
            .ok_or_else(|| format!("no capability `{}` in scope", c.command.value))?;
        let capability = self.demand(capability_node, Demand::Identity)?;
        let cap_fp = capability.canon_hash();

        let mut argv = Vec::new();
        let mut mounts = Vec::new();
        for part in &c.parts {
            match part {
                CommandPart::Token(token) => argv.push(token.value.clone()),
                CommandPart::Splice(splice) => {
                    let node = self.alloc_thunk(splice.expr.clone(), env.clone());
                    let value = self.demand(node, Demand::Identity)?;
                    splice_into(value, &mut argv, &mut mounts)?;
                }
            }
        }

        let plan = assign_roles(&c.command.value, &argv)?;
        let tool = tool_for(&c.command.value)?;
        let outcome = self.exec_cache.exec(&plan, cap_fp, &mounts, tool)?;
        let event = self
            .exec_cache
            .events
            .last()
            .cloned()
            .expect("exec pushed an event");
        let run: Arc<dyn PendingRun> = Arc::new(LocalRun {
            outputs: outcome.outputs,
            event,
        });
        let id = runs::register(run);
        self.unlogged_runs
            .insert(id, (c.command.value.clone(), c.span));
        self.emit(Event::Created {
            command: c.command.value.clone(),
            run: id,
            span: c.span,
            in_fn: self.fn_stack.last().cloned(),
            argv: argv.clone(),
            describe: crate::exec::describe(&c.command.value, &plan),
        });
        Ok(Value::PendingTree { run: id })
    }

    fn observe(&mut self, key: String, produce: impl FnOnce() -> Value) -> Value {
        if let Some(pinned) = self.journal.get(&key).cloned() {
            self.emit(Event::Observation {
                key,
                replayed: true,
            });
            return pinned;
        }
        let value = produce();
        self.journal.insert(key.clone(), value.clone());
        self.emit(Event::Observation {
            key,
            replayed: false,
        });
        value
    }

    fn fetch(&mut self, args: Vec<CallArg>) -> EvalResult {
        let mut url = None;
        let mut sha256 = None;
        for arg in args {
            let value = self.demand(arg.node, Demand::Identity)?;
            match arg.name.as_deref() {
                Some("url") => {
                    let Value::Str(value) = value else {
                        return Err("fetch url must be a string".to_string());
                    };
                    url = Some(value);
                }
                Some("sha256") => {
                    let Value::Str(value) = value else {
                        return Err("fetch sha256 must be a string".to_string());
                    };
                    sha256 = Some(value);
                }
                Some(name) => return Err(format!("fetch got unknown argument `{name}`")),
                None => return Err("fetch arguments must be named".to_string()),
            }
        }
        let url = url.ok_or_else(|| "fetch requires a url".to_string())?;
        let (value, observation) =
            crate::fetch::fetch_value(&mut self.journal, self.fetch_backend.as_ref(), url, sha256)?;
        self.emit(Event::Observation {
            key: observation.key,
            replayed: observation.replayed,
        });
        Ok(value)
    }

    fn extract(&mut self, args: Vec<CallArg>) -> EvalResult {
        let Some(arg) = args.first() else {
            return Err("extract takes a tree".to_string());
        };
        self.demand(arg.node, Demand::Identity)
    }

    fn toml(&mut self, args: Vec<CallArg>) -> EvalResult {
        let [arg] = args.as_slice() else {
            return Err("toml takes one string or single-blob tree".to_string());
        };
        let input = self.demand(arg.node, Demand::Identity)?;
        crate::data::parse_toml(input)
    }

    fn json(&mut self, args: Vec<CallArg>) -> EvalResult {
        let [arg] = args.as_slice() else {
            return Err("json takes one string or single-blob tree".to_string());
        };
        let input = self.demand(arg.node, Demand::Identity)?;
        crate::data::parse_json(input)
    }

    fn acquire(&mut self, kind: &str, args: Vec<CallArg>) -> EvalResult {
        let target_hash = if let Some(arg) = args.first() {
            self.demand(arg.node, Demand::Identity)?.canon_hash()
        } else {
            0
        };
        let key = format!("acquire:{kind}:{target_hash:x}");
        Ok(self.observe(key.clone(), || Value::Struct {
            name: kind.to_string(),
            fields: vec![("fingerprint".to_string(), Value::Str(key.clone()))],
        }))
    }

    fn method(&mut self, recv: Value, name: &str, args: Vec<CallArg>) -> EvalResult {
        if name == "collect" {
            let positional = args
                .into_iter()
                .map(|arg| self.demand(arg.node, Demand::Identity))
                .collect::<Result<Vec<_>, _>>()?;
            if !positional.is_empty() {
                return Err("collect takes no arguments".to_string());
            }
            return self.collect_method(recv);
        }

        let recv = self.force_value(recv)?;
        let positional = args
            .into_iter()
            .map(|arg| self.demand(arg.node, Demand::Identity))
            .collect::<Result<Vec<_>, _>>()?;
        match (recv, name) {
            (Value::Map(map), "get") => {
                let [key] = positional.as_slice() else {
                    return Err("get takes one key".to_string());
                };
                Ok(match map.get(key) {
                    Some(value) => option_some(value.clone()),
                    None => option_none(),
                })
            }
            (Value::Map(mut map), "insert") => {
                let [key, value] = positional.as_slice() else {
                    return Err("insert takes key and value".to_string());
                };
                map.insert(key.clone(), value.clone());
                Ok(Value::Map(map))
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
                ("Some", Payload::Tuple(mut values)) => Ok(values.remove(0)),
                _ => Err("unwrap on None".to_string()),
            },
            (Value::Array(values), "map") => {
                let [func] = positional.as_slice() else {
                    return Err("map takes a function".to_string());
                };
                Ok(Value::Array(
                    values
                        .into_iter()
                        .map(|value| {
                            let node = self.alloc_forced(value);
                            self.call_value(func.clone(), vec![CallArg { name: None, node }])
                        })
                        .collect::<Result<_, _>>()?,
                ))
            }
            (Value::Array(values), "filter") => {
                let [func] = positional.as_slice() else {
                    return Err("filter takes a predicate".to_string());
                };
                let mut out = Vec::new();
                for value in values {
                    let node = self.alloc_forced(value.clone());
                    match self.call_value(func.clone(), vec![CallArg { name: None, node }])? {
                        Value::Bool(true) => out.push(value),
                        Value::Bool(false) => {}
                        other => return Err(format!("predicate returned {other:?}")),
                    }
                }
                Ok(Value::Array(out))
            }
            (Value::Tree(t), "glob") => {
                let [Value::Str(pattern)] = positional.as_slice() else {
                    return Err("glob takes a pattern string".to_string());
                };
                let suffix = pattern
                    .strip_prefix('*')
                    .ok_or("glob v0 supports `*.ext` patterns")?;
                let mut paths: Vec<Value> = t
                    .entries
                    .keys()
                    .filter(|k| !k.contains('/') && k.ends_with(suffix))
                    .map(|k| Value::Path(k.clone()))
                    .collect();
                paths.sort();
                Ok(Value::Array(paths))
            }
            (Value::Path(path), "with_ext") => {
                let [Value::Str(ext)] = positional.as_slice() else {
                    return Err("with_ext takes a string".to_string());
                };
                let stem = path.rsplit_once('.').map(|(s, _)| s).unwrap_or(&path);
                Ok(Value::Path(format!("{stem}.{ext}")))
            }
            (recv, name) => Err(format!("no method `{name}` on {recv:?}")),
        }
    }

    fn collect_method(&mut self, recv: Value) -> EvalResult {
        match recv {
            Value::Array(mut values) => {
                if !values.is_empty()
                    && values.iter().all(|value| {
                        matches!(
                            value,
                            Value::Tree(_) | Value::PendingTree { .. } | Value::MergedTree(_)
                        )
                    })
                {
                    return Ok(Value::MergedTree(values));
                }
                values.sort();
                Ok(Value::Array(values))
            }
            other => Err(format!("no method `collect` on {other:?}")),
        }
    }
}

fn canon_expr_hash(expr: &Expr) -> u64 {
    let bytes = phon::api::encode(expr).expect("AST serializes");
    let mut h = DefaultHasher::new();
    std::hash::Hash::hash(&bytes, &mut h);
    h.finish()
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

fn option_some(value: Value) -> Value {
    Value::Variant {
        enum_name: "Option".to_string(),
        index: 0,
        name: "Some".to_string(),
        payload: Payload::Tuple(vec![value]),
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
