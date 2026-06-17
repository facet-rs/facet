+++
title = "Solver"
weight = 5
insert_anchor_links = "heading"
+++

The **solver** helps format crates implement `#[facet(flatten)]` and `#[facet(untagged)]` correctly, efficiently, and with useful diagnostics. It answers the question: "given the fields I've seen so far, which variant(s) could this be?"

## The problem

Consider a type with a flattened enum:

```rust,noexec
use facet::Facet;

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

When deserializing JSON, we don't know which variant to use until we've seen the fields:

```json
{"id": "msg-1", "content": "hello"}
{"id": "msg-2", "data": [1,2,3], "encoding": "raw"}
```

The first has `content` → must be `Text`. The second has `data` and `encoding` → must be `Binary`.

Without a solver, you'd have to:
1. Buffer all values into an intermediate representation
2. Try each variant until one works
3. Re-deserialize from the buffer

This is what serde does, and it has fundamental problems (more on that later).

## The solution: configuration-based disambiguation

The solver pre-computes all valid "configurations" — unique combinations of fields that can appear together. Then it uses an inverted index to quickly narrow down which configuration(s) match as you see fields.

```
┌─────────────────────────────────────────────────────────────┐
│                         Schema                              │
├─────────────────────────────────────────────────────────────┤
│  Configuration 0 (Text):                                    │
│    fields: { "id", "content" }                              │
│    path to variant: payload → Text                          │
│                                                             │
│  Configuration 1 (Binary):                                  │
│    fields: { "id", "data", "encoding" }                     │
│    path to variant: payload → Binary                        │
├─────────────────────────────────────────────────────────────┤
│  Inverted Index:                                            │
│    "id"       → [Config 0, Config 1]  (both have it)        │
│    "content"  → [Config 0]            (only Text)           │
│    "data"     → [Config 1]            (only Binary)         │
│    "encoding" → [Config 1]            (only Binary)         │
└─────────────────────────────────────────────────────────────┘
```

## Basic usage

```rust,noexec
use facet_solver::{KeyResult, Schema, Solver};

// Build schema once (can be cached per-type)
let schema = Schema::build(Message::SHAPE).unwrap();

// Create a solver for this deserialization
let mut solver = Solver::new(&schema);

// As you see fields, report them:
match solver.see_key("id") {
    KeyResult::Unambiguous { .. } => {
        // Both configs have "id" — still ambiguous
    }
    _ => {}
}

match solver.see_key("content") {
    KeyResult::Solved(config) => {
        // Only Text has "content" — we now know the variant!
        println!("Resolved to: {:?}", config);
    }
    _ => {}
}
```

## How disambiguation works

The solver maintains a bitmask of candidate configurations. Each time you report a key, it ANDs the bitmask with the set of configurations that have that key:

```
Initial:      [1, 1]     ← Both configs are candidates

see_key("id"):
  "id" mask:  [1, 1]     ← Both have "id"
  Result:     [1, 1]     ← Still ambiguous (2 candidates)

see_key("content"):
  "content":  [1, 0]     ← Only Config 0 has "content"
  Result:     [1, 0]     ← SOLVED! Only Config 0 remains
```

This is O(1) per key lookup and O(configs/64) for the bitwise AND — extremely fast.

## Nested field disambiguation

Sometimes top-level keys don't distinguish variants:

```rust,noexec
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

Both variants have an `inner` field. But `inner.content` only exists in `Text`, and `inner.bytes` only exists in `Binary`:

```
┌──────────────────────────────────────────────────────┐
│  Wrapper                                             │
├──────────────────────────────────────────────────────┤
│                    ┌──────────────────┐              │
│                    │ Payload (enum)   │              │
│                    ├──────────────────┤              │
│         ┌──────────┴──────────┐       │              │
│         ▼                     ▼       │              │
│  ┌─────────────┐      ┌─────────────┐ │              │
│  │ Text        │      │ Binary      │ │              │
│  │ inner: ──┐  │      │ inner: ──┐  │ │              │
│  └──────────│──┘      └──────────│──┘ │              │
│             ▼                    ▼                   │
│  ┌─────────────────┐  ┌─────────────────┐            │
│  │ TextPayload     │  │ BinaryPayload   │            │
│  │ content: String │  │ bytes: Vec<u8>  │            │
│  └─────────────────┘  └─────────────────┘            │
└──────────────────────────────────────────────────────┘

Disambiguation paths:
  • inner         → ambiguous (both have it)
  • inner.content → Text only
  • inner.bytes   → Binary only
```

