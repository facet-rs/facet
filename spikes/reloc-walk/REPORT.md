# Relocation-Graph Test Selection Spike

## Fixture

`spikes/reloc-walk/fixture` is a two-crate workspace:

- `lib_a`: `hash_stuff`, `stable_mix`, `hash_pipeline`, and generic `generic_fold<T>`.
- `test_crate`: one integration test binary, `selection`, with six `#[test]` functions.

The tests split as:

- local only: `local_arithmetic_is_isolated`, `local_string_is_isolated`, `local_table_is_isolated`
- non-generic `lib_a`: `hash_direct_uses_lib_a`, `hash_pipeline_uses_lib_a`
- downstream generic instantiation: `generic_instantiation_uses_lib_a`

The analyzer builds copied scenario workspaces with:

```sh
cargo test --no-run -p test_crate --test selection
```

and `RUSTFLAGS` containing `-C save-temps=yes`, `-C codegen-units=8`,
`-C link-dead-code=no`, `-C incremental=off` via `CARGO_INCREMENTAL=0`,
and v0 symbol mangling for readable names. No fixture test binary is executed.

## Mechanics

What worked:

- `object 0.39.1` parsed both Mach-O `.o` files and `.rlib` archive members.
- `-C save-temps=yes` left loose `selection` and `test_crate` CGU objects in
  `target/debug/deps`; `lib_a` and `test_crate` rlib members were parsed with
  `object::read::archive::ArchiveFile`.
- The graph includes text-to-text calls, data-to-text test registrar entries,
  text-to-data panic/assert metadata, and data/text references represented by
  Mach-O relocations.
- `libtest` registration was visible as data relocations: each test function
  had one incoming non-text registrar atom. The generated `main`/static test
  table reaches all tests, so it is useful for discovery/proof of registration,
  not as the per-test root.

Mach-O quirks:

- Rustc on `aarch64-apple-darwin` did not appear as one physical `object`
  section per function. The no-debug build had 18 physical text sections but
  69 text atoms. The analyzer therefore uses Mach-O symbol-bounded atoms inside
  sections, matching the `subsections_via_symbols` style linker granularity.
- Observed relocation widths were 26, 32, and 64 bits. No non-zero relocation
  addends appeared in this fixture.
- With `debuginfo=2`, physical sections rose from 86 to 254 and atoms from 151
  to 319. Debug atoms changed on comment edits, but reachable loadable atom
  sets stayed stable for the same-line comment case.

Hashing:

- Each atom hash is the atom bytes with relocation target ranges masked, plus
  the ordered relocation target names by offset. The tool records relocation
  kind/size/addend for diagnosis, but the spike hash follows the requested
  `(offset, target-symbol-name)` shape.
- For production, relocation kind/size/addend should be part of the normalized
  hash. This fixture had no non-zero addends, so the omission did not affect the
  matrix.

## Invalidation Matrix

Baseline for `*-nodebug` is `baseline-nodebug`; baseline for `*-debug` is
`baseline-debug`.

| scenario | local_arithmetic | local_string | local_table | hash_direct | hash_pipeline | generic_instantiation |
| --- | --- | --- | --- | --- | --- | --- |
| same-line comment, debuginfo=0 | skip | skip | skip | skip | skip | skip |
| same-line comment, debuginfo=2 | skip | skip | skip | skip | skip | skip |
| `hash_stuff` body change | skip | skip | skip | rerun | rerun | skip |
| `generic_fold` body change | skip | skip | skip | skip | skip | rerun |
| line-inserting comment, debuginfo=0 | skip | skip | skip | skip | skip | rerun |
| line-inserting comment, debuginfo=2 | skip | skip | skip | skip | skip | rerun |

Changed loadable atoms:

- `hash_stuff` body: one loadable atom changed, `lib_a::hash_stuff`, in
  `target/debug/deps/lib_a-...lib_a...-cgu.1.rcgu.o`.
- `generic_fold` body: one loadable atom changed,
  `lib_a::generic_fold::<u64>`, in
  `target/debug/deps/selection-...selection...-cgu.2.rcgu.o`. This is the
  monomorphization case pure edge-walking misses; content hashing catches it.
- same-line comment: no loadable atoms changed.
- line-inserting comment: one loadable anonymous const atom changed in
  `selection` CGU 2, under `__DATA/__const`. This is source-location/panic
  metadata in loadable data, not DWARF. It is a false positive for ordinary
  tests, but not safe to normalize away globally because tests can observe
  `Location::caller`.

## Soundness Holes

Relocation reachability is sound for what the linker can see. It does not see:

- computed calls whose target is not represented by a relocation or relocation
  backed function pointer table
- `dlsym`/name-based dispatch, inline assembly, JIT emission, or FFI callbacks
- filesystem/environment/build-script inputs that affect test behavior
- proc macro output unless the generated code's objects are also represented
  in the analyzed build
- changes in compiler flags, target features, panic strategy, cfgs, or build
  profile that are outside the object graph

The atom model is conservative when names collide: every matching definition is
followed. That can over-invalidate but does not skip a reachable changed atom in
this fixture.

## Verdict

For the motivating fixture, relocation-graph reachability over object atoms
soundly selected the two `hash_stuff` tests after a body change, skipped the
three local tests, and caught the generic monomorphization by changed downstream
content rather than by an edge to a changed `lib_a` object.

This is not shippable as Vix's full predicted-read-set mechanism by itself. It
is shippable as a conservative object-code evidence layer if unresolved or
relocation-invisible behavior falls back to rerun. Before relying on it for Vix
test prediction, the next production slice should add exact link-input capture
from Cargo/rustc, include relocation addends/kinds in normalized hashes, model
Mach-O atom boundaries explicitly, and keep source-location metadata as a
documented false-positive source rather than trying to erase it.
