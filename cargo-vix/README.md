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

## vix compiles and runs a Rust binary

`hello.vix` is rung one of the ladder, and it is green:

```
RUSTC=$(rustup which rustc) cargo test -p vixen-runtime --test exec_tree_mounts
```

That test runs `hello.vix` itself, so the shipped example and the proof are one
source. Running it through `vx` directly needs a machine manifest offering
`Rustc` — the default offers only `Echo`/`Sh`/`ProgressiveSh`, so `vx` refuses
before anything runs. `hello.vix`'s header has the manifest to copy.

Three processes — write the source, compile it with rustc, run the binary it
produced — and the check reads `hello from vix` off the third one's stdout.

This works because a `Tree` is now spliceable into a command template. The tree
joins the request's `mounts` array and the splice renders as its deterministic
workspace-relative path, so `{src}/src/main.rs` fuses through the ordinary
adjacency rule and needs no path-building surface. The tree's value identity is
IN the request, which is what makes "the same plan over changed sources" a
different demand rather than a stale memo hit.

## Where the road stops now

- **Origin-backed trees cannot be mounted.** A `fixture_tree` is a lazy handle,
  and enumerating one needs a directory verb on the effect authority that does
  not exist. Content-identified trees — an `untar`'d archive, another exec's
  output — mount fine, which is why `hello.vix` writes its source with a shell
  exec first. This is the next thing to build: without it a build walk cannot
  read a workspace off disk.
- **No declared env** (rung `074-exec-env.vix`, red): the request record has no
  env field. Crates read `CARGO_PKG_*` through `env!`, and build scripts are
  nothing but env.
- **An exec template cannot contain a literal brace** — `{` always starts an
  interpolation and there is no escape. Harmless for rustc argv; crippling for
  `sh -c` with real shell code (`hello.vix` writes `\173`/`\175` octal escapes
  and lets printf produce the braces).
- **A template splices bare NAMES only**, not expressions: `{sources.tree}` is
  an unbound identifier, so each tree is bound to a `let` first.
- **No rmeta pipelining.** The machinery exists and is proven — rung 128 has a
  subfile consumer finishing before its producer exits — but `Rustc` declares
  `ExecOutputProtocol::ExitOnly`, so nothing is announced. rustc does emit the
  signal (`--json=artifacts` writes `{"$message_type":"artifact",…,"emit":"metadata"}`
  to stderr, metadata before link). Two easy pieces (a rustc artifact protocol;
  read the protocol off stderr, not just stdout) and one hard one: the
  progressive rail only engages when the projection path is a COMPILE-TIME
  CONSTANT, and a build walk computes `lib{name}.rmeta` from lock data.

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
- **The build order keys on package NAME alone.** A real lock may hold one name
  at two versions, and its `dependencies` entries then read `"name version"`.
  The Kahn walk would come up short and the truncation reads as a cycle. Keying
  on name+version is the fix; the length check in `orders_the_locked_packages`
  is what keeps it loud meanwhile.
- **No `String.len`, slicing, or right-split**, which is why `stdlib/strings.vix`
  ships no `ends_with`: the "split on the suffix, is the last piece empty" trick
  is wrong for a suffix that overlaps an earlier occurrence (`"a---"` vs `"--"`),
  and that is the shape extensions and operators take.
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