The `ProbingSolver` handles nested disambiguation:

```rust,noexec
use facet_solver::{ProbingSolver, ProbeResult, Schema};

let schema = Schema::build(Wrapper::SHAPE).unwrap();
let mut solver = ProbingSolver::new(&schema);

// Top-level "inner" doesn't disambiguate
assert!(matches!(
    solver.probe_key(&[], "inner"),
    ProbeResult::KeepGoing
));

// But "inner.content" does!
match solver.probe_key(&["inner"], "content") {
    ProbeResult::Solved(config) => {
        // We know it's Text!
    }
    _ => panic!("should have solved"),
}
```

## Type-based disambiguation

Sometimes variants have **identical keys** but different value types:

```rust,noexec
#[derive(Facet)]
struct SmallPayload { value: u8 }   // max 255

#[derive(Facet)]
struct LargePayload { value: u16 }  // max 65535

#[derive(Facet)]
#[repr(u8)]
enum Payload {
    Small { payload: SmallPayload },
    Large { payload: LargePayload },
}
```

Both have `payload.value`, but with different types. When the deserializer sees the value `1000`, it can rule out `Small` without ever parsing into the wrong type:

```
┌──────────────────────────────────────────────────────┐
│  Input JSON: {"payload": {"value": 1000}}            │
├──────────────────────────────────────────────────────┤
│                                                      │
│  1. See key "payload" → ambiguous                    │
│  2. See key "payload.value" → still ambiguous!      │
│     (both have it, different types)                  │
│                                                      │
│  3. Check value "1000":                              │
│     • Can parse as u8?  → NO (255 max)              │
│     • Can parse as u16? → YES                        │
│                                                      │
│  4. Narrow by satisfied types → SOLVED: Large        │
│                                                      │
└──────────────────────────────────────────────────────┘
```

```rust,noexec
use facet_solver::{Solver, KeyResult, Schema};

let schema = Schema::build(Container::SHAPE).unwrap();
let mut solver = Solver::new(&schema);

// Both have "payload.value" with different types
solver.probe_key(&[], "payload");
solver.probe_key(&["payload"], "value");

// Get the possible shapes at this path
let shapes = solver.get_shapes_at_path(&["payload", "value"]);

// Check which types the actual value fits
let fits: Vec<_> = shapes.iter()
    .filter(|s| match s.type_identifier {
        "u8" => "1000".parse::<u8>().is_ok(),   // false!
        "u16" => "1000".parse::<u16>().is_ok(), // true
        _ => false,
    })
    .copied()
    .collect();

// Narrow to types the value satisfies
solver.satisfy_at_path(&["payload", "value"], &fits);
assert_eq!(solver.candidates().len(), 1);  // Solved!
```

This enables **true streaming deserialization**: you never buffer values, never parse speculatively, and never lose precision.

## Performance characteristics

| Operation | Complexity | Typical Time |
|-----------|------------|--------------|
| Schema::build | O(variants × fields) | ~1-10µs (cacheable) |
| Solver::new | O(1) | ~10ns |
| see_key | O(1) lookup + O(configs/64) AND | ~20-50ns |
| probe_key (nested) | O(path_len) | ~50-100ns |

The solver uses:
- **Inverted index**: Maps field names to bitmasks of configurations that have them
- **Bitwise operations**: All narrowing is bitwise AND on u64/u128 masks
- **Zero allocation**: Schema built once, solving just manipulates stack-allocated bitmasks
- **Early termination**: Stops immediately when one candidate remains

## Why not buffer like serde?

Serde's `#[serde(flatten)]` and `#[serde(untagged)]` buffer values into an intermediate `Content` enum, then re-deserialize. This has fundamental problems:

