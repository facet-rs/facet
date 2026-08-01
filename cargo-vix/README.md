# cargo-vix

Reimplementing cargo in vix, starting from the one end of it that runs today:
read a real `Cargo.toml` **and its `Cargo.lock`**, discover the crate's targets,
order the locked packages into a build sequence, and plan the rustc invocation
each one needs.

The lock is the load-bearing input, not the manifest: the 0.1 north star is "an
existing `Cargo.lock` → a built binary", so nothing here resolves a version or
visits an index. The manifest names the root package and its targets; the lock
supplies the resolved graph, and its `checksum` is the sha256 that pins every
fetch (`machine.primitive.fetch-is-pinned`, as amended 2026-07-26).

```
cargo build -p vixen-runtime --bin vx
./target/debug/vx cargo-vix/cargo.vix
```

`vx` is a shim (`vixen-runtime/src/bin/vx.rs`), not a product: it runs one file's
`#[test]` declarations through the same production path the ratchet drives and
reports the checks. It roots the harness `FixtureStore` at the `.vix` file's own
directory, which is why the crate under test lives in `trees/small-crate/` —
`fixture_tree("small-crate")` reads `<root>/trees/<name>`.

## Where the road stops, and why

`cargo.vix` gets as far as rendering the exact argv rustc wants. It does not run
rustc, and the blocker is one specific gap rather than a pile of them:

**An exec cannot receive an input file.** `lower_exec` builds the request as
`{capability, argv: [String]}`, and a template splices `Int | String | Path`
only (`vix-core/src/compiler.rs`, `parse_command_template`). A tree projection
is a `Blob`, there are no mounts, and there is no `cwd`. So rustc can be spawned
but cannot be shown `src/lib.rs`. Rung `073-exec-consumes-tree.vix` is exactly
this shape and is excluded from the ratchet runner — it is a red rung, not an
oversight.

The second gap, close behind: **no declared env** (rung `074-exec-env.vix`, red
for the same reason — the request record has no env field). Crates read
`CARGO_PKG_*` through `env!`, and build scripts are nothing but env.

Everything else the first step needs already exists and is exercised here:
`tree-read` for the manifest bytes, `decode` for TOML → typed structs,
`tree-glob` for target discovery, and `exec` itself for the invocation. No new
primitive is required to finish this — the exec rail's input side is.

## Gaps found while writing this

Recorded where they were hit, so they are not rediscovered:

- **Dynamic `Path` construction.** `Path / segment` requires a string LITERAL,
  so a `Path` cannot be rebuilt from computed text. Computed `Path` *values*
  still exist (`Tree.glob` yields them, tree projection accepts them) — you just
  cannot build one. This is why `stdlib/paths.vix` is all queries and why its
  `parent_text` returns text.
- **Closure-return type inference.** A type parameter appearing only in a
  closure's return position cannot be inferred (`cannot infer type argument
  `U``), and instantiation is inference-only, so it cannot be stated at the call
  either. `Option.map` / `and_then` are unwritable today; so is the array twin
  `fn amap<T, U>(xs: [T]) where { f: fn(T) -> U }`.
- **No `#[decode(rename)]`.** Hyphenated TOML keys (`dev-dependencies`,
  `default-features`, `crate-type`) have no spelling that decodes onto a struct
  field, which bounds how much of a manifest can be ingested typed.
- **No `Map` decode target.** `[dependencies]` cannot decode into
  `Map<String, String>` — only into a struct naming each dependency, which a
  general tool cannot write ahead of time. Not on the critical path (the lock's
  `dependencies` array carries the resolved graph), but it is what a manifest's
  open-keyed tables want.
- **Fixed while writing this: sequences did not decode at all.** Every array
  target fell through to `UnsupportedTarget`, so no `Cargo.lock` could be read
  — `[[package]]` needs `[LockPackage]` and `dependencies` needs `[String]`.
  Rungs 062–066 have no array field between them, and an infallible `decode`
  reports the failure as an `InvalidRequest` machine fault, so the symptom
  never named its cause. See `vixen-runtime/tests/decode_sequences.rs`.
- **Method calls take no `where` clause** ("named method arguments"): a
  multi-argument helper is called method-style with positional arguments
  (`text.replace("-", "_")`), never `.replace("-") where { to: "_" }`.
