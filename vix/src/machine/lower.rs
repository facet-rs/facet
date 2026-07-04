//! The AST lowering — vix functions become task programs (lowering
//! constitution: vixen repo, docs/design/machine-lowering.md).
//!
//! Slice-1 subset, honestly bounded: scalar Int functions, parameters,
//! lets, `+ - *`, parens, and CALLS — every user-function call is a
//! MEMO BOUNDARY lowered to the INVOKE protocol (write [slot, fn,
//! argc, args...] into the frame's invoke region, HostCall(INVOKE),
//! Await(slot)). Anything outside the subset is a loud error, never a
//! silent approximation. Floats, conds, composites arrive with their
//! slices.
//!
//! Memo identity is right from day one: the memo key's function
//! component is the CLOSURE HASH from the module tables (canonical
//! AST of the fn plus everything it references, code and types,
//! transitively) — trivia edits preserve it, semantic edits change
//! exactly the affected closures.

use std::collections::{BTreeSet, HashMap};

use weavy::mem::Layout;
use weavy::task::{Fn as TaskFn, FnId, Op, Program};

use super::TotalF64;
use super::driver::{
    ACQUIRE_HOST, ARRAY_ALLOC_HOST, ARRAY_COLLECT_HOST, ARRAY_MAP_PENDING_HOST, DriveEvent, Driver,
    EXEC_HOST, INVOKE_HOST, Lane, LoweredFn, MAP_EMPTY_HOST, MAP_GET_HOST, MAP_INSERT_HOST,
    OPTION_UNWRAP_HOST, PATH_WITH_EXT_HOST, STORE_ALLOC_HOST, STORE_READ_HOST, STORE_TAG_HOST,
    TREE_PROJECT_HOST,
};
use crate::ast;
use crate::module::{ModuleTables, VariantShape, load_module_tables, type_schema_name};

/// The machine facade for this slice: load source, demand a function's
/// value at the edge.
pub struct Machine {
    driver: Driver,
    fn_refs: HashMap<String, usize>,
}

impl Machine {
    pub fn load(source: &str) -> Result<Machine, String> {
        Self::load_with_lane(source, Lane::Interp)
    }

    pub fn load_with_lane(source: &str, lane: Lane) -> Result<Machine, String> {
        let tables = load_module_tables(source)?;

        // Deterministic fn_ref assignment: sorted names.
        let mut names: Vec<&String> = tables.fns.keys().collect();
        names.sort();
        let fn_refs: HashMap<String, usize> = names
            .iter()
            .enumerate()
            .map(|(ix, name)| ((*name).clone(), ix))
            .collect();
        let fn_returns: HashMap<String, String> = names
            .iter()
            .map(|name| {
                let item = &tables.fns[*name];
                let schema = item
                    .return_type
                    .as_ref()
                    .map(type_schema_name)
                    .transpose()?
                    .unwrap_or_else(|| "Int".into());
                Ok(((*name).clone(), schema))
            })
            .collect::<Result<_, String>>()?;
        let fn_params: HashMap<String, Vec<String>> = names
            .iter()
            .map(|name| {
                let item = &tables.fns[*name];
                Ok((
                    (*name).clone(),
                    item.params
                        .params
                        .iter()
                        .map(|param| type_schema_name(&param.ty))
                        .collect::<Result<Vec<_>, _>>()?,
                ))
            })
            .collect::<Result<_, String>>()?;
        let mut schema_names: Vec<String> = tables.descriptors.keys().cloned().collect();
        schema_names.extend([
            "String".to_string(),
            "Path".to_string(),
            "Target".to_string(),
            "Cc".to_string(),
            "Tree".to_string(),
            "Array".to_string(),
            "Map".to_string(),
        ]);
        for schema in fn_returns.values() {
            schema_names.push(schema.clone());
            if let Some(value_schema) = map_value_schema(schema) {
                schema_names.push(value_schema.to_string());
            }
        }
        for schemas in fn_params.values() {
            for schema in schemas {
                schema_names.push(schema.clone());
                if let Some(value_schema) = map_value_schema(schema) {
                    schema_names.push(value_schema.to_string());
                }
            }
        }
        for item in tables.fns.values() {
            collect_block_type_schemas(&item.body, &mut schema_names)?;
        }
        schema_names.sort();
        schema_names.dedup();
        let schema_refs: HashMap<String, i64> = schema_names
            .iter()
            .enumerate()
            .map(|(ix, name)| {
                (
                    name.clone(),
                    i64::try_from(ix).expect("schema ref fits i64"),
                )
            })
            .collect();
        let string_handles = string_handles(&tables);
        let path_handles = path_handles(&tables, string_handles.len());
        let literal_handles = LiteralHandles {
            strings: &string_handles,
            paths: &path_handles,
        };

        let mut task_fns = Vec::with_capacity(names.len());
        let mut lowered = Vec::with_capacity(names.len());
        for (ix, name) in names.iter().enumerate() {
            let item = &tables.fns[*name];
            let hash = tables.fn_hashes[*name];
            let (task_fn, info) = FnLowerer::lower(
                item,
                &tables,
                &fn_refs,
                &fn_returns,
                &fn_params,
                &schema_refs,
                literal_handles,
            )
            .map_err(|e| format!("lowering {name}: {e}"))?;
            task_fns.push(task_fn);
            lowered.push(LoweredFn {
                hash,
                task_fn: FnId(u32::try_from(ix).expect("fn count fits u32")),
                arg_offsets: info.arg_offsets,
                arg_schemas: item
                    .params
                    .params
                    .iter()
                    .map(|param| type_schema_name(&param.ty))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("lowering {name}: {e}"))?,
                return_schema: fn_returns[*name].clone(),
                invoke_region: info.invoke_region,
                store_alloc_region: info.store_alloc_region,
                store_read_region: info.store_read_region,
                store_tag_region: info.store_tag_region,
                primitive_region: info.primitive_region,
            });
        }

        let mut driver = Driver::try_with_descriptors(
            Program { fns: task_fns },
            lowered,
            tables.descriptors,
            lane,
        )?;
        for name in &schema_names {
            let actual = driver.intern_schema_ref(name.clone());
            assert_eq!(
                actual, schema_refs[name],
                "schema ref assignment is deterministic"
            );
        }
        let mut string_handles_sorted: Vec<(&String, &i64)> = string_handles.iter().collect();
        string_handles_sorted.sort_by_key(|(_, handle)| **handle);
        for (value, expected) in string_handles_sorted {
            let (actual, _) = driver.intern_raw_value("String", value.as_bytes().to_vec());
            assert_eq!(
                actual, *expected,
                "string handle assignment is deterministic"
            );
        }
        let mut path_handles_sorted: Vec<(&String, &i64)> = path_handles.iter().collect();
        path_handles_sorted.sort_by_key(|(_, handle)| **handle);
        for (value, expected) in path_handles_sorted {
            let (actual, _) = driver.intern_raw_value("Path", value.as_bytes().to_vec());
            assert_eq!(actual, *expected, "path handle assignment is deterministic");
        }

        Ok(Machine { driver, fn_refs })
    }

    /// Demand a function's value at the edge (scalars, this slice).
    pub fn demand_i64(&mut self, name: &str, args: Vec<i64>) -> Result<i64, String> {
        let fn_ref = *self
            .fn_refs
            .get(name)
            .ok_or_else(|| format!("no function named {name}"))?;
        self.driver.demand(fn_ref, args)
    }

    pub fn demand_f64(&mut self, name: &str, args: Vec<i64>) -> Result<f64, String> {
        let bits = self.demand_i64(name, args)? as u64;
        Ok(f64::from_bits(bits))
    }

    pub fn linux_target_handle(&self) -> i64 {
        self.driver.intern_linux_target().0
    }

    pub fn trace(&self) -> &[DriveEvent] {
        &self.driver.trace
    }

    pub fn clear_trace(&mut self) {
        self.driver.trace.clear();
    }

    pub fn store_len(&self) -> usize {
        self.driver.store_len()
    }

    pub fn tree_entries(
        &self,
        handle: i64,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        self.driver.tree_entries(handle)
    }

    pub fn fn_hash(&self, name: &str) -> Option<u64> {
        self.fn_refs
            .get(name)
            .map(|&fn_ref| self.driver.fn_hash(fn_ref))
    }
}

fn string_handles(tables: &ModuleTables) -> HashMap<String, i64> {
    let mut strings = BTreeSet::new();
    for item in tables.fns.values() {
        collect_block_strings(&item.body, &mut strings);
    }
    strings
        .into_iter()
        .enumerate()
        .map(|(ix, value)| (value, i64::try_from(ix).expect("string handle fits i64")))
        .collect()
}

fn path_handles(tables: &ModuleTables, offset: usize) -> HashMap<String, i64> {
    let mut paths = BTreeSet::new();
    for item in tables.fns.values() {
        collect_block_paths(&item.body, &mut paths);
    }
    paths
        .into_iter()
        .enumerate()
        .map(|(ix, value)| {
            (
                value,
                i64::try_from(offset + ix).expect("path handle fits i64"),
            )
        })
        .collect()
}

fn collect_block_strings(block: &ast::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => collect_expr_strings(&l.value, out),
            ast::Stmt::Expr(e) => collect_expr_strings(&e.expr, out),
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_strings(tail, out);
    }
}

