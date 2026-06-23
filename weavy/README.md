# weavy

[![crates.io](https://img.shields.io/crates/v/weavy.svg)](https://crates.io/crates/weavy)
[![documentation](https://docs.rs/weavy/badge.svg)](https://docs.rs/weavy)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/weavy.svg)](../../LICENSE-MIT)

weavy is the shared substrate for *lowered programs* — the common carrier that sits
between a front-end language and the backends that run it. It stays deliberately
format-agnostic: callers bring their own schema identities, parsers, and value
models. weavy supplies the shared shape for lowered programs (flat programs,
named blocks, and a call-stack runner that avoids unbounded Rust recursion), the
canonical IR that PHON, facet-json, facet-hash, and future scripting frontends
converge toward, and the typed-memory descriptor and op vocabulary (`mem`) that
drives encode/decode without ever assuming a concrete wire format.

The same program/block shape is reused by both the generic interpreter runner and
native copy-and-patch backends, so an interpreter and a JIT compile from one
lowered representation without translation. Front-ends like facet-hash build
directly on top of weavy; higher-level runtimes such as Fable lower toward the
canonical Weavy IR.

## Example

The `Step` trait connects a caller-defined op vocabulary to the generic runner.
Returning `Control::CallBlock` or `Control::CallProgram` pushes a new frame onto
weavy's internal stack rather than recursing through Rust, which keeps stack depth
bounded even for deeply recursive types:

```rust
use std::collections::BTreeMap;
use weavy::{run, Control, Lowered, Step};

#[derive(Clone, Debug)]
enum Op {
    Push(u32),
    Call(u32),
}

struct Eval {
    seen: Vec<u32>,
}

impl<'p> Step<'p, u32, Op> for Eval {
    type Error = ();
    type Continuation = ();

    fn step(&mut self, op: &'p Op) -> Result<Control<'p, u32, Op>, ()> {
        Ok(match op {
            Op::Push(n) => { self.seen.push(*n); Control::Continue }
            Op::Call(block) => Control::CallBlock(*block),
        })
    }
}

let lowered = Lowered {
    program: vec![Op::Push(1), Op::Call(7), Op::Push(4)],
    blocks: BTreeMap::from([(7, vec![Op::Push(2), Op::Push(3)])]),
};
let mut eval = Eval { seen: Vec::new() };
run(&lowered, &mut eval).unwrap();
assert_eq!(eval.seen, vec![1, 2, 3, 4]);
```

## How it fits

```
facet-hash  ──┐
facet-json  ──┤  lower to  ┌─ weavy::ir (canonical WeavyOp IR)
PHON        ──┘            │    ControlOp / MemoryOp / InitOp / AggregateOp
                           │    + domain Intrinsics via IntrinsicOp trait
                           │
                           ├─ weavy::mem (typed-memory MemOp vocabulary)
                           │    Scalar / Sequence / Enum / Option / Map / …
                           │    thunk vtables for Vec, HashMap, &str, Box<T>, …
                           │
                           ├─ interpreter  (weavy::run / run_dense)
                           └─ copy-and-patch JIT  (weavy::jit, feature = "jit")
```

The `Lowered<BlockId, Op>` type uses caller-defined symbolic block ids during
lowering and diagnostics; `block_refs()` resolves them into dense `BlockRef`
indices once before hot interpreter paths run.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
