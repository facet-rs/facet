# facet-format-toml Design

## Overview

`facet-format-toml` implements the `FormatParser` trait for TOML, enabling streaming deserialization that works with facet's deferred materialization. The key challenge is TOML's ability to "reopen" tables - fields for the same struct can appear at different points in the document.

## The Hard Problem: Table Reopening

In JSON, structs are contiguous:
```json
{"foo": {"bar": {"x": 1, "y": 2}}}
```
Events: `StructStart` → `FieldKey("foo")` → `StructStart` → `FieldKey("bar")` → `StructStart` → `FieldKey("x")` → `Scalar(1)` → `FieldKey("y")` → `Scalar(2)` → `StructEnd` → `StructEnd` → `StructEnd`

In TOML, the same struct's fields can be scattered:
```toml
[foo.bar]
x = 1

[foo.baz]
z = 3

[foo.bar]  # reopening!
y = 2
```

This is **valid TOML**. The `foo.bar` table receives `x` first, then later `y`.

## Solution: Graph Navigation Model

Instead of thinking of `StructEnd` / `SequenceEnd` as "we're done with this container forever", think of them as **navigating up the graph**. The deserializer continues processing until `None` (true EOF), not until `StructEnd` or `SequenceEnd`.

### Event Stream for the Example Above

```
FieldKey("foo") → StructStart    # enter foo
FieldKey("bar") → StructStart    # enter foo.bar
FieldKey("x") → Scalar(1)        # set foo.bar.x
StructEnd                        # navigate up to foo
FieldKey("baz") → StructStart    # enter foo.baz
FieldKey("z") → Scalar(3)        # set foo.baz.z
StructEnd                        # navigate up to foo
FieldKey("bar") → StructStart    # re-enter foo.bar
FieldKey("y") → Scalar(2)        # set foo.bar.y
StructEnd                        # navigate up to foo
StructEnd                        # navigate up to root
None                             # EOF
```

The key insight: `Partial` with deferred mode handles fields/elements arriving out of order, including repeated "re-entry" into the same field path. Validation happens once at the end.

## Parser State Machine

### Core State

```rust
struct TomlParser<'de> {
    input: &'de str,
    lexer: Lexer<'de>,

    /// Current path in the document (stack of table names).
    /// `["foo", "bar"]` means we're inside `[foo.bar]`.
    current_path: Vec<Cow<'de, str>>,

    /// Pending events to emit (navigation when tables change).
    pending_events: VecDeque<ParseEvent<'de>>,

    /// Current state in the state machine.
    state: ParserState,

    /// Peeked token for lookahead.
    peeked_token: Option<Token>,

    /// Cached event for peek_event().
    event_peek: Option<ParseEvent<'de>>,
}

enum ParserState {
    /// At document start, before any content.
    Start,
    /// Expecting a key-value pair or table header.
    ExpectKeyOrTable,
    /// Just saw a key, expecting `=`.
    ExpectEquals,
    /// Just saw `=`, expecting a value.
    ExpectValue,
    /// Inside an inline table `{ ... }`.
    InlineTable { first: bool },
    /// Inside an array `[ ... ]`.
    Array { first: bool },
    /// Document finished.
    Eof,
}
```

### Table Header Handling

When we encounter a table header like `[foo.bar.baz]`:

1. Parse the dotted key to get the new path: `["foo", "bar", "baz"]`
2. Compare with current path to compute navigation events
3. Emit `StructEnd` events to "pop up" to the common ancestor
4. Emit `FieldKey` + `StructStart` pairs to navigate down to the new table

```rust
fn compute_navigation_events(
    current: &[Cow<str>],
    target: &[Cow<str>],
) -> Vec<ParseEvent> {
    // Find common prefix length
    let common_len = current.iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut events = Vec::new();

    // Pop up to common ancestor
    for _ in common_len..current.len() {
        events.push(ParseEvent::StructEnd);
    }

    // Navigate down to target
    for name in &target[common_len..] {
        events.push(ParseEvent::FieldKey(FieldKey::new(
            name.clone(),
            FieldLocationHint::KeyValue,
        )));
        events.push(ParseEvent::StructStart(ContainerKind::Object));
    }

    events
}
```

### Example: Navigation Computation

Current path: `["foo", "bar"]`
New table header: `[foo.baz.qux]`

1. Common prefix: `["foo"]` (length 1)
2. Pop from current (length 2) to common (length 1): 1 `StructEnd`
3. Navigate from common to target: `FieldKey("baz")`, `StructStart`, `FieldKey("qux")`, `StructStart`

Result: `[StructEnd, FieldKey("baz"), StructStart, FieldKey("qux"), StructStart]`

## Array Tables `[[...]]`

Array tables like `[[servers]]` create array elements. Each occurrence appends a new struct to the array.