fn collect_block_paths(block: &ast::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => collect_expr_paths(&l.value, out),
            ast::Stmt::Expr(e) => collect_expr_paths(&e.expr, out),
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_paths(tail, out);
    }
}

fn collect_expr_paths(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expr::Path(p) => {
            out.insert(p.value.clone());
        }
        ast::Expr::Binary(b) => {
            collect_expr_paths(&b.left, out);
            collect_expr_paths(&b.right, out);
        }
        ast::Expr::Unary(u) => collect_expr_paths(&u.operand, out),
        ast::Expr::Call(c) => {
            for arg in &c.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_paths(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_paths(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::MethodCall(m) => {
            collect_expr_paths(&m.receiver, out);
            for arg in &m.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_paths(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_paths(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Match(m) => {
            collect_expr_paths(&m.scrutinee, out);
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_paths(guard, out);
                }
                collect_expr_paths(&arm.value, out);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_expr_paths(&field.value, out);
            }
        }
        ast::Expr::Paren(p) => collect_expr_paths(&p.inner, out),
        ast::Expr::Field(f) => collect_expr_paths(&f.receiver, out),
        ast::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_paths(elem, out);
            }
        }
        ast::Expr::Array(a) => {
            for elem in &a.elems {
                if let ast::ArrayElem::Expr(e) = elem {
                    collect_expr_paths(e, out);
                }
            }
        }
        ast::Expr::Map(m) => {
            for entry in &m.entries {
                collect_expr_paths(&entry.key, out);
                collect_expr_paths(&entry.value, out);
            }
        }
        ast::Expr::Closure(c) => collect_expr_paths(&c.body, out),
        ast::Expr::Command(c) => {
            for part in &c.parts {
                if let ast::CommandPart::Splice(s) = part {
                    collect_expr_paths(&s.expr, out);
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Identifier(_)
        | ast::Expr::Str(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_expr_strings(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expr::Str(s) => {
            out.insert(s.value.clone());
        }
        ast::Expr::Binary(b) => {
            collect_expr_strings(&b.left, out);
            collect_expr_strings(&b.right, out);
        }
        ast::Expr::Unary(u) => collect_expr_strings(&u.operand, out),
        ast::Expr::Call(c) => {
            for arg in &c.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_strings(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_strings(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Match(m) => {
            collect_expr_strings(&m.scrutinee, out);
            for arm in &m.arms {
                collect_pattern_strings(&arm.pattern, out);
                if let Some(guard) = &arm.guard {
                    collect_expr_strings(guard, out);
                }
                collect_expr_strings(&arm.value, out);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_expr_strings(&field.value, out);
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    collect_expr_strings(base, out);
                }
            }
        }
        ast::Expr::Paren(p) => collect_expr_strings(&p.inner, out),
        ast::Expr::Field(f) => collect_expr_strings(&f.receiver, out),
        ast::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_strings(elem, out);
            }
        }
        ast::Expr::Array(a) => {
            for elem in &a.elems {
                if let ast::ArrayElem::Expr(e) = elem {
                    collect_expr_strings(e, out);
                }
            }
        }
        ast::Expr::Map(m) => {
            for entry in &m.entries {
                collect_expr_strings(&entry.key, out);
                collect_expr_strings(&entry.value, out);
            }
        }
        ast::Expr::MethodCall(m) => {
            collect_expr_strings(&m.receiver, out);
            for arg in &m.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_strings(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_strings(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Closure(c) => collect_expr_strings(&c.body, out),
        ast::Expr::Command(c) => {
            for part in &c.parts {
                if let ast::CommandPart::Splice(s) = part {
                    collect_expr_strings(&s.expr, out);
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Identifier(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_pattern_strings(pattern: &ast::Pattern, out: &mut BTreeSet<String>) {
    match pattern {
        ast::Pattern::Str(s) => {
            out.insert(s.value.clone());
        }
        ast::Pattern::Variant(v) => {
            for arg in &v.args {
                collect_pattern_strings(arg, out);
            }
        }
        ast::Pattern::Struct(s) => {
            for field in &s.fields {
                if let Some(pattern) = &field.pattern {
                    collect_pattern_strings(pattern, out);
                }
            }
        }
        ast::Pattern::Tuple(t) => {
            for elem in &t.elems {
                collect_pattern_strings(elem, out);
            }
        }
        ast::Pattern::Wildcard(_)
        | ast::Pattern::Scoped(_)
        | ast::Pattern::Identifier(_)
        | ast::Pattern::Number(_) => {}
    }
}

fn collect_block_type_schemas(block: &ast::Block, out: &mut Vec<String>) -> Result<(), String> {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => {
                if let Some(ty) = &l.ty {
                    collect_type_schema(ty, out)?;
                }
            }
            ast::Stmt::Expr(_) => {}
        }
    }
    Ok(())
}

fn collect_type_schema(ty: &ast::Type, out: &mut Vec<String>) -> Result<(), String> {
    let schema = type_schema_name(ty)?;
    out.push(schema.clone());
    if let Some(value_schema) = map_value_schema(&schema) {
        out.push(value_schema.to_string());
    }
    if let ast::Type::Generic(generic) = ty {
        for arg in &generic.args {
            collect_type_schema(arg, out)?;
        }
    }
    Ok(())
}

struct LoweredInfo {
    arg_offsets: Vec<u32>,
    invoke_region: u32,
    store_alloc_region: u32,
    store_read_region: u32,
    store_tag_region: u32,
    primitive_region: u32,
}

#[derive(Clone, Copy)]
struct LiteralHandles<'a> {
    strings: &'a HashMap<String, i64>,
    paths: &'a HashMap<String, i64>,
}

#[derive(Clone)]
struct ValueSlot {
    slot: u32,
    schema: String,
}

struct FnLowerer<'a> {
    tables: &'a ModuleTables,
    fn_refs: &'a HashMap<String, usize>,
    fn_returns: &'a HashMap<String, String>,
    fn_params: &'a HashMap<String, Vec<String>>,
    schema_refs: &'a HashMap<String, i64>,
    literal_handles: LiteralHandles<'a>,
    slots: HashMap<String, ValueSlot>,
    next: u32,
    code: Vec<Op>,
    invoke_region: u32,
    store_alloc_region: u32,
    store_read_region: u32,
    store_tag_region: u32,
    primitive_region: u32,
    next_input_slot: i64,
}

impl<'a> FnLowerer<'a> {
    fn lower(
        item: &ast::FnItem,
        tables: &'a ModuleTables,
        fn_refs: &'a HashMap<String, usize>,
        fn_returns: &'a HashMap<String, String>,
        fn_params: &'a HashMap<String, Vec<String>>,
        schema_refs: &'a HashMap<String, i64>,
        literal_handles: LiteralHandles<'a>,
    ) -> Result<(TaskFn, LoweredInfo), String> {
        let mut this = FnLowerer {
            tables,
            fn_refs,
            fn_returns,
            fn_params,
            schema_refs,
            literal_handles,
            slots: HashMap::new(),
            next: 0,
            code: Vec::new(),
            invoke_region: 0,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
            next_input_slot: 0,
        };

        let mut arg_offsets = Vec::new();
        for param in &item.params.params {
            let slot = this.alloc();
            let schema = type_schema_name(&param.ty)?;
            this.slots
                .insert(param.name.value.clone(), ValueSlot { slot, schema });
            arg_offsets.push(slot);
        }

        // Reserve the invoke region: [slot, fn_ref, argc, args...] —
        // sized for the widest call in the body.
        let max_argc = max_call_argc(&item.body);
        this.invoke_region = this.next;
        this.next += 8 * (3 + u32::try_from(max_argc).expect("argc fits u32"));
        let max_store_fields = max_store_field_count(&item.body);
        this.store_alloc_region = this.next;
        this.next += 8 * (4 + u32::try_from(max_store_fields).expect("field count fits u32"));
        this.store_read_region = this.next;
        this.next += 8 * 3;
        this.store_tag_region = this.next;
        this.next += 8 * 2;
        this.primitive_region = this.next;
        this.next += 8 * (8 + u32::try_from(max_store_fields).expect("field count fits u32"));

        let return_schema = item
            .return_type
            .as_ref()
            .map(type_schema_name)
            .transpose()?
            .unwrap_or_else(|| "Int".into());
        let result = this.block(&item.body, Some(&return_schema))?;
        this.code.push(Op::Ret {
            src: result.slot,
            size: 8,
        });

        let frame = Layout {
            size: this.next as usize,
            align: 8,
        };
        Ok((
            TaskFn {
                frame,
                code: this.code,
            },
            LoweredInfo {
                arg_offsets,
                invoke_region: this.invoke_region,
                store_alloc_region: this.store_alloc_region,
                store_read_region: this.store_read_region,
                store_tag_region: this.store_tag_region,
                primitive_region: this.primitive_region,
            },
        ))
    }

    fn alloc(&mut self) -> u32 {
        let slot = self.next;
        self.next += 8;
        slot
    }

    fn block(
        &mut self,
        block: &ast::Block,
        tail_expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        for stmt in &block.stmts {
            match stmt {
                ast::Stmt::Let(l) => {
                    let expected = l.ty.as_ref().map(type_schema_name).transpose()?;
                    let slot = self.expr_expected(&l.value, expected.as_deref())?;
                    // Lets are sequential and may shadow (binder
                    // semantics); binding the produced slot directly
                    // is safe because slots are single-assignment in
                    // this lowering (naive bump allocation).
                    self.slots.insert(l.name.value.clone(), slot);
                }
                ast::Stmt::Expr(_) => {
                    return Err("expression statements are outside the slice-1 subset".into());
                }
            }
        }
        let tail = block
            .tail
            .as_ref()
            .ok_or("slice-1 functions must end in a tail expression")?;
        self.expr_expected(tail, tail_expected)
    }

    /// Compile an expression; returns the frame slot holding its value.
    fn expr(&mut self, e: &ast::Expr) -> Result<ValueSlot, String> {
        self.expr_expected(e, None)
    }

    fn expr_expected(
        &mut self,
        e: &ast::Expr,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        match e {
            ast::Expr::Number(n) => {
                if n.value.contains('.') || expected == Some("Float") {
                    let value: f64 = n
                        .value
                        .parse()
                        .map_err(|_| format!("float literal {} does not parse", n.value))?;
                    let dst = self.alloc();
                    self.code.push(Op::ConstF64 {
                        dst,
                        bits: TotalF64::new(value).get().to_bits(),
                    });
                    return Ok(ValueSlot {
                        slot: dst,
                        schema: "Float".into(),
                    });
                }
                let value: i64 = n
                    .value
                    .parse()
                    .map_err(|_| format!("integer literal {} does not parse", n.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "Int".into(),
                })
            }
            ast::Expr::Str(s) => {
                let value = *self
                    .literal_handles
                    .strings
                    .get(&s.value)
                    .ok_or_else(|| format!("string literal {:?} was not interned", s.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "String".into(),
                })
            }
            ast::Expr::Path(p) => {
                let value = *self
                    .literal_handles
                    .paths
                    .get(&p.value)
                    .ok_or_else(|| format!("path literal {:?} was not interned", p.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "Path".into(),
                })
            }
            ast::Expr::Identifier(name) => self
                .slots
                .get(&name.value)
                .cloned()
                .ok_or_else(|| format!("unbound name {}", name.value)),
            ast::Expr::Paren(p) => self.expr(&p.inner),
            ast::Expr::Scoped(path) => self.scoped_value(path),
            ast::Expr::StructLit(lit) => self.struct_literal(lit),
            ast::Expr::Binary(b) if b.op == "/" => {
                let left = self.expr(&b.left)?;
                let right = self.expr(&b.right)?;
                match (left.schema.as_str(), right.schema.as_str()) {
                    ("Tree", "Path") => self.tree_project(&left, &right),
                    ("Path", "Path") => {
                        Err("Path / Path is outside the machine slice-4 subset".into())
                    }
                    _ => Err(format!(
                        "`/` on {} and {} is outside the machine slice-4 subset",
                        left.schema, right.schema
                    )),
                }
            }
            ast::Expr::Binary(b) => {
                let a = self.expr(&b.left)?;
                let r = self.expr(&b.right)?;
                let dst = self.alloc();
                let (op, schema) = match (b.op.as_str(), a.schema.as_str(), r.schema.as_str()) {
                    ("+", "Int", "Int") => (
                        Op::AddI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Int",
                    ),
                    ("-", "Int", "Int") => (
                        Op::SubI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Int",
                    ),
                    ("*", "Int", "Int") => (
                        Op::MulI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Int",
                    ),
                    ("+", "Float", "Float") => (
                        Op::AddF64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Float",
                    ),
                    ("*", "Float", "Float") => (
                        Op::MulF64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Float",
                    ),
                    ("==", _, _) => {
                        if a.schema != r.schema {
                            return Err(format!(
                                "cannot compare {} to {} in the machine slice-2 subset",
                                a.schema, r.schema
                            ));
                        }
                        (
                            Op::EqI64 {
                                dst,
                                a: a.slot,
                                b: r.slot,
                            },
                            "Int",
                        )
                    }
                    (other, _, _) => {
                        return Err(format!(
                            "operator {other:?} on {} and {} is outside the machine slice-3 subset",
                            a.schema, r.schema
                        ));
                    }
                };
                self.code.push(op);
                Ok(ValueSlot {
                    slot: dst,
                    schema: schema.into(),
                })
            }
            ast::Expr::Call(call) => self.call(call),
            ast::Expr::MethodCall(call) => self.method_call(call),
            ast::Expr::Map(map) => self.map_literal(map, expected),
            ast::Expr::Array(array) => self.array_literal(array),
            ast::Expr::Command(command) => self.command_block(command),
            ast::Expr::Match(m) => self.match_expr(m),
            other => Err(format!(
                "expression {other:?} is outside the slice-1 subset"
            )),
        }
    }

    /// Match on scalars: literal arms compile to EqI64 + JumpIfZero
    /// chains; the final arm must be irrefutable (wildcard or a
    /// binding) until the checker owns exhaustiveness. THE LAZINESS
    /// INVARIANT AT MACHINE LEVEL: an untaken arm's code never
    /// executes, so an INVOKE it contains never fires — unused arms
    /// never spawn, provable by trace absence.
    fn match_expr(&mut self, m: &ast::MatchExpr) -> Result<ValueSlot, String> {
        let scrut = self.expr(&m.scrutinee)?;
        let result = self.alloc();
        let mut result_schema: Option<String> = None;
        let mut jump_to_end: Vec<usize> = Vec::new();

        let last = m.arms.len().saturating_sub(1);
        for (i, arm) in m.arms.iter().enumerate() {
            if arm.guard.is_some() {
                return Err("match guards are outside the slice-2 subset".into());
            }
            let saved_slots = self.slots.clone();
            let mut skip_patch: Option<usize> = None;
            match &arm.pattern {
                ast::Pattern::Number(n) => {
                    let value: i64 = n
                        .value
                        .parse()
                        .map_err(|_| format!("pattern {} does not parse", n.value))?;
                    let lit = self.alloc();
                    self.code.push(Op::ConstI64 { dst: lit, value });
                    let test = self.alloc();
                    self.code.push(Op::EqI64 {
                        dst: test,
                        a: self.expect_schema(&scrut, "Int")?,
                        b: lit,
                    });
                    skip_patch = Some(self.code.len());
                    self.code.push(Op::JumpIfZero {
                        value: test,
                        target: 0,
                    });
                }
                ast::Pattern::Str(s) => {
                    let value =
                        *self.literal_handles.strings.get(&s.value).ok_or_else(|| {
                            format!("string pattern {:?} was not interned", s.value)
                        })?;
                    let lit = self.alloc();
                    self.code.push(Op::ConstI64 { dst: lit, value });
                    let test = self.alloc();
                    self.code.push(Op::EqI64 {
                        dst: test,
                        a: self.expect_schema(&scrut, "String")?,
                        b: lit,
                    });
                    skip_patch = Some(self.code.len());
                    self.code.push(Op::JumpIfZero {
                        value: test,
                        target: 0,
                    });
                }
                ast::Pattern::Scoped(path) => {
                    let (enum_name, variant_index, _) = self.resolve_scoped_variant(path)?;
                    self.variant_match_test(&scrut, &enum_name, variant_index, &mut skip_patch)?;
                }
                ast::Pattern::Variant(p) => {
                    let (enum_name, variant_index, shape) = self.resolve_path_variant(&p.path)?;
                    self.variant_match_test(&scrut, &enum_name, variant_index, &mut skip_patch)?;
                    let VariantShape::Tuple(expected) = shape else {
                        return Err(format!(
                            "tuple variant pattern used on non-tuple variant {enum_name}"
                        ));
                    };
                    if p.args.len() != expected {
                        return Err(format!(
                            "variant pattern expected {expected} fields, got {}",
                            p.args.len()
                        ));
                    }
                    for (field_index, pattern) in p.args.iter().enumerate() {
                        self.bind_payload_pattern(&scrut, variant_index, field_index, pattern)?;
                    }
                }
                ast::Pattern::Struct(p) => {
                    let (enum_name, variant_index, shape) = self.resolve_path_variant(&p.path)?;
                    self.variant_match_test(&scrut, &enum_name, variant_index, &mut skip_patch)?;
                    let VariantShape::Record(field_names) = shape else {
                        return Err(format!(
                            "record pattern used on non-record variant {enum_name}"
                        ));
                    };
                    for field in &p.fields {
                        let field_index = field_names
                            .iter()
                            .position(|name| name == &field.name.value)
                            .ok_or_else(|| format!("unknown field {}", field.name.value))?;
                        if let Some(pattern) = &field.pattern {
                            self.bind_payload_pattern(&scrut, variant_index, field_index, pattern)?;
                        } else {
                            let value = self.store_read(
                                &scrut,
                                field_index,
                                self.variant_field_schema(&enum_name, variant_index, field_index)?,
                            );
                            self.slots.insert(field.name.value.clone(), value);
                        }
                    }
                }
                ast::Pattern::Wildcard(_) => {
                    if i != last {
                        return Err("wildcard arm must be last".into());
                    }
                }
                ast::Pattern::Identifier(name) => {
                    if i != last {
                        return Err("binding arm must be last".into());
                    }
                    self.slots.insert(name.value.clone(), scrut.clone());
                }
                other => {
                    return Err(format!("pattern {other:?} is outside the slice-2 subset"));
                }
            }
            if skip_patch.is_none() && i != last {
                return Err("irrefutable arm before the last arm".into());
            }
            let v = self.expr(&arm.value)?;
            match &result_schema {
                Some(schema) if schema != &v.schema => {
                    return Err(format!(
                        "match arm returned {}, previous arm returned {schema}",
                        v.schema
                    ));
                }
                None => result_schema = Some(v.schema.clone()),
                _ => {}
            }
            self.code.push(Op::CopyI64 {
                dst: result,
                src: v.slot,
            });
            if i != last {
                jump_to_end.push(self.code.len());
                self.code.push(Op::Jump { target: 0 });
            }
            if let Some(at) = skip_patch {
                let next = u32::try_from(self.code.len()).expect("code len fits u32");
                let Op::JumpIfZero { value, .. } = self.code[at] else {
                    unreachable!("skip patch site is a JumpIfZero");
                };
                self.code[at] = Op::JumpIfZero {
                    value,
                    target: next,
                };
            } else if i == last {
                break;
            }
            self.slots = saved_slots;
        }
        if matches!(scrut.schema.as_str(), "Int" | "String")
            && !matches!(
                m.arms.last().map(|a| &a.pattern),
                Some(ast::Pattern::Wildcard(_) | ast::Pattern::Identifier(_))
            )
        {
            return Err(
                "scalar/string match must end with an irrefutable arm (exhaustiveness \
                 checking arrives with the checker)"
                    .into(),
            );
        }
        let end = u32::try_from(self.code.len()).expect("code len fits u32");
        for at in jump_to_end {
            self.code[at] = Op::Jump { target: end };
        }
        Ok(ValueSlot {
            slot: result,
            schema: result_schema.unwrap_or_else(|| "Int".into()),
        })
    }

    /// A user-function call: a MEMO BOUNDARY through the INVOKE
    /// protocol. Argument values are computed into slots first, then
    /// copied into the invoke region (frame-direct at the driver
    /// boundary), then HostCall + Await.
    fn call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        if let Some(value) = self.variant_constructor_call(call)? {
            return Ok(value);
        }
        if let Some(value) = self.builtin_scoped_call(call)? {
            return Ok(value);
        }
        let name = match &call.callee {
            ast::PathRef::Identifier(name) => &name.value,
            other => {
                return Err(format!("callee {other:?} is outside the slice-1 subset"));
            }
        };
        let fn_ref = *self
            .fn_refs
            .get(name)
            .ok_or_else(|| format!("unknown function {name}"))?;
        let expected_args = self
            .fn_params
            .get(name)
            .ok_or_else(|| format!("missing param schemas for {name}"))?
            .clone();

        let mut arg_slots = Vec::new();
        for (index, arg) in call.args.args.iter().enumerate() {
            match arg {
                ast::Arg::Expr(e) => {
                    let expected = expected_args
                        .get(index)
                        .ok_or_else(|| format!("too many arguments for {name}"))?;
                    arg_slots.push(self.expr_expected(e, Some(expected))?);
                }
                other => {
                    return Err(format!("argument {other:?} is outside the slice-1 subset"));
                }
            }
        }
        if arg_slots.len() != expected_args.len() {
            return Err(format!(
                "function {name} expected {} arguments, got {}",
                expected_args.len(),
                arg_slots.len()
            ));
        }

        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.invoke_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: i64::try_from(fn_ref).expect("fn_ref fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(arg_slots.len()).expect("argc fits i64"),
        });
        for (i, slot) in arg_slots.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 24 + 8 * u32::try_from(i).expect("arg index"),
                src: slot.slot,
            });
        }
        self.code.push(Op::HostCall { host: INVOKE_HOST });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: self.fn_returns[name].clone(),
        })
    }

    fn builtin_scoped_call(&mut self, call: &ast::Call) -> Result<Option<ValueSlot>, String> {
        let ast::PathRef::Scoped(path) = &call.callee else {
            return Ok(None);
        };
        let segments: Vec<&str> = path.segments.iter().map(|s| s.value.as_str()).collect();
        match segments.as_slice() {
            ["Cc", "acquire"] => {
                let [ast::Arg::Expr(target)] = call.args.args.as_slice() else {
                    return Err("Cc::acquire takes one target".into());
                };
                let target = self.expr_expected(target, Some("Target"))?;
                Ok(Some(self.acquire("Cc", &target)?))
            }
            _ => Ok(None),
        }
    }

    fn method_call(&mut self, call: &ast::MethodCall) -> Result<ValueSlot, String> {
        let receiver = self.expr(&call.receiver)?;
        match call.name.value.as_str() {
            "with_ext" => {
                if receiver.schema != "Path" {
                    return Err(format!("with_ext called on {}", receiver.schema));
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err("Path.with_ext takes one extension".into());
                };
                let ext = self.method_arg(arg, Some("String"))?;
                self.path_with_ext(&receiver, &ext)
            }
            "map" => self.array_map_pending(&receiver, call),
            "collect" => {
                if !call.args.args.is_empty() {
                    return Err("collect takes no arguments".into());
                }
                self.array_collect(&receiver)
            }
            "insert" => {
                let Some((key_schema, value_schema)) = map_schemas(&receiver.schema) else {
                    return Err(format!("insert called on {}", receiver.schema));
                };
                if call.args.args.len() != 2 {
                    return Err("Map.insert takes key and value".into());
                }
                let key = self.method_arg(&call.args.args[0], Some(key_schema))?;
                let value = self.method_arg(&call.args.args[1], Some(value_schema))?;
                self.map_insert(&receiver, key, value, key_schema, value_schema)
            }
            "get" => {
                let Some((key_schema, value_schema)) = map_schemas(&receiver.schema) else {
                    return Err(format!("get called on {}", receiver.schema));
                };
                if call.args.args.len() != 1 {
                    return Err("Map.get takes one key".into());
                }
                let key = self.method_arg(&call.args.args[0], Some(key_schema))?;
                self.map_get(&receiver, key, key_schema, value_schema)
            }
            "unwrap" => {
                if !call.args.args.is_empty() {
                    return Err("Option.unwrap takes no arguments".into());
                }
                let Some(value_schema) = option_value_schema(&receiver.schema) else {
                    return Err(format!("unwrap called on {}", receiver.schema));
                };
                Ok(self.option_unwrap(&receiver, value_schema))
            }
            other => Err(format!(
                "method {other} is outside the machine slice-3 subset"
            )),
        }
    }

    fn method_arg(&mut self, arg: &ast::Arg, expected: Option<&str>) -> Result<ValueSlot, String> {
        match arg {
            ast::Arg::Expr(expr) => self.expr_expected(expr, expected),
            other => Err(format!(
                "method argument {other:?} is outside the machine slice-3 subset"
            )),
        }
    }

    fn map_literal(
        &mut self,
        map: &ast::MapLiteral,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let schema = expected
            .filter(|schema| schema.starts_with("Map"))
            .unwrap_or("Map");
        let mut value = self.map_empty(schema)?;
        let (key_schema, value_schema) = map_schemas(schema)
            .map(|(key, value)| (Some(key.to_string()), Some(value.to_string())))
            .unwrap_or((None, None));
        for entry in &map.entries {
            let key = self.expr_expected(&entry.key, key_schema.as_deref())?;
            let item = self.expr_expected(&entry.value, value_schema.as_deref())?;
            let key_schema = key_schema.clone().unwrap_or_else(|| key.schema.clone());
            let value_schema = value_schema.clone().unwrap_or_else(|| item.schema.clone());
            value = self.map_insert(&value, key, item, &key_schema, &value_schema)?;
        }
        Ok(value)
    }

    fn scoped_value(&mut self, path: &ast::ScopedIdentifier) -> Result<ValueSlot, String> {
        let (enum_name, variant_index, shape) = self.resolve_scoped_variant(path)?;
        match shape {
            VariantShape::Unit => self.store_alloc(&enum_name, variant_index, &[]),
            other => Err(format!(
                "scoped value {enum_name}::{variant_index} has payload shape {other:?}; use call or struct literal"
            )),
        }
    }

    fn struct_literal(&mut self, lit: &ast::StructLiteral) -> Result<ValueSlot, String> {
        if !lit.spreads.is_empty() {
            return Err("record update is outside the machine slice-2 subset".into());
        }
        let path = path_ref_segments(&lit.path)?;
        if path.len() == 1 {
            let name = &path[0];
            let info = self
                .tables
                .structs
                .get(name)
                .ok_or_else(|| format!("unknown struct {name}"))?
                .clone();
            if info.is_unit {
                return self.store_alloc(name, 0, &[]);
            }
            let mut fields = Vec::new();
            for (field_name, default) in &info.fields {
                let init = lit
                    .fields
                    .iter()
                    .find(|field| &field.name.value == field_name)
                    .map(|field| &field.value)
                    .or(default.as_ref())
                    .ok_or_else(|| format!("missing field {field_name} for struct {name}"))?;
                fields.push(self.expr(init)?);
            }
            return self.store_alloc(name, 0, &fields);
        }

        let (enum_name, variant_index, shape) = self.resolve_path_variant(&lit.path)?;
        let VariantShape::Record(field_names) = shape else {
            return Err(format!(
                "struct literal syntax used for non-record variant {enum_name}"
            ));
        };
        let mut fields = Vec::new();
        for field_name in &field_names {
            let init = lit
                .fields
                .iter()
                .find(|field| &field.name.value == field_name)
                .ok_or_else(|| format!("missing field {field_name} for variant {enum_name}"))?;
            fields.push(self.expr(&init.value)?);
        }
        self.store_alloc(&enum_name, variant_index, &fields)
    }

    fn variant_constructor_call(&mut self, call: &ast::Call) -> Result<Option<ValueSlot>, String> {
        let Ok((enum_name, variant_index, shape)) = self.resolve_path_variant(&call.callee) else {
            return Ok(None);
        };
        let VariantShape::Tuple(expected) = shape else {
            return Err(format!(
                "call syntax used for non-tuple variant {enum_name}"
            ));
        };
        if call.args.args.len() != expected {
            return Err(format!(
                "variant constructor expected {expected} args, got {}",
                call.args.args.len()
            ));
        }
        let mut fields = Vec::new();
        for arg in &call.args.args {
            let ast::Arg::Expr(expr) = arg else {
                return Err(
                    "variant constructor kwargs are outside the machine slice-2 subset".into(),
                );
            };
            fields.push(self.expr(expr)?);
        }
        self.store_alloc(&enum_name, variant_index, &fields)
            .map(Some)
    }

    fn resolve_scoped_variant(
        &self,
        path: &ast::ScopedIdentifier,
    ) -> Result<(String, usize, VariantShape), String> {
        let segments: Vec<String> = path.segments.iter().map(|s| s.value.clone()).collect();
        resolve_variant_segments(self.tables, &segments)
    }

    fn resolve_path_variant(
        &self,
        path: &ast::PathRef,
    ) -> Result<(String, usize, VariantShape), String> {
        let segments = path_ref_segments(path)?;
        resolve_variant_segments(self.tables, &segments)
    }

    fn variant_match_test(
        &mut self,
        scrut: &ValueSlot,
        enum_name: &str,
        variant_index: usize,
        skip_patch: &mut Option<usize>,
    ) -> Result<(), String> {
        self.expect_schema(scrut, enum_name)?;
        let tag = self.store_tag(scrut);
        let lit = self.alloc();
        self.code.push(Op::ConstI64 {
            dst: lit,
            value: i64::try_from(variant_index).expect("variant index fits i64"),
        });
        let test = self.alloc();
        self.code.push(Op::EqI64 {
            dst: test,
            a: tag.slot,
            b: lit,
        });
        *skip_patch = Some(self.code.len());
        self.code.push(Op::JumpIfZero {
            value: test,
            target: 0,
        });
        Ok(())
    }

    fn bind_payload_pattern(
        &mut self,
        scrut: &ValueSlot,
        variant_index: usize,
        field_index: usize,
        pattern: &ast::Pattern,
    ) -> Result<(), String> {
        let schema = self.variant_field_schema(&scrut.schema, variant_index, field_index)?;
        match pattern {
            ast::Pattern::Identifier(name) => {
                let value = self.store_read(scrut, field_index, schema);
                self.slots.insert(name.value.clone(), value);
                Ok(())
            }
            ast::Pattern::Wildcard(_) => Ok(()),
            other => Err(format!(
                "nested pattern {other:?} is outside the machine slice-2 subset"
            )),
        }
    }

    fn variant_field_schema(
        &self,
        enum_name: &str,
        variant_index: usize,
        field_index: usize,
    ) -> Result<String, String> {
        let descriptor = self
            .tables
            .descriptors
            .get(enum_name)
            .ok_or_else(|| format!("missing descriptor for {enum_name}"))?;
        let weavy::mem::Access::Enum(access) = &descriptor.access else {
            return Err(format!("{enum_name} is not an enum descriptor"));
        };
        let field = access
            .variants
            .get(variant_index)
            .and_then(|variant| variant.payload.fields.get(field_index))
            .ok_or_else(|| format!("missing payload field {field_index} for {enum_name}"))?;
        match &field.descriptor.access {
            weavy::mem::Access::Handle { target } => Ok(target.clone()),
            _ => Ok(field.descriptor.schema.clone()),
        }
    }

    fn expect_schema(&self, value: &ValueSlot, expected: &str) -> Result<u32, String> {
        if value.schema == expected {
            Ok(value.slot)
        } else {
            Err(format!(
                "expected {expected}, got {} in the machine slice-2 subset",
                value.schema
            ))
        }
    }

    fn store_alloc(
        &mut self,
        schema: &str,
        variant_index: usize,
        fields: &[ValueSlot],
    ) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let type_ref = *self
            .schema_refs
            .get(schema)
            .ok_or_else(|| format!("no schema ref for {schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: type_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(variant_index).expect("variant index fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: i64::try_from(fields.len()).expect("field count fits i64"),
        });
        for (i, field) in fields.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 32 + 8 * u32::try_from(i).expect("field index fits u32"),
                src: field.slot,
            });
        }
        self.code.push(Op::HostCall {
            host: STORE_ALLOC_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: schema.to_string(),
        })
    }

    fn store_read(&mut self, handle: &ValueSlot, field_index: usize, schema: String) -> ValueSlot {
        let dst = self.alloc();
        let region = self.store_read_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: handle.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(field_index).expect("field index fits i64"),
        });
        self.code.push(Op::HostCall {
            host: STORE_READ_HOST,
        });
        ValueSlot { slot: dst, schema }
    }

    fn store_tag(&mut self, handle: &ValueSlot) -> ValueSlot {
        let dst = self.alloc();
        let region = self.store_tag_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: handle.slot,
        });
        self.code.push(Op::HostCall {
            host: STORE_TAG_HOST,
        });
        ValueSlot {
            slot: dst,
            schema: "Int".into(),
        }
    }

    fn map_empty(&mut self, schema: &str) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let schema_ref = *self
            .schema_refs
            .get(schema)
            .ok_or_else(|| format!("no schema ref for {schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: schema_ref,
        });
        self.code.push(Op::HostCall {
            host: MAP_EMPTY_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: schema.to_string(),
        })
    }

    fn map_insert(
        &mut self,
        map: &ValueSlot,
        key: ValueSlot,
        value: ValueSlot,
        key_schema: &str,
        value_schema: &str,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(&key, key_schema)?;
        self.expect_schema(&value, value_schema)?;
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let map_schema_ref = *self
            .schema_refs
            .get(&map.schema)
            .ok_or_else(|| format!("no schema ref for {}", map.schema))?;
        let key_schema_ref = *self
            .schema_refs
            .get(key_schema)
            .ok_or_else(|| format!("no schema ref for {key_schema}"))?;
        let value_schema_ref = *self
            .schema_refs
            .get(value_schema)
            .ok_or_else(|| format!("no schema ref for {value_schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: map.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: map_schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: key_schema_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 32,
            src: key.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 40,
            value: value_schema_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 48,
            src: value.slot,
        });
        self.code.push(Op::HostCall {
            host: MAP_INSERT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: map.schema.clone(),
        })
    }

    fn map_get(
        &mut self,
        map: &ValueSlot,
        key: ValueSlot,
        key_schema: &str,
        value_schema: &str,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(&key, key_schema)?;
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let key_schema_ref = *self
            .schema_refs
            .get(key_schema)
            .ok_or_else(|| format!("no schema ref for {key_schema}"))?;
        let value_schema_ref = *self
            .schema_refs
            .get(value_schema)
            .ok_or_else(|| format!("no schema ref for {value_schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: map.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: value_schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: key_schema_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 32,
            src: key.slot,
        });
        self.code.push(Op::HostCall { host: MAP_GET_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: option_schema(value_schema),
        })
    }

    fn option_unwrap(&mut self, option: &ValueSlot, value_schema: &str) -> ValueSlot {
        let dst = self.alloc();
        let region = self.store_alloc_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: option.slot,
        });
        self.code.push(Op::HostCall {
            host: OPTION_UNWRAP_HOST,
        });
        ValueSlot {
            slot: dst,
            schema: value_schema.to_string(),
        }
    }

    fn acquire(&mut self, kind: &str, target: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(target, "Target")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        let kind_ref = *self
            .schema_refs
            .get(kind)
            .ok_or_else(|| format!("no schema ref for {kind}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: kind_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: target.slot,
        });
        self.code.push(Op::HostCall { host: ACQUIRE_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: kind.to_string(),
        })
    }

    fn array_literal(&mut self, array: &ast::Array) -> Result<ValueSlot, String> {
        let mut elems = Vec::new();
        for elem in &array.elems {
            let ast::ArrayElem::Expr(expr) = elem else {
                return Err("array flags are outside the machine slice-4 subset".into());
            };
            elems.push(self.expr_expected(expr, Some("Path"))?);
        }
        if elems.iter().any(|elem| elem.schema != "Path") {
            return Err("slice-4 array literals are Path arrays only".into());
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: *self.schema_refs.get("Path").expect("Path schema ref"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(elems.len()).expect("array length fits i64"),
        });
        for (index, elem) in elems.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 24 + 8 * u32::try_from(index).expect("array index fits u32"),
                src: elem.slot,
            });
        }
        self.code.push(Op::HostCall {
            host: ARRAY_ALLOC_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Array".into(),
        })
    }

    fn array_map_pending(
        &mut self,
        receiver: &ValueSlot,
        call: &ast::MethodCall,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let [ast::Arg::Expr(ast::Expr::Closure(closure))] = call.args.args.as_slice() else {
            return Err("slice-4 array map requires a single closure argument".into());
        };
        let (fn_name, captured_name, param_name) = partial_named_fn_closure(closure)?;
        let fn_ref = *self
            .fn_refs
            .get(fn_name)
            .ok_or_else(|| format!("unknown function {fn_name}"))?;
        let captured = self
            .slots
            .get(captured_name)
            .cloned()
            .ok_or_else(|| format!("unbound capture {captured_name}"))?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(fn_ref).expect("fn ref fits i64"),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 24,
            src: captured.slot,
        });
        self.code.push(Op::HostCall {
            host: ARRAY_MAP_PENDING_HOST,
        });
        let _ = param_name;
        Ok(ValueSlot {
            slot: dst,
            schema: "Array".into(),
        })
    }

    fn array_collect(&mut self, receiver: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::HostCall {
            host: ARRAY_COLLECT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Tree".into(),
        })
    }

    fn tree_project(&mut self, tree: &ValueSlot, path: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(tree, "Tree")?;
        self.expect_schema(path, "Path")?;
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: tree.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: path.slot,
        });
        self.code.push(Op::HostCall {
            host: TREE_PROJECT_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Tree".into(),
        })
    }

    fn command_block(&mut self, command: &ast::CommandBlock) -> Result<ValueSlot, String> {
        if command.command.value != "cc" {
            return Err(format!(
                "command {} is outside the machine slice-4 subset",
                command.command.value
            ));
        }
        let capability = self
            .slots
            .get(&command.command.value)
            .cloned()
            .ok_or_else(|| format!("no capability `{}` in scope", command.command.value))?;
        let output = command_output_path_expr(command)
            .ok_or_else(|| "slice-4 cc command requires `-o {path}`".to_string())
            .and_then(|expr| self.expr_expected(expr, Some("Path")))?;
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: capability.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: output.slot,
        });
        self.code.push(Op::HostCall { host: EXEC_HOST });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Tree".into(),
        })
    }

    fn path_with_ext(&mut self, path: &ValueSlot, ext: &ValueSlot) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: path.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: ext.slot,
        });
        self.code.push(Op::HostCall {
            host: PATH_WITH_EXT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Path".into(),
        })
    }
}

