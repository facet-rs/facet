# facet-solver

[![codecov](https://codecov.io/gh/facet-rs/facet/graph/badge.svg)](https://codecov.io/gh/facet-rs/facet)
[![crates.io](https://img.shields.io/crates/v/facet-solver.svg)](https://crates.io/crates/facet-solver)
[![documentation](https://docs.rs/facet-solver/badge.svg)](https://docs.rs/facet-solver)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-solver.svg)](./LICENSE)
[![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)



Helps facet deserializers implement `#[facet(flatten)]` and `#[facet(untagged)]`
correctly, efficiently, and with useful diagnostics.

## The Problem

When deserializing a type with a flattened enum:

```rust
# use facet::Facet;
#[derive(Facet)]
struct TextMessage { content: String }

#[derive(Facet)]
struct BinaryMessage { data: Vec<u8>, encoding: String }

#[derive(Facet)]
#[repr(u8)]
enum MessagePayload {
    Text(TextMessage),
    Binary(BinaryMessage),
}

#[derive(Facet)]
struct Message {
    id: String,
    #[facet(flatten)]
    payload: MessagePayload,
}
```

...we don't know which variant to use until we've seen the fields:

```json
{"id": "msg-1", "content": "hello"}        // Text
{"id": "msg-2", "data": [1,2,3], "encoding": "raw"}  // Binary
```

The solver answers: "which variant has a `content` field?" or "which variant
has both `data` and `encoding`?"

## How It Works

The solver pre-computes all valid field combinations ("configurations") for a type,
then uses an inverted index to quickly find which configuration(s) match the
fields you've seen.

```rust
use facet_solver::{KeyResult, Schema, Solver};
# use facet::Facet;
# #[derive(Facet)]
# struct TextMessage { content: String }
# #[derive(Facet)]
# struct BinaryMessage { data: Vec<u8>, encoding: String }
# #[derive(Facet)]
# #[repr(u8)]
# enum MessagePayload { Text(TextMessage), Binary(BinaryMessage) }
# #[derive(Facet)]
# struct Message { id: String, #[facet(flatten)] payload: MessagePayload }

// Build schema once (can be cached)
let schema = Schema::build(Message::SHAPE).unwrap();

// Create a solver for this deserialization
let mut solver = Solver::new(&schema);

// As you see fields, report them:
match solver.see_key("id") {
    KeyResult::Unambiguous { .. } => { /* both configs have "id" */ }
    _ => {}
}

match solver.see_key("content") {
    KeyResult::Solved(config) => {
        // Only Text has "content" - we now know the variant!
        assert!(config.resolution().has_key_path(&["content"]));
    }
    _ => {}
}
```

### Nested Disambiguation

When top-level keys don't distinguish variants, the solver can look deeper:

```rust
# use facet::Facet;
#[derive(Facet)]
struct TextPayload { content: String }

#[derive(Facet)]
struct BinaryPayload { bytes: Vec<u8> }

#[derive(Facet)]
#[repr(u8)]
enum Payload {
    Text { inner: TextPayload },
    Binary { inner: BinaryPayload },
}

#[derive(Facet)]
struct Wrapper {
    #[facet(flatten)]
    payload: Payload,
}
```

Both variants have an `inner` field. But `inner.content` only exists in `Text`,
and `inner.bytes` only exists in `Binary`. The `ProbingSolver` handles this:

```rust
use facet_solver::{ProbingSolver, ProbeResult, Schema};
# use facet::Facet;
# #[derive(Facet)]
# struct TextPayload { content: String }
# #[derive(Facet)]
# struct BinaryPayload { bytes: Vec<u8> }
# #[derive(Facet)]
# #[repr(u8)]
# enum Payload { Text { inner: TextPayload }, Binary { inner: BinaryPayload } }
# #[derive(Facet)]
# struct Wrapper { #[facet(flatten)] payload: Payload }

let schema = Schema::build(Wrapper::SHAPE).unwrap();
let mut solver = ProbingSolver::new(&schema);

// Top-level "inner" doesn't disambiguate
assert!(matches!(solver.probe_key(&[], "inner"), ProbeResult::KeepGoing));

// But "inner.content" does!
match solver.probe_key(&["inner"], "content") {
    ProbeResult::Solved(config) => {
        assert!(config.has_key_path(&["inner", "content"]));
    }
    _ => panic!("should have solved"),
}
```

### Lazy Type Disambiguation

Sometimes variants have **identical keys** but different value types. The solver handles
this without buffering—it lets you probe "can this value fit type X?" lazily:

```rust
# use facet::Facet;
#[derive(Facet)]
struct SmallPayload { value: u8 }

#[derive(Facet)]
struct LargePayload { value: u16 }

#[derive(Facet)]
#[repr(u8)]
enum Payload {
    Small { payload: SmallPayload },
    Large { payload: LargePayload },
}

#[derive(Facet)]
struct Container {
    #[facet(flatten)]
    inner: Payload,
}
```

Both variants have `payload.value`, but one is `u8` (max 255) and one is `u16` (max 65535).
When the deserializer sees value `1000`, it can rule out `Small` without ever parsing into
the wrong type:

```rust
use facet_solver::{Solver, KeyResult, Schema};
# use facet::Facet;
# #[derive(Facet)]
# struct SmallPayload { value: u8 }
# #[derive(Facet)]
# struct LargePayload { value: u16 }
# #[derive(Facet)]
# #[repr(u8)]
# enum Payload { Small { payload: SmallPayload }, Large { payload: LargePayload } }
# #[derive(Facet)]
# struct Container { #[facet(flatten)] inner: Payload }

let schema = Schema::build(Container::SHAPE).unwrap();
let mut solver = Solver::new(&schema);

// "payload" exists in both - ambiguous by key alone
solver.probe_key(&[], "payload");

// "value" also exists in both, but with different types!
match solver.probe_key(&["payload"], "value") {
    KeyResult::Ambiguous { fields } => {
        // fields contains (FieldInfo, score) pairs for u8 and u16
        // Lower score = more specific type
        assert_eq!(fields.len(), 2);
    }
    _ => {}
}

// Deserializer sees value 1000 - ask which types fit
let shapes = solver.get_shapes_at_path(&["payload", "value"]);
let fits: Vec<_> = shapes.iter()
    .filter(|s| match s.type_identifier {
        "u8" => "1000".parse::<u8>().is_ok(),   // false!
        "u16" => "1000".parse::<u16>().is_ok(), // true
        _ => false,
    })
    .copied()
    .collect();

// Narrow to types the value actually fits
solver.satisfy_at_path(&["payload", "value"], &fits);
assert_eq!(solver.candidates().len(), 1);  // Solved: Large
```

This enables true streaming deserialization: you never buffer values, never parse
speculatively, and never lose precision. The solver tells you what types are possible,
you check which ones the raw input satisfies, and disambiguation happens lazily.

## Performance

- **O(1) field lookup**: Inverted index maps field names to bitmasks
- **O(configs/64) narrowing**: Bitwise AND to filter candidates
- **Zero allocation during solving**: Schema is built once, solving just manipulates bitmasks
- **Early termination**: Stops as soon as one candidate remains

Typical disambiguation: ~50ns for 4 configurations, <1µs for 64+ configurations.

## Why This Exists

Serde's `#[serde(flatten)]` and `#[serde(untagged)]` have fundamental limitations
because they buffer values into an intermediate `Content` enum, then re-deserialize.
This loses type information and breaks many use cases.

Facet takes a different approach: **determine the type first, then deserialize
directly**. No buffering, no loss of fidelity.

### Serde Issues This Resolves

| Issue | Problem | Facet's Solution |
|-------|---------|------------------|
| [serde#2186](https://github.com/serde-rs/serde/issues/2186) | Flatten buffers into `Content`, losing type distinctions (e.g., `1` vs `"1"`) | Scan keys only, deserialize values directly into the resolved type |
| [serde#1600](https://github.com/serde-rs/serde/issues/1600) | `flatten` + `deny_unknown_fields` doesn't work | Schema knows all valid fields per configuration |
| [serde#1626](https://github.com/serde-rs/serde/issues/1626) | `flatten` + `default` on enums | Solver tracks required vs optional per-field |
| [serde#1560](https://github.com/serde-rs/serde/issues/1560) | Empty variant ambiguity with "first match wins" | Explicit configuration enumeration, no guessing |
| [serde_json#721](https://github.com/serde-rs/json/issues/721) | `arbitrary_precision` + `flatten` loses precision | No buffering through `serde_json::Value` |
| [serde_json#1155](https://github.com/serde-rs/json/issues/1155) | `u128` in flattened struct fails | Direct deserialization, no `Value` intermediary |

## Sponsors

Thanks to all individual sponsors:

<p> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
    <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
    <img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
    </picture>
</a> </p>

...along with corporate sponsors:

<p> <a href="https://aws.amazon.com">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/aws-light.svg" height="40" alt="AWS">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

...without whom this work could not exist.

## Special thanks

The facet logo was drawn by [Misiasart](https://misiasart.com/).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
