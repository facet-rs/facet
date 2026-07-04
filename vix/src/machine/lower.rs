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

use std::collections::HashMap;

use weavy::mem::Layout;
use weavy::task::{Fn as TaskFn, FnId, Op, Program};

use super::driver::{Driver, DriveEvent, INVOKE_HOST, LoweredFn};
use crate::ast;
use crate::module::load_module_tables;

/// The machine facade for this slice: load source, demand a function's
/// value at the edge.
pub struct Machine {
    driver: Driver,
    fn_refs: HashMap<String, usize>,
}

impl Machine {
    pub fn load(source: &str) -> Result<Machine, String> {
        let tables = load_module_tables(source)?;

        // Deterministic fn_ref assignment: sorted names.
        let mut names: Vec<&String> = tables.fns.keys().collect();
        names.sort();
        let fn_refs: HashMap<String, usize> = names
            .iter()
            .enumerate()
            .map(|(ix, name)| ((*name).clone(), ix))
            .collect();

        let mut task_fns = Vec::with_capacity(names.len());
        let mut lowered = Vec::with_capacity(names.len());
        for (ix, name) in names.iter().enumerate() {
            let item = &tables.fns[*name];
            let hash = tables.fn_hashes[*name];
            let (task_fn, info) = FnLowerer::lower(item, &fn_refs)
                .map_err(|e| format!("lowering {name}: {e}"))?;
            task_fns.push(task_fn);
            lowered.push(LoweredFn {
                hash,
                task_fn: FnId(u32::try_from(ix).expect("fn count fits u32")),
                arg_offsets: info.arg_offsets,
                invoke_region: info.invoke_region,
            });
        }

        Ok(Machine {
            driver: Driver::new(Program { fns: task_fns }, lowered),
            fn_refs,
        })
    }

    /// Demand a function's value at the edge (scalars, this slice).
    pub fn demand_i64(&mut self, name: &str, args: Vec<i64>) -> Result<i64, String> {
        let fn_ref = *self
            .fn_refs
            .get(name)
            .ok_or_else(|| format!("no function named {name}"))?;
        Ok(self.driver.demand(fn_ref, args))
    }

    pub fn trace(&self) -> &[DriveEvent] {
        &self.driver.trace
    }

    pub fn clear_trace(&mut self) {
        self.driver.trace.clear();
    }
}

struct LoweredInfo {
    arg_offsets: Vec<u32>,
    invoke_region: u32,
}

struct FnLowerer<'a> {
    fn_refs: &'a HashMap<String, usize>,
    slots: HashMap<String, u32>,
    next: u32,
    code: Vec<Op>,
    invoke_region: u32,
    next_input_slot: i64,
}

