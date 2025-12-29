# facet-toml Deserializer Design

This document describes the architecture of facet-toml's streaming deserializer,
which is built on `toml_parser`'s push-based event system.

## The TOML Challenge

TOML has a unique property that makes streaming deserialization non-trivial:
**keys can appear in any order**.

```toml
foo.bar.x = 1
foo.baz = 2      # back to foo level
foo.bar.y = 3    # back into foo.bar
```

This is valid TOML. A streaming parser sees these as three separate key-value
events, but they're writing to interleaved locations in the type tree:
- `foo.bar.x`
- `foo.baz`
- `foo.bar.y`

In JSON or YAML, objects are self-contained—you finish `foo.bar` before moving
to `foo.baz`. TOML doesn't guarantee this.

## Layer 1: Always Deferred Mode

Because of TOML's key ordering, we **always** use facet-reflect's deferred
materialization mode. This is not optional.

```rust,ignore
let mut partial = Partial::alloc::<T>()?;
partial.begin_deferred(resolution);  // First thing we do
```

### What Deferred Mode Provides

In deferred mode:

1. **Frames are stored, not discarded** — When you navigate out of a nested
   field, the frame is saved (keyed by path) rather than validated and dropped.

2. **Frames are restored on re-entry** — When you navigate back to the same
   path, the stored frame is retrieved with all its state intact.

3. **Validation happens at the end** — A final `finish_deferred()` call
   validates that everything is properly initialized.

This lets us handle TOML's arbitrary key ordering:

```rust,ignore
// Event: foo.bar.x = 1
partial.begin_field("foo")?;
partial.begin_field("bar")?;
partial.set_field("x", 1)?;
partial.end()?;  // bar frame stored at ["foo", "bar"]
partial.end()?;  // foo frame stored at ["foo"]

// Event: foo.baz = 2
partial.begin_field("foo")?;  // foo frame restored!
partial.set_field("baz", 2)?;
partial.end()?;  // foo frame stored again

// Event: foo.bar.y = 3
partial.begin_field("foo")?;  // foo frame restored
partial.begin_field("bar")?;  // bar frame restored, x is still set!
partial.set_field("y", 3)?;
partial.end()?;
partial.end()?;

// End of document
partial.finish_deferred()?;  // Validates everything is initialized
```

## Layer 2: Buffering for Flatten Disambiguation

On top of deferred mode, we need another mechanism for `#[facet(flatten)]` enums.

When a struct has a flattened enum:

```rust,ignore
struct Message {
    id: String,
    #[facet(flatten)]
    payload: MessagePayload,  // Text has "content", Binary has "data"+"encoding"
}
```

We don't know which variant to use until we've seen which keys are present.
We can't call `partial.select_variant(...)` until we know the answer.

### The Solver Integration

facet-solver pre-computes all valid field combinations for a type. During
deserialization:

1. **Detect flatten context** — When entering a type with flattened enums
2. **Start buffering events** — Don't call `Partial` methods yet
3. **Feed keys to solver** — As events arrive, report keys to narrow candidates
4. **Resolve** — When solver determines the variant (or we hit end of object)
5. **Replay buffered events** — Now we know which variant, so we can call
   the right `Partial` methods
6. **Resume normal flow** — Continue in deferred mode

```text
Push parser event
    ↓
[Always in deferred mode]
    ↓
Is this a flatten context?
    ├─ No  → Call Partial methods directly (deferred handles out-of-order)
    └─ Yes → Buffer + solver
                 ↓
             Resolved?
                 ├─ No  → Keep buffering
                 └─ Yes → Replay buffer through Partial (still deferred)
    ↓
End of document
    ↓
finish_deferred() → validate everything
```

### Why Buffer During Disambiguation?

We buffer events (not just keys) because:

1. **We need to replay values** — After resolving, we need to actually
   deserialize the values we skipped
2. **Push parser can't seek** — Unlike facet-json which re-reads from byte
   offsets, a push parser doesn't let us go back
3. **Minimal buffering** — We only buffer during the disambiguation window,
   not the entire document

## The Push Parser Challenge

`toml_parser` is push-based: you implement `EventReceiver`, and the parser
calls your methods. You don't control the flow.

For our model, we need:
- **Peek** — Look at events without consuming (for solver)
- **Buffer** — Store events during disambiguation
- **Replay** — Process buffered events after resolution

The solution is to buffer events into a `Vec` when entering a flatten context,
then replay them. Outside of flatten contexts, events flow directly to
`Partial` methods (still in deferred mode).

## Future Optimizations

These are not implemented initially, but could be added:

1. **Early validation of complete subtrees** — If a nested struct is fully
   initialized and has no side effects, validate it immediately rather than
   waiting for `finish_deferred()`

2. **Streaming validation** — Track which subtrees are "closed" (no more
   dotted keys can add to them) and validate incrementally

3. **Schema caching** — Cache `Schema` instances for types we see repeatedly

## Summary

| Aspect | Mechanism |
|--------|-----------|
| Out-of-order TOML keys | Deferred materialization (always on) |
| Flatten disambiguation | Event buffering + facet-solver |
| Push parser adaptation | Buffer during flatten, direct otherwise |
| Validation | `finish_deferred()` at document end |