fn path_ref_segments(path: &ast::PathRef) -> Result<Vec<String>, String> {
    match path {
        ast::PathRef::Identifier(name) => Ok(vec![name.value.clone()]),
        ast::PathRef::Scoped(path) => Ok(path.segments.iter().map(|s| s.value.clone()).collect()),
    }
}

fn map_schemas(schema: &str) -> Option<(&str, &str)> {
    let inner = schema.strip_prefix("Map<")?.strip_suffix('>')?;
    let (key, value) = inner.split_once(',')?;
    Some((key, value))
}

fn map_value_schema(schema: &str) -> Option<&str> {
    map_schemas(schema).map(|(_, value)| value)
}

fn option_schema(value_schema: &str) -> String {
    format!("Option<{value_schema}>")
}

fn option_value_schema(schema: &str) -> Option<&str> {
    schema.strip_prefix("Option<")?.strip_suffix('>')
}

fn partial_named_fn_closure(closure: &ast::Closure) -> Result<(&str, &str, &str), String> {
    let [param] = closure.params.as_slice() else {
        return Err("slice-4 map closure must have one parameter".into());
    };
    let ast::Expr::Call(call) = &closure.body else {
        return Err("slice-4 map closure body must be a named function call".into());
    };
    let ast::PathRef::Identifier(fn_name) = &call.callee else {
        return Err("slice-4 map closure callee must be a named function".into());
    };
    let [
        ast::Arg::Expr(ast::Expr::Identifier(captured)),
        ast::Arg::Expr(ast::Expr::Identifier(argument)),
    ] = call.args.args.as_slice()
    else {
        return Err("slice-4 map closure must call f(captured, parameter)".into());
    };
    if argument.value != param.value {
        return Err(format!(
            "slice-4 map closure argument must be parameter {}, got {}",
            param.value, argument.value
        ));
    }
    Ok((
        fn_name.value.as_str(),
        captured.value.as_str(),
        param.value.as_str(),
    ))
}