Critically, array tables can be **interleaved** with other tables/keys, and even nested array tables can appear between array table entries. We therefore treat `SequenceEnd` the same way we treat `StructEnd`: **navigation**, not "this array is complete forever".

```toml
[[servers]]
name = "alpha"

[database]
host = "localhost"

[[servers]]
name = "beta"
```

Events:
```
FieldKey("servers") → SequenceStart → StructStart  # first [[servers]] element
FieldKey("name") → Scalar("alpha")
StructEnd → SequenceEnd                             # navigate up to parent

FieldKey("database") → StructStart
FieldKey("host") → Scalar("localhost")
StructEnd                                           # navigate up to parent

FieldKey("servers") → SequenceStart → StructStart   # reopen servers, append element
FieldKey("name") → Scalar("beta")
StructEnd → SequenceEnd                             # navigate up to parent
```

This relies on `Partial` list construction semantics: beginning a list for a field is idempotent (it does not clear previously-added items), so re-entering the same list field later can append additional elements without any buffering or pre-scanning.

## Inline Tables and Arrays

Inline tables `{ key = value }` and arrays `[1, 2, 3]` are handled with a recursive state:

```rust
enum ParserState {
    // ...
    InlineTable { depth: usize, first: bool },
    Array { depth: usize, first: bool },
}
```

These don't interact with the table reopening logic - they're self-contained values.

## Token-to-Event Translation

### toml_parser Events → Our Events

| toml_parser Event | Our ParseEvent |
|-------------------|----------------|
| `StdTableOpen` + keys + `StdTableClose` | Navigation events (computed) |
| `ArrayTableOpen` + keys + `ArrayTableClose` | Navigation + `FieldKey` + `SequenceStart` + `StructStart` (each occurrence appends an element) |
| `SimpleKey` | `FieldKey` |
| `Scalar` | `Scalar` (with decoded value) |
| `InlineTableOpen` | `StructStart(Object)` |
| `InlineTableClose` | `StructEnd` |
| `ArrayOpen` | `SequenceStart(Array)` |
| `ArrayClose` | `SequenceEnd` |

### Scalar Decoding

TOML scalars need type detection and decoding:

```rust
fn decode_scalar(raw: Raw<'_>, source: Source<'_>) -> ScalarValue {
    let mut output = Cow::Borrowed("");
    let kind = raw.decode_scalar(&mut output, &mut ());

    match kind {
        ScalarKind::String => ScalarValue::Str(output),
        ScalarKind::Boolean(b) => ScalarValue::Bool(b),
        ScalarKind::Integer(radix) => {
            // Parse with appropriate radix
            let n: i64 = parse_int(&output, radix);
            ScalarValue::I64(n)
        }
        ScalarKind::Float => {
            let f: f64 = output.parse().unwrap();
            ScalarValue::F64(f)
        }
        ScalarKind::DateTime => {
            // Keep as string, let facet-reflect handle datetime types
            ScalarValue::Str(output)
        }
    }
}
```

## EOF Handling

The recent `Option<ParseEvent>` change enables this design:

- `next_event()` returns `Ok(Some(event))` for each event
- `next_event()` returns `Ok(None)` at true end-of-file
- `StructEnd` / `SequenceEnd` are just navigation, not termination

The deserializer continues calling `next_event()` until `None`, allowing Partial to accumulate all fields regardless of document order.

## Probing (Untagged Enums)

For `begin_probe()`, we need to collect field evidence without consuming. Strategy:

1. Take a parser checkpoint (position + any relevant stacks)
2. Temporarily advance to collect field names and scalar values
3. Restore the checkpoint (so the real deserialization stream is unchanged)
4. Return a `ProbeStream` over the collected evidence

## Implementation Phases

### Phase 1: Basic Parsing
- Lexer integration
- Simple key-value pairs at root level
- Scalar decoding (strings, integers, floats, booleans)

### Phase 2: Table Navigation
- Table header parsing `[table]`
- Navigation event computation
- Path tracking

### Phase 3: Nested Structures
- Array tables `[[array]]`
- Inline tables `{ ... }`
- Inline arrays `[...]`

### Phase 4: Full Compliance
- Probing support
- DateTime handling
- Error recovery and diagnostics
- FormatSuite tests

## Open Questions

1. **Dotted keys in values**: `foo.bar.baz = 1` creates nested tables inline. Do we emit navigation events, or treat it as a single assignment?

   Answer: We should emit navigation events. `foo.bar.baz = 1` at root is equivalent to:
   ```
   FieldKey("foo") → StructStart
   FieldKey("bar") → StructStart
   FieldKey("baz") → Scalar(1)
   StructEnd → StructEnd
   ```

2. **Raw capture**: Should `capture_raw()` return the TOML text? Probably not - TOML is rarely embedded as raw text in the way JSON is (`RawJson`).

3. **JIT support**: Tier-2 JIT for TOML would require format-specific Cranelift IR. Low priority given TOML's complexity.