impl<'a> FnLowerer<'a> {
    fn lower(
        item: &ast::FnItem,
        fn_refs: &'a HashMap<String, usize>,
    ) -> Result<(TaskFn, LoweredInfo), String> {
        let mut this = FnLowerer {
            fn_refs,
            slots: HashMap::new(),
            next: 0,
            code: Vec::new(),
            invoke_region: 0,
            next_input_slot: 0,
        };

        let mut arg_offsets = Vec::new();
        for param in &item.params.params {
            let slot = this.alloc();
            this.slots.insert(param.name.value.clone(), slot);
            arg_offsets.push(slot);
        }

        // Reserve the invoke region: [slot, fn_ref, argc, args...] —
        // sized for the widest call in the body.
        let max_argc = max_call_argc(&item.body);
        this.invoke_region = this.next;
        this.next += 8 * (3 + u32::try_from(max_argc).expect("argc fits u32"));

        let result = this.block(&item.body)?;
        this.code.push(Op::Ret {
            src: result,
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
            },
        ))
    }

    fn alloc(&mut self) -> u32 {
        let slot = self.next;
        self.next += 8;
        slot
    }

    fn block(&mut self, block: &ast::Block) -> Result<u32, String> {
        for stmt in &block.stmts {
            match stmt {
                ast::Stmt::Let(l) => {
                    let slot = self.expr(&l.value)?;
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
        self.expr(tail)
    }

    /// Compile an expression; returns the frame slot holding its value.
    fn expr(&mut self, e: &ast::Expr) -> Result<u32, String> {
        match e {
            ast::Expr::Number(n) => {
                if n.value.contains('.') {
                    return Err(format!(
                        "float literal {} is outside the slice-1 subset",
                        n.value
                    ));
                }
                let value: i64 = n
                    .value
                    .parse()
                    .map_err(|_| format!("integer literal {} does not parse", n.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(dst)
            }
            ast::Expr::Identifier(name) => self
                .slots
                .get(&name.value)
                .copied()
                .ok_or_else(|| format!("unbound name {}", name.value)),
            ast::Expr::Paren(p) => self.expr(&p.inner),
            ast::Expr::Binary(b) => {
                let a = self.expr(&b.left)?;
                let r = self.expr(&b.right)?;
                let dst = self.alloc();
                let op = match b.op.as_str() {
                    "+" => Op::AddI64 { dst, a, b: r },
                    "-" => Op::SubI64 { dst, a, b: r },
                    "*" => Op::MulI64 { dst, a, b: r },
                    other => {
                        return Err(format!(
                            "operator {other} is outside the slice-1 subset"
                        ));
                    }
                };
                self.code.push(op);
                Ok(dst)
            }
            ast::Expr::Call(call) => self.call(call),
            other => Err(format!(
                "expression {other:?} is outside the slice-1 subset"
            )),
        }
    }

    /// A user-function call: a MEMO BOUNDARY through the INVOKE
    /// protocol. Argument values are computed into slots first, then
    /// copied into the invoke region (frame-direct at the driver
    /// boundary), then HostCall + Await.
    fn call(&mut self, call: &ast::Call) -> Result<u32, String> {
        let name = match &call.callee {
            ast::PathRef::Identifier(name) => &name.value,
            other => {
                return Err(format!(
                    "callee {other:?} is outside the slice-1 subset"
                ));
            }
        };
        let fn_ref = *self
            .fn_refs
            .get(name)
            .ok_or_else(|| format!("unknown function {name}"))?;

        let mut arg_slots = Vec::new();
        for arg in &call.args.args {
            match arg {
                ast::Arg::Expr(e) => arg_slots.push(self.expr(e)?),
                other => {
                    return Err(format!(
                        "argument {other:?} is outside the slice-1 subset"
                    ));
                }
            }
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
                src: *slot,
            });
        }
        self.code.push(Op::HostCall { host: INVOKE_HOST });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(dst)
    }
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
            ast::Expr::Binary(b) => {
                in_expr(&b.left, max);
                in_expr(&b.right, max);
            }
            ast::Expr::Paren(p) => in_expr(&p.inner, max),
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
    max
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &str = r#"
fn square(x: Int) -> Int { x * x }

fn twice_sq(n: Int) -> Int { square(n) + square(n) }

pub fn poly(n: Int) -> Int {
    let t = twice_sq(n + 1);
    t - n
}
"#;

    #[test]
    fn the_scalar_corpus_runs_on_the_machine() {
        let mut m = Machine::load(CORPUS).expect("loads");
        // poly(3): square(4)=16 twice -> 32; 32 - 3 = 29.
        assert_eq!(m.demand_i64("poly", vec![3]).unwrap(), 29);
    }

    #[test]
    fn shared_calls_spawn_once() {
        let mut m = Machine::load(CORPUS).expect("loads");
        m.demand_i64("poly", vec![3]).unwrap();
        let spawns = m
            .trace()
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        // poly, twice_sq, square — square(4) is called twice with the
        // same argument and spawns ONCE (memo + waiter joining).
        assert_eq!(spawns, 3);
    }

    #[test]
    fn warm_demand_is_two_events() {
        let mut m = Machine::load(CORPUS).expect("loads");
        m.demand_i64("poly", vec![3]).unwrap();
        m.clear_trace();
        assert_eq!(m.demand_i64("poly", vec![3]).unwrap(), 29);
        assert_eq!(m.trace().len(), 2, "Demanded + MemoHit, nothing else");
    }

    #[test]
    fn undemanded_functions_never_trace() {
        let source = format!("{CORPUS}\nfn never(z: Int) -> Int {{ z * 1000 }}\n");
        let mut m = Machine::load(&source).expect("loads");
        m.demand_i64("poly", vec![5]).unwrap();
        // Mechanism 2 by absence: `never`'s closure hash appears
        // nowhere in the trace.
        let never_ref = m.fn_refs["never"];
        let _ = never_ref;
        let poly = m.demand_i64("poly", vec![5]).unwrap();
        assert_eq!(poly, (6 * 6) * 2 - 5);
        assert_eq!(
            m.trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
                .count(),
            3,
            "three spawns total; `never` never appears"
        );
    }

    #[test]
    fn floats_are_rejected_loudly() {
        let err = Machine::load("fn f() -> Float { 1.5 }")
            .and_then(|mut m| m.demand_i64("f", vec![]))
            .unwrap_err();
        assert!(err.contains("float literal"), "{err}");
    }
}