```
Serde's approach:
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  JSON       │ ──▶ │  Content    │ ──▶ │  Target     │
│  Input      │     │  (buffer)   │     │  Type       │
└─────────────┘     └─────────────┘     └─────────────┘
                          │
                          ▼
                    Type info lost!
                    • 1 vs "1" conflated
                    • u128 precision lost
                    • Borrowing impossible

Facet's approach:
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  JSON       │ ──▶ │  Solver     │ ──▶ │  Target     │
│  Input      │     │  (keys only)│     │  Type       │
└─────────────┘     └─────────────┘     └─────────────┘
                          │
                          ▼
                    No buffering!
                    • Direct deserialization
                    • Full type fidelity
                    • Zero-copy possible
```

### Issues facet resolves

| Serde Issue | Problem | Facet's Solution |
|-------------|---------|------------------|
| [#2186](https://github.com/serde-rs/serde/issues/2186) | Flatten loses type distinctions (`1` vs `"1"`) | Scan keys only, deserialize values directly |
| [#1600](https://github.com/serde-rs/serde/issues/1600) | `flatten` + `deny_unknown_fields` broken | Schema knows all valid fields per config |
| [#1626](https://github.com/serde-rs/serde/issues/1626) | `flatten` + `default` on enums | Solver tracks required vs optional |
| [#1560](https://github.com/serde-rs/serde/issues/1560) | Empty variant ambiguity | Explicit enumeration, no guessing |
| [json#721](https://github.com/serde-rs/json/issues/721) | `arbitrary_precision` + `flatten` loses precision | No `Value` intermediary |
| [json#1155](https://github.com/serde-rs/json/issues/1155) | `u128` in flattened struct fails | Direct deserialization |

## Integration pattern

Here's how a format crate typically integrates the solver:

```rust,noexec
use facet_solver::{Schema, Solver, KeyResult};

fn deserialize_object<T: Facet>(input: &str) -> Result<T, Error> {
    // 1. Build or retrieve cached schema
    let schema = Schema::build(T::SHAPE)?;
    let mut solver = Solver::new(&schema);

    // 2. First pass: scan keys to disambiguate
    for key in parse_keys(input) {
        match solver.see_key(&key) {
            KeyResult::Solved(config) => {
                // We know the variant! Deserialize directly.
                return deserialize_with_config(input, config);
            }
            KeyResult::Unambiguous { .. } => continue,
            KeyResult::Ambiguous { .. } => continue,
            KeyResult::Unknown => {
                // Handle unknown field based on deny_unknown_fields
            }
        }
    }

    // 3. If still ambiguous after all keys, check defaults/optionals
    match solver.finalize() {
        Some(config) => deserialize_with_config(input, config),
        None => Err(Error::AmbiguousVariant),
    }
}
```

## API reference

### Schema

```rust,noexec
impl Schema {
    /// Build a schema from a Shape (cacheable per-type)
    pub fn build(shape: &'static Shape) -> Result<Schema, SchemaError>;

    /// Number of configurations in this schema
    pub fn config_count(&self) -> usize;
}
```

### Solver

```rust,noexec
impl Solver<'_> {
    /// Create a new solver from a schema
    pub fn new(schema: &Schema) -> Solver;

    /// Report seeing a key at the current path
    pub fn see_key(&mut self, key: &str) -> KeyResult;

    /// Probe a key at a nested path
    pub fn probe_key(&mut self, path: &[&str], key: &str) -> KeyResult;

    /// Get possible shapes at a path (for type disambiguation)
    pub fn get_shapes_at_path(&self, path: &[&str]) -> Vec<&Shape>;

    /// Narrow by types that satisfy the value
    pub fn satisfy_at_path(&mut self, path: &[&str], shapes: &[&Shape]);

    /// Current candidate configurations
    pub fn candidates(&self) -> &[Configuration];
}
```

### KeyResult

```rust,noexec
pub enum KeyResult<'a> {
    /// Only one configuration remains — solved!
    Solved(Configuration<'a>),

    /// Multiple configs have this key — keep going
    Unambiguous { fields: Vec<FieldInfo<'a>> },

    /// Multiple configs, but they have different types for this key
    Ambiguous { fields: Vec<(FieldInfo<'a>, Score)> },

    /// No configuration has this key
    Unknown,
}
```

## Next steps

- See [Build a Format Crate](@/extend/format-crate.md) for the full architecture
- Check the [facet-json source](https://github.com/facet-rs/facet/tree/main/facet-json) for a real integration
- Read about [Partial](@/extend/partial.md) for constructing values after disambiguation