fn command_output_path_expr(command: &ast::CommandBlock) -> Option<&ast::Expr> {
    let mut saw_output = false;
    for part in &command.parts {
        match part {
            ast::CommandPart::Token(token) if token.value == "-o" => saw_output = true,
            ast::CommandPart::Splice(splice) if saw_output => return Some(&splice.expr),
            _ => saw_output = false,
        }
    }
    None
}

fn resolve_variant_segments(
    tables: &ModuleTables,
    segments: &[String],
) -> Result<(String, usize, VariantShape), String> {
    let [enum_name, variant_name] = segments else {
        return Err(format!("path {segments:?} is not a declared variant path"));
    };
    let info = tables
        .enums
        .get(enum_name)
        .ok_or_else(|| format!("unknown enum {enum_name}"))?;
    let (index, (_, shape)) = info
        .variants
        .iter()
        .enumerate()
        .find(|(_, (name, _))| name == variant_name)
        .ok_or_else(|| format!("unknown variant {enum_name}::{variant_name}"))?;
    Ok((enum_name.clone(), index, shape.clone()))
}

fn max_call_argc(block: &ast::Block) -> usize {
    fn in_expr(e: &ast::Expr, max: &mut usize) {
        match e {
            ast::Expr::Call(c) => {
                *max = (*max).max(c.args.args.len());
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::MethodCall(c) => {
                *max = (*max).max(c.args.args.len());
                in_expr(&c.receiver, max);
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::Map(m) => {
                for entry in &m.entries {
                    in_expr(&entry.key, max);
                    in_expr(&entry.value, max);
                }
            }
            ast::Expr::StructLit(lit) => {
                for field in &lit.fields {
                    in_expr(&field.value, max);
                }
            }
            ast::Expr::Binary(b) => {
                in_expr(&b.left, max);
                in_expr(&b.right, max);
            }
            ast::Expr::Paren(p) => in_expr(&p.inner, max),
            ast::Expr::Match(m) => {
                in_expr(&m.scrutinee, max);
                for arm in &m.arms {
                    in_expr(&arm.value, max);
                }
            }
            _ => {}
        }
    }
    let mut max = 0;
    for stmt in &block.stmts {
        if let ast::Stmt::Let(l) = stmt {
            in_expr(&l.value, &mut max);
        }
    }
    if let Some(tail) = &block.tail {
        in_expr(tail, &mut max);
    }
    max.max(2)
}

fn max_store_field_count(block: &ast::Block) -> usize {
    fn in_expr(e: &ast::Expr, max: &mut usize) {
        match e {
            ast::Expr::Call(c) => {
                *max = (*max).max(c.args.args.len());
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::MethodCall(c) => {
                in_expr(&c.receiver, max);
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::Map(m) => {
                for entry in &m.entries {
                    in_expr(&entry.key, max);
                    in_expr(&entry.value, max);
                }
            }
            ast::Expr::StructLit(lit) => {
                *max = (*max).max(lit.fields.len());
                for field in &lit.fields {
                    in_expr(&field.value, max);
                }
            }
            ast::Expr::Binary(b) => {
                in_expr(&b.left, max);
                in_expr(&b.right, max);
            }
            ast::Expr::Paren(p) => in_expr(&p.inner, max),
            ast::Expr::Match(m) => {
                in_expr(&m.scrutinee, max);
                for arm in &m.arms {
                    in_expr(&arm.value, max);
                }
            }
            _ => {}
        }
    }
    let mut max = 0;
    for stmt in &block.stmts {
        if let ast::Stmt::Let(l) = stmt {
            in_expr(&l.value, &mut max);
        }
    }
    if let Some(tail) = &block.tail {
        in_expr(tail, &mut max);
    }
    max.max(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::hash::{DefaultHasher, Hash, Hasher};

    const CORPUS: &str = r#"
fn square(x: Int) -> Int { x * x }

fn twice_sq(n: Int) -> Int { square(n) + square(n) }

pub fn poly(n: Int) -> Int {
    let t = twice_sq(n + 1);
    t - n
}
"#;

    fn lanes() -> Vec<Lane> {
        let mut lanes = vec![Lane::Interp];
        #[cfg(any(test, feature = "jit"))]
        lanes.push(Lane::Jit);
        lanes
    }

    fn load_with_lane(source: &str, lane: Lane) -> Machine {
        Machine::load_with_lane(source, lane).unwrap_or_else(|err| {
            panic!("loads on {lane:?}: {err}");
        })
    }

    #[test]
    fn the_scalar_corpus_runs_on_the_machine() {
        for lane in lanes() {
            let mut m = load_with_lane(CORPUS, lane);
            // poly(3): square(4)=16 twice -> 32; 32 - 3 = 29.
            assert_eq!(m.demand_i64("poly", vec![3]).unwrap(), 29, "{lane:?}");
        }
    }

    #[test]
    fn shared_calls_spawn_once() {
        for lane in lanes() {
            let mut m = load_with_lane(CORPUS, lane);
            m.demand_i64("poly", vec![3]).unwrap();
            let spawns = m
                .trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
                .count();
            // poly, twice_sq, square — square(4) is called twice with the
            // same argument and spawns ONCE (memo + waiter joining).
            assert_eq!(spawns, 3, "{lane:?}");
        }
    }

    #[test]
    fn warm_demand_is_two_events() {
        for lane in lanes() {
            let mut m = load_with_lane(CORPUS, lane);
            m.demand_i64("poly", vec![3]).unwrap();
            m.clear_trace();
            assert_eq!(m.demand_i64("poly", vec![3]).unwrap(), 29, "{lane:?}");
            assert_eq!(
                m.trace().len(),
                2,
                "Demanded + MemoHit, nothing else on {lane:?}"
            );
        }
    }

    #[test]
    fn undemanded_functions_never_trace() {
        let source = format!("{CORPUS}\nfn never(z: Int) -> Int {{ z * 1000 }}\n");
        for lane in lanes() {
            let mut m = load_with_lane(&source, lane);
            m.demand_i64("poly", vec![5]).unwrap();
            // Mechanism 2 by absence: `never`'s closure hash appears
            // nowhere in the trace.
            let never_ref = m.fn_refs["never"];
            let _ = never_ref;
            let poly = m.demand_i64("poly", vec![5]).unwrap();
            assert_eq!(poly, (6 * 6) * 2 - 5, "{lane:?}");
            assert_eq!(
                m.trace()
                    .iter()
                    .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
                    .count(),
                3,
                "three spawns total; `never` never appears on {lane:?}"
            );
        }
    }

    #[test]
    fn fib_runs_linear_on_the_machine() {
        let src = r#"
fn fib(n: Int) -> Int {
    match n {
        0 => 0,
        1 => 1,
        _ => fib(n - 1) + fib(n - 2),
    }
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("fib", vec![20]).unwrap(), 6765, "{lane:?}");
            let spawns = m
                .trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
                .count();
            // fib(0)..fib(20): 21 distinct invocations, 21 spawns — LINEAR.
            // Naive recursion runs 13,529 more bodies than this.
            assert_eq!(spawns, 21, "{lane:?}");
            traces.push((lane, m.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn untaken_arms_never_spawn() {
        let src = r#"
fn cheap(x: Int) -> Int { x + 1 }
fn expensive(x: Int) -> Int { x * 1000000 }
fn pick(b: Int) -> Int {
    match b {
        0 => cheap(b),
        _ => expensive(b),
    }
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("pick", vec![0]).unwrap(), 1, "{lane:?}");
            let spawns = m
                .trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
                .count();
            // pick + cheap. `expensive` sits in an untaken arm: its INVOKE
            // never executed, so it never spawned, never demanded, never
            // anything — the laziness proof by trace absence.
            assert_eq!(spawns, 2, "{lane:?}");
            traces.push((lane, m.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn binding_arms_bind_the_scrutinee() {
        let src = r#"
fn f(n: Int) -> Int {
    match n {
        0 => 7,
        m => m * 2,
    }
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("f", vec![0]).unwrap(), 7, "{lane:?}");
            assert_eq!(m.demand_i64("f", vec![21]).unwrap(), 42, "{lane:?}");
        }
    }

    #[test]
    fn refutable_matches_without_irrefutable_tail_are_rejected() {
        let src = "fn f(n: Int) -> Int { match n { 0 => 1, 1 => 2 } }";
        for lane in lanes() {
            let err = Machine::load_with_lane(src, lane)
                .and_then(|mut m| m.demand_i64("f", vec![0]))
                .unwrap_err();
            assert!(err.contains("irrefutable"), "{lane:?}: {err}");
        }
    }

    #[test]
    fn float_literals_lower_as_canonical_bits() {
        for lane in lanes() {
            let mut m = load_with_lane("fn f() -> Float { 1.5 }", lane);
            assert_eq!(
                (m.demand_i64("f", vec![]).unwrap() as u64),
                1.5f64.to_bits(),
                "{lane:?}"
            );
        }
    }

    #[test]
    fn recursive_enum_tree_evaluates_on_the_machine() {
        let src = r#"
enum Expr {
    Num(Int),
    Add(Expr, Expr),
    Mul(Expr, Expr),
}

fn eval(e: Expr) -> Int {
    match e {
        Expr::Num(n) => n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
    }
}

fn main() -> Int {
    eval(Expr::Add(Expr::Num(2), Expr::Mul(Expr::Num(3), Expr::Num(4))))
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("main", vec![]).unwrap(), 14, "{lane:?}");
        }
    }

    #[test]
    fn structural_equal_handles_share_store_and_memo() {
        let src = r#"
enum Expr {
    Num(Int),
    Add(Expr, Expr),
}

fn make_a() -> Expr {
    Expr::Add(Expr::Num(1), Expr::Num(2))
}

fn make_b() -> Expr {
    Expr::Add(Expr::Num(1), Expr::Num(2))
}

fn eval(e: Expr) -> Int {
    match e {
        Expr::Num(n) => n,
        Expr::Add(a, b) => eval(a) + eval(b),
    }
}

fn main() -> Int {
    let a = make_a();
    let b = make_b();
    eval(a) + eval(b)
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("main", vec![]).unwrap(), 6, "{lane:?}");
            let eval_hash = m.fn_hash("eval").expect("eval hash");
            let eval_spawns = m
                .trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::Spawned { fn_hash } if *fn_hash == eval_hash))
                .count();
            let eval_hits = m
                .trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::MemoHit { fn_hash } if *fn_hash == eval_hash))
                .count();
            assert_eq!(
                eval_spawns, 3,
                "Add, Num(1), Num(2) each spawn once on {lane:?}"
            );
            assert!(
                eval_hits > 0,
                "second structurally equal tree hits memo on {lane:?}"
            );
            assert!(
                m.trace()
                    .iter()
                    .any(|e| matches!(e, DriveEvent::StoreAlloc { deduped: true, .. })),
                "second constructor path dedupes in the value store on {lane:?}"
            );
        }
    }

    #[test]
    fn untaken_variant_arms_never_spawn() {
        let src = r#"
enum Choice { A, B }

fn expensive() -> Int { 999999 }

fn pick(c: Choice) -> Int {
    match c {
        Choice::A => 1,
        Choice::B => expensive(),
    }
}

fn main() -> Int {
    pick(Choice::A)
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("main", vec![]).unwrap(), 1, "{lane:?}");
            let expensive_hash = m.fn_hash("expensive").expect("expensive hash");
            assert!(
                !m.trace().iter().any(|e| matches!(
                    e,
                    DriveEvent::Demanded { fn_hash } | DriveEvent::Spawned { fn_hash }
                        if *fn_hash == expensive_hash
                )),
                "untaken variant arm never demands or spawns expensive on {lane:?}"
            );
            traces.push((lane, m.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn strings_are_interned_and_match_by_handle() {
        let src = r#"
fn classify() -> Int {
    let a = "same";
    let b = "same";
    match a {
        "same" => 42,
        _ => 0,
    }
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(
                m.store_len(),
                1,
                "two identical literals intern once on {lane:?}"
            );
            assert_eq!(m.demand_i64("classify", vec![]).unwrap(), 42, "{lane:?}");
            assert_eq!(
                m.store_len(),
                1,
                "string matching does not allocate on {lane:?}"
            );
        }
    }

    #[test]
    fn eval_vix_demo_returns_42_on_the_machine() {
        let src = include_str!("../../../playgrounds/snark/src/bundled/vix/samples/eval.vix");
        let mut cold_traces = Vec::new();
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            let bits = m.demand_i64("demo", vec![]).unwrap() as u64;
            assert_eq!(bits, 42.0f64.to_bits(), "{lane:?}");
            let demo_hash = m.fn_hash("demo").expect("demo hash");
            let spawns = m
                .trace()
                .iter()
                .filter(|event| matches!(event, DriveEvent::Spawned { .. }))
                .count();
            assert_eq!(
                spawns, 6,
                "demo plus five distinct eval invocations on {lane:?}"
            );
            cold_traces.push((lane, m.trace().to_vec()));

            m.clear_trace();
            let warm_bits = m.demand_i64("demo", vec![]).unwrap() as u64;
            assert_eq!(warm_bits, 42.0f64.to_bits(), "{lane:?}");
            assert_eq!(
                m.trace(),
                &[
                    DriveEvent::Demanded { fn_hash: demo_hash },
                    DriveEvent::MemoHit { fn_hash: demo_hash },
                ],
                "warm demo is exactly demand + memo hit on {lane:?}"
            );
        }
        assert_lane_traces_equal(&cold_traces);
    }

    #[test]
    fn eval_vix_untaken_helper_never_appears() {
        let src = format!(
            "{}\n{}",
            include_str!("../../../playgrounds/snark/src/bundled/vix/samples/eval.vix"),
            r#"
fn never_float() -> Float { 99.0 }

pub fn lazy_probe() -> Float {
    let e = Expr::Num(1.0);
    match e {
        Expr::Num(n) => n,
        Expr::Var(_) => never_float(),
        _ => 0.0,
    }
}
"#
        );
        for lane in lanes() {
            let mut m = load_with_lane(&src, lane);
            assert_eq!(
                (m.demand_i64("lazy_probe", vec![]).unwrap() as u64),
                1.0f64.to_bits(),
                "{lane:?}"
            );
            let never_hash = m.fn_hash("never_float").expect("never_float hash");
            assert!(
                !m.trace().iter().any(|event| matches!(
                    event,
                    DriveEvent::Demanded { fn_hash } | DriveEvent::Spawned { fn_hash }
                        if *fn_hash == never_hash
                )),
                "helper in untaken variant arm never demanded or spawned on {lane:?}"
            );
        }
    }

    #[test]
    fn maps_are_canonical_regardless_of_insertion_order() {
        let src = r#"
fn ab() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("a", 1.0).insert("b", 2.0)
}

fn ba() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("b", 2.0).insert("a", 1.0)
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            let ab = m.demand_i64("ab", vec![]).unwrap();
            let ba = m.demand_i64("ba", vec![]).unwrap();
            let ab_entry = m.driver.store_entry(ab).expect("ab entry");
            let ba_entry = m.driver.store_entry(ba).expect("ba entry");
            assert_eq!(ab_entry.content_hash, ba_entry.content_hash, "{lane:?}");
            assert_eq!(
                ab, ba,
                "dedupe returns the same canonical handle on {lane:?}"
            );
        }
    }

    #[test]
    fn insertion_order_equal_maps_memoize_as_equal_arguments() {
        let src = r#"
fn ab() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("a", 1.0).insert("b", 2.0)
}

fn ba() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("b", 2.0).insert("a", 1.0)
}

fn consume(m: Map<String, Float>) -> Float {
    m.get("a").unwrap() + m.get("b").unwrap()
}

fn main() -> Float {
    consume(ab()) + consume(ba())
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(
                (m.demand_i64("main", vec![]).unwrap() as u64),
                6.0f64.to_bits(),
                "{lane:?}"
            );
            let consume_hash = m.fn_hash("consume").expect("consume hash");
            let consume_spawns = m
                .trace()
                .iter()
                .filter(|event| matches!(event, DriveEvent::Spawned { fn_hash } if *fn_hash == consume_hash))
                .count();
            let consume_hits = m
                .trace()
                .iter()
                .filter(|event| matches!(event, DriveEvent::MemoHit { fn_hash } if *fn_hash == consume_hash))
                .count();
            assert_eq!(consume_spawns, 1, "{lane:?}");
            assert_eq!(consume_hits, 1, "{lane:?}");
        }
    }

    #[test]
    fn option_unwrap_none_is_a_machine_error() {
        let src = r#"
fn missing() -> Float {
    let m: Map<String, Float> = {};
    m.get("missing").unwrap()
}
"#;
        for lane in lanes() {
            let err = Machine::load_with_lane(src, lane)
                .and_then(|mut machine| machine.demand_i64("missing", vec![]))
                .unwrap_err();
            assert!(err.contains("unwrap on None"), "{lane:?}: {err}");
        }
    }

    fn assert_lane_traces_equal(traces: &[(Lane, Vec<DriveEvent>)]) {
        let Some((first_lane, first_trace)) = traces.first() else {
            return;
        };
        for (lane, trace) in &traces[1..] {
            assert_eq!(
                trace, first_trace,
                "driver trace diverged between {first_lane:?} and {lane:?}"
            );
        }
    }

    fn trace_hash(value: &str) -> u64 {
        let mut h = DefaultHasher::new();
        value.hash(&mut h);
        h.finish()
    }

    fn expected_object() -> BTreeMap<String, String> {
        BTreeMap::from([("wanted.o".to_string(), "obj(9259fea8a69f1945)".to_string())])
    }

    fn spawned_count(machine: &Machine, name: &str) -> usize {
        let hash = machine.fn_hash(name).expect("function hash");
        machine
            .trace()
            .iter()
            .filter(|event| matches!(event, DriveEvent::Spawned { fn_hash } if *fn_hash == hash))
            .count()
    }

    fn run_outputs(machine: &Machine, pick: impl Fn(&DriveEvent) -> Option<u64>) -> Vec<u64> {
        let mut outputs: Vec<u64> = machine.trace().iter().filter_map(pick).collect();
        outputs.sort();
        outputs
    }

    fn started_outputs(machine: &Machine) -> Vec<u64> {
        run_outputs(machine, |event| match event {
            DriveEvent::RunStarted { command, output } => {
                assert_eq!(*command, trace_hash("cc"));
                Some(*output)
            }
            _ => None,
        })
    }

    fn completed_outputs(machine: &Machine) -> Vec<u64> {
        run_outputs(machine, |event| match event {
            DriveEvent::RunCompleted { command, output } => {
                assert_eq!(*command, trace_hash("cc"));
                Some(*output)
            }
            _ => None,
        })
    }

    fn output_set(paths: &[&str]) -> Vec<u64> {
        let mut values: Vec<u64> = paths.iter().map(|path| trace_hash(path)).collect();
        values.sort();
        values
    }

    fn load_merge_demand(lane: Lane) -> Machine {
        load_with_lane(
            include_str!("../../../playgrounds/snark/src/bundled/vix/samples/merge-demand.vix"),
            lane,
        )
    }

    #[test]
    fn merge_demand_selected_tunnels_and_never_runs_left() {
        let mut cold_traces = Vec::new();
        let mut first_handle = None;
        for lane in lanes() {
            let mut machine = load_merge_demand(lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("selected", vec![target]).unwrap();

            assert_eq!(
                machine.tree_entries(handle).unwrap(),
                expected_object(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "selected"), 1, "{lane:?}");
            assert_eq!(spawned_count(&machine, "object"), 1, "{lane:?}");
            assert_eq!(
                started_outputs(&machine),
                output_set(&["wanted.o"]),
                "{lane:?}"
            );
            assert_eq!(
                completed_outputs(&machine),
                output_set(&["wanted.o"]),
                "{lane:?}"
            );
            assert!(
                !machine
                    .trace()
                    .iter()
                    .any(|event| matches!(event, DriveEvent::RunRequested { output, .. } if *output == trace_hash("left.o"))),
                "left.o producer is never requested on {lane:?}"
            );
            if let Some(expected) = first_handle {
                assert_eq!(handle, expected, "same selected result handle on {lane:?}");
            } else {
                first_handle = Some(handle);
            }
            cold_traces.push((lane, machine.trace().to_vec()));

            let selected_hash = machine.fn_hash("selected").expect("selected hash");
            machine.clear_trace();
            let warm = machine.demand_i64("selected", vec![target]).unwrap();
            assert_eq!(warm, handle, "{lane:?}");
            assert_eq!(
                machine.trace(),
                &[
                    DriveEvent::Demanded {
                        fn_hash: selected_hash
                    },
                    DriveEvent::MemoHit {
                        fn_hash: selected_hash
                    },
                ],
                "warm selected demand is exactly root memo hit on {lane:?}"
            );
        }
        assert_lane_traces_equal(&cold_traces);
    }

    #[test]
    fn merge_demand_fallback_falls_left_after_right_absence() {
        let mut cold_traces = Vec::new();
        let mut first_handle = None;
        for lane in lanes() {
            let mut machine = load_merge_demand(lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("fallback", vec![target]).unwrap();

            assert_eq!(
                machine.tree_entries(handle).unwrap(),
                expected_object(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "fallback"), 1, "{lane:?}");
            assert_eq!(
                spawned_count(&machine, "object"),
                2,
                "right.o is run to prove absence, then wanted.o is demanded on {lane:?}"
            );
            assert_eq!(
                started_outputs(&machine),
                output_set(&["right.o", "wanted.o"]),
                "{lane:?}"
            );
            assert_eq!(
                completed_outputs(&machine),
                output_set(&["right.o", "wanted.o"]),
                "{lane:?}"
            );
            assert!(
                !machine
                    .trace()
                    .iter()
                    .any(|event| matches!(event, DriveEvent::RunRequested { output, .. } if *output == trace_hash("left.o"))),
                "left.o is outside fallback's demanded path on {lane:?}"
            );
            if let Some(expected) = first_handle {
                assert_eq!(handle, expected, "same fallback result handle on {lane:?}");
            } else {
                first_handle = Some(handle);
            }
            cold_traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&cold_traces);
    }

    #[test]
    fn merge_demand_subtree_chain_refines_without_left() {
        let mut cold_traces = Vec::new();
        let mut first_handle = None;
        for lane in lanes() {
            let mut machine = load_merge_demand(lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("subtree_chain", vec![target]).unwrap();

            assert_eq!(
                machine.tree_entries(handle).unwrap(),
                expected_object(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "subtree_chain"), 1, "{lane:?}");
            assert_eq!(spawned_count(&machine, "object"), 1, "{lane:?}");
            assert_eq!(
                started_outputs(&machine),
                output_set(&["x/wanted.o"]),
                "{lane:?}"
            );
            assert_eq!(
                completed_outputs(&machine),
                output_set(&["x/wanted.o"]),
                "{lane:?}"
            );
            assert!(
                !machine
                    .trace()
                    .iter()
                    .any(|event| matches!(event, DriveEvent::RunRequested { output, .. } if *output == trace_hash("left.o"))),
                "left.o producer is never requested through the subtree chain on {lane:?}"
            );
            if let Some(expected) = first_handle {
                assert_eq!(
                    handle, expected,
                    "same subtree_chain result handle on {lane:?}"
                );
            } else {
                first_handle = Some(handle);
            }
            cold_traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&cold_traces);
    }
}
