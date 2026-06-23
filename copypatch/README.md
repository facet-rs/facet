# copypatch

[![crates.io](https://img.shields.io/crates/v/copypatch.svg)](https://crates.io/crates/copypatch)
[![documentation](https://docs.rs/copypatch/badge.svg)](https://docs.rs/copypatch)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/copypatch.svg)](../../LICENSE-MIT)

A copy-and-patch JIT works by keeping precompiled machine code for each
operation — *stencils* — each with a hole where the branch to the next
operation will go. At run time the JIT copies the relevant stencils back-to-back
into a fresh buffer, then patches each hole with the offset of the next stencil
in that buffer, chaining them into a straight-line code path. The compiler does
all the instruction selection up front; the runtime cost is a handful of `memcpy`
calls and a few bitfield writes.

`copypatch` is the bottom layer that stencil JIT sits on. It handles the two
platform concerns a caller cannot ignore — write-xor-execute (W^X) memory on
Apple Silicon via `MAP_JIT` and `pthread_jit_write_protect_np`, plus
`sys_icache_invalidate` after writing — and the one AArch64 instruction concern:
encoding a signed PC-relative offset into a `B`/`BL` instruction's 26-bit
immediate. It has no opinion on IR, schemas, or value representations; those all
live in the caller.

## Runtime usage

Patch stencil bytes into a buffer, copy the buffer into executable memory, and
call the result:

```rust
use copypatch::{ExecBuf, patch_branch26};

// Stencil bytes come from build.rs (via copypatch::extract) or any other
// source. Here two tiny AArch64 stubs are already in hand.
let stencil_a: &[u8] = &[/* ... */]; // ends with a B whose immediate is 0
let stencil_b: &[u8] = &[/* ... */]; // the continuation

// Lay the stencils out contiguously.
let mut code = Vec::new();
let site_a = stencil_a.len() - 4; // offset of the B instruction in stencil A
code.extend_from_slice(stencil_a);
let target_b = code.len();         // stencil B starts here
code.extend_from_slice(stencil_b);

// Patch the branch in A so it jumps to B.
patch_branch26(&mut code, site_a, target_b);

// Copy into executable memory (handles MAP_JIT + i-cache flush).
let buf = ExecBuf::new(&code);
let f: unsafe extern "C" fn() -> u64 = unsafe { std::mem::transmute(buf.as_ptr()) };
let result = unsafe { f() };
```

## Build-time stencil extraction

In a `build.rs`, use the `build` feature to compile a stencil source file and
read each stencil's bytes and continuation-relocation offsets back out of the
object file:

```toml
[build-dependencies]
copypatch = { version = "0.2", features = ["build"] }
```

```rust
// build.rs
use copypatch::extract::{compile_object, extract_stencil, nightly_available};
use std::path::Path;

let src  = Path::new("src/stencils.rs");
let obj  = Path::new(&std::env::var("OUT_DIR").unwrap()).join("stencils.o");
let target = std::env::var("TARGET").unwrap();

// Try nightly for tail-call stencils; fall back to stable.
let ok = nightly_available()
    && compile_object("rustc", &["+nightly"], &src, &obj, &target, true);
if !ok {
    assert!(compile_object("rustc", &[], &src, &obj, &target, false));
}

let obj_data = std::fs::read(&obj).unwrap();
let all = &["stencil_add", "stencil_ret"];
let s = extract_stencil(&obj_data, all, "stencil_add", "stencil_ret");
// s.bytes       — machine code for stencil_add
// s.cont_relocs — offsets within s.bytes where B instructions need patching
```

## What stays in the caller

The stencils themselves, the per-op state structs, and the lowering from your IR
to a chain of stencil calls all live outside this crate. `copypatch` only runs
bytes and patches offsets.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
