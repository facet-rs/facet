# Where a type is declared: Rust or vix

A recurring decision when building on the vix machine: does a given type live in
Rust (declared with `#[derive(Facet)]`, Rust ABI) or in the vix language (vix
ABI)? This note fixes the criterion so it stops being re-derived (and re-gotten
wrong) per agent.

## There is no god `Value` enum

Values are **bytes laid out by their schema** (`SchemaRef` is the type
authority): facet-style *records-at-offsets* and *enums-as-tag+variants*. The
interpreter operates on bytes-with-schema; there is no monolithic
`enum Value { Int, Float, VersionSet, Region, … }` that every operation matches
on, and the JIT/stencils stay type-specialized because the type is static at
lower time. Because layout is schema-driven, a type can be declared on **either**
side and both sides can manipulate it — Rust via facet reflection, vix via its
own ABI. Debuggers, the REPL, and other tooling speak both ABIs.

## The criterion: who manipulates it?

- **Rust code manipulates it** → declare it in **Rust** (`#[derive(Facet)]`),
  Rust ABI. vix reaches it through facet (the schema is the bridge).
- **Only vix touches it** → declare it in **vix**, vix ABI.

That is the whole rule. It is *not* "does it have special algebra" or "is it a
primitive" — it is strictly about which side actually creates and mutates the
value. Inspection is never a reason to force a type to Rust: tooling reads both
ABIs.

## Content-addressing / caching is free — never hand-roll it

**Any vix value is store-cacheable by virtue of being a vix value.** The
schema-driven layout already gives every value a canonical representation the
store content-addresses (hashes) natively. So:

- Do **not** write a manual `canonical_bytes() -> Vec<u8>` (or equivalent) on a
  type to "make it cacheable." That reinvents what the store does for
  everything, and pays a `Vec` allocation per call to do it.
- A hand-rolled canonical-bytes method is a **smell** — it means either the type
  is fighting the store, or it was declared in Rust for a reason that has since
  evaporated.

"We need its canonical bytes" is therefore **not** a reason to declare a type in
Rust. Neither is "it is parsed" — parsing can be vix code.

## The bar for a Rust primitive is high

Declare in Rust only when Rust genuinely manipulates the value, **or** when a
*measured* hotspot (not an assumed one) needs a native representation and
operations that vix + JIT + flesh demonstrably cannot meet. Default to vix.

Worked example — `VersionSet`: today it is Rust with a `canonical_bytes` method.
Under this criterion, the content-addressing justification is void (store gives
it free) and "it is parsed" is weak. The only remaining case for keeping it in
Rust is interval-algebra throughput in the resolver's propagate loop — and that
is a measurement to run, not an assumption. Everything composed *above* it in a
pure-vix resolver (`Region`, `State`, `Domain`, `LearnedFact`) is vix: nothing in
Rust creates or narrows them, so they carry the vix ABI and persist as vix values
through the machine's own store/demand lifecycle.
