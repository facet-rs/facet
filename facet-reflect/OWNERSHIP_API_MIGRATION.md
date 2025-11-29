# Partial Ownership API Migration Guide

This document describes the transition from `&mut self` (mutable reference) API to `self` (ownership) API for `Partial`.

## Why This Change?

**Before:** Methods took `&mut self` and returned `Result<&mut Self, ReflectError>`. On error, the Partial was "poisoned" and subsequent operations would fail.

**After:** Methods take `self` (ownership) and return `Result<Self, ReflectError>`. On error, the Partial is consumed/dropped - no poisoning needed. Cleanup happens automatically in `Drop`.

### Benefits

1. **Simpler error handling** - No poisoning logic, errors naturally consume the value
2. **Cleaner semantics** - Ownership transfer makes state transitions explicit
3. **No invalid states** - Can't accidentally use a poisoned Partial
4. **Rust-idiomatic** - Follows builder pattern conventions

---

## Pattern Reference

### 1. Simple Method Calls

```rust
// BEFORE: Mutable reference, ignore result
let _ = partial.begin_field("name");
let _ = partial.set(42u32);
let _ = partial.end();

// AFTER: Ownership transfer, reassign on success
partial = partial.begin_field("name")?;
partial = partial.set(42u32)?;
partial = partial.end()?;

// AFTER (ignoring errors - value consumed on error):
if let Ok(p) = partial.begin_field("name") {
    partial = p;
}
```

### 2. Method Chaining

```rust
// BEFORE: Chain returns &mut Self
partial
    .begin_field("name")?
    .set(42u32)?
    .end()?;

// AFTER: Chain returns Self - SAME SYNTAX!
partial = partial
    .begin_field("name")?
    .set(42u32)?
    .end()?;

// Or without intermediate binding:
let result = Partial::alloc::<MyStruct>()?
    .begin_field("name")?
    .set(42u32)?
    .end()?
    .build()?;
```

### 3. Loops in Deserializers

```rust
// BEFORE: Pass mutable reference through loop
fn deserialize_list(&mut self, partial: &mut Partial<'_>) -> Result<(), Error> {
    partial.begin_list()?;
    for item in items {
        partial.begin_list_item()?;
        self.deserialize_value(partial)?;
        partial.end()?;
    }
    Ok(())
}

// AFTER: Reassign in loop
fn deserialize_list(&mut self, partial: Partial<'_>) -> Result<Partial<'_>, Error> {
    let mut partial = partial.begin_list()?;
    for item in items {
        partial = partial.begin_list_item()?;
        partial = self.deserialize_value(partial)?;
        partial = partial.end()?;
    }
    Ok(partial)
}
```

### 4. Deserializer Method Signatures

```rust
// BEFORE: Take &mut, return ()
fn deserialize_into(&mut self, wip: &mut Partial<'_>) -> Result<(), DeserError> {
    // ... modify wip ...
    Ok(())
}

// AFTER: Take ownership, return Partial
fn deserialize_into(&mut self, wip: Partial<'_>) -> Result<Partial<'_>, DeserError> {
    // ... transform wip ...
    Ok(wip)
}
```

### 5. Conditional/Branching Deserialization

```rust
// BEFORE: Use same mutable reference in branches
fn deserialize_value(&mut self, partial: &mut Partial<'_>) -> Result<(), Error> {
    match self.peek()? {
        Token::Number => {
            partial.set(self.parse_number()?)?;
        }
        Token::String => {
            partial.set(self.parse_string()?)?;
        }
        Token::Array => {
            self.deserialize_list(partial)?;
        }
    }
    Ok(())
}

// AFTER: Return Partial from each branch
fn deserialize_value(&mut self, partial: Partial<'_>) -> Result<Partial<'_>, Error> {
    match self.peek()? {
        Token::Number => {
            Ok(partial.set(self.parse_number()?)?)
        }
        Token::String => {
            Ok(partial.set(self.parse_string()?)?)
        }
        Token::Array => {
            self.deserialize_list(partial)
        }
    }
}
```

### 6. Error Handling (No More Poisoning)

```rust
// BEFORE: Errors poison the Partial, need explicit cleanup
fn do_something(partial: &mut Partial<'_>) -> Result<(), Error> {
    if let Err(e) = partial.begin_field("x") {
        // Partial is now poisoned, can't use it
        return Err(e.into());
    }
    // ...
}

// AFTER: Errors consume the Partial - it's gone
fn do_something(partial: Partial<'_>) -> Result<Partial<'_>, Error> {
    let partial = partial.begin_field("x")?;  // On error, partial is dropped
    // ...
    Ok(partial)
}
```

### 7. Building and Materializing

```rust
// BEFORE: TypedPartial wrapper
let typed = Partial::alloc::<MyStruct>()?;
let result = typed.build()?;  // Returns Box<MyStruct>
let value = *result;

// AFTER: Partial::alloc returns Partial directly, use build + materialize
let partial = Partial::alloc::<MyStruct>()?;
let partial = partial.begin_field("x")?;
let partial = partial.set(42)?;
let partial = partial.end()?;
let heap_value = partial.build()?;
let value: MyStruct = heap_value.materialize()?;

// Or chained:
let value: MyStruct = Partial::alloc::<MyStruct>()?
    .begin_field("x")?
    .set(42)?
    .end()?
    .build()?
    .materialize()?;
```

### 8. Fuzzer Pattern (Option<Partial>)

```rust
// BEFORE: Pass mutable reference repeatedly
fn apply_op(partial: &mut Partial<'_>, op: &Op) {
    match op {
        Op::BeginField(name) => { let _ = partial.begin_field(name); }
        Op::Set(value) => { let _ = partial.set(value); }
        Op::End => { let _ = partial.end(); }
    }
}

for op in ops {
    apply_op(&mut partial, op);
}

// AFTER: Use Option to handle consumption
fn apply_op(partial: &mut Option<Partial<'_>>, op: &Op) {
    let Some(p) = partial.take() else { return };

    let result = match op {
        Op::BeginField(name) => p.begin_field(name),
        Op::SetU32(value) => p.set(*value),
        Op::End => p.end(),
    };

    // Put back on success, leave as None on error (consumed)
    *partial = result.ok();
}

let mut partial = Some(Partial::alloc::<T>().unwrap());
for op in ops {
    apply_op(&mut partial, op);
}
// partial is None if any operation failed
```

### 9. Deferred Mode

```rust
// BEFORE
partial.begin_deferred(resolution)?;
partial.begin_field("a")?;
partial.set(1)?;
partial.end()?;
partial.finish_deferred()?;

// AFTER - same pattern, just reassign
partial = partial.begin_deferred(resolution)?;
partial = partial.begin_field("a")?;
partial = partial.set(1)?;
partial = partial.end()?;
partial = partial.finish_deferred()?;
```

### 10. Reading State (Non-Mutating Methods)

```rust
// BEFORE & AFTER: These don't change
let shape = partial.shape();           // &self -> &Shape
let count = partial.frame_count();     // &self -> usize
let is_def = partial.is_deferred();    // &self -> bool

// These stay as &self methods, no ownership transfer needed
```

### 11. Build (Terminal Operation)

```rust
// BEFORE: Consumes internal state, returns HeapValue
let result = partial.build()?;
// partial is now in "Built" state, unusable

// AFTER: Consumes Partial entirely
let result = partial.build()?;
// partial no longer exists (moved into build)
```

### 12. Nested Deserializer Calls

```rust
// BEFORE
fn deserialize_struct(&mut self, partial: &mut Partial<'_>) -> Result<(), Error> {
    for field in fields {
        partial.begin_field(field.name)?;
        self.deserialize_value(partial)?;  // partial passed by mut ref
        partial.end()?;
    }
    Ok(())
}

// AFTER
fn deserialize_struct(&mut self, partial: Partial<'_>) -> Result<Partial<'_>, Error> {
    let mut partial = partial;
    for field in fields {
        partial = partial.begin_field(field.name)?;
        partial = self.deserialize_value(partial)?;  // partial passed by value, returned
        partial = partial.end()?;
    }
    Ok(partial)
}
```

---

## Method Signature Changes

### Partial Methods

| Method | Before | After |
|--------|--------|-------|
| `begin_field` | `(&mut self, name) -> Result<&mut Self>` | `(self, name) -> Result<Self>` |
| `begin_nth_field` | `(&mut self, idx) -> Result<&mut Self>` | `(self, idx) -> Result<Self>` |
| `set<T>` | `(&mut self, value) -> Result<&mut Self>` | `(self, value) -> Result<Self>` |
| `end` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_list` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_list_item` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_map` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_key` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_value` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_set` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_set_item` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_some` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_inner` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_smart_ptr` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `begin_deferred` | `(&mut self, res) -> Result<&mut Self>` | `(self, res) -> Result<Self>` |
| `finish_deferred` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `set_default` | `(&mut self) -> Result<&mut Self>` | `(self) -> Result<Self>` |
| `build` | `(&mut self) -> Result<HeapValue>` | `(self) -> Result<HeapValue>` |

### Methods That Stay `&self`

- `shape(&self) -> &'static Shape`
- `try_shape(&self) -> Option<&'static Shape>`
- `frame_count(&self) -> usize`
- `is_active(&self) -> bool`
- `is_deferred(&self) -> bool`
- `deferred_resolution(&self) -> Option<&Resolution>`

---

## Removed Concepts

### Poisoning

```rust
// REMOVED: poison_and_cleanup()
// REMOVED: is_poisoned() / is_active() checks in methods
// REMOVED: PartialState::BuildFailed for poisoning

// Instead: Drop handles all cleanup automatically
// If an error occurs, the Partial is simply dropped
```

### require_active() Check

```rust
// BEFORE: Every method started with
self.require_active()?;

// AFTER: Not needed - if you have a Partial, it's valid
// Invalid states are impossible because errors consume the value
```

---

## Files to Update

### Core Partial Implementation
- `facet-reflect/src/partial/partial_api/misc.rs`
- `facet-reflect/src/partial/partial_api/fields.rs`
- `facet-reflect/src/partial/partial_api/lists.rs`
- `facet-reflect/src/partial/partial_api/maps.rs`
- `facet-reflect/src/partial/partial_api/sets.rs`
- `facet-reflect/src/partial/partial_api/set.rs`
- `facet-reflect/src/partial/partial_api/option.rs`
- `facet-reflect/src/partial/partial_api/eenum.rs`
- `facet-reflect/src/partial/partial_api/ptr.rs`
- `facet-reflect/src/partial/partial_api/build.rs`
- `facet-reflect/src/partial/partial_api/shorthands.rs`
- `facet-reflect/src/partial/partial_api/internal.rs`
- `facet-reflect/src/partial/mod.rs`

### Deserializers
- `facet-json/src/deserialize.rs`
- `facet-yaml/src/deserialize.rs`
- `facet-toml/src/deserialize/streaming.rs`
- `facet-msgpack/src/deserialize.rs`
- `facet-kdl/src/deserialize.rs`
- `facet-xdr/src/lib.rs` (already uses ownership pattern!)
- `facet-csv/src/deserialize.rs`
- `facet-urlencoded/src/deserialize.rs`

### Fuzzers
- `facet-reflect/fuzz/fuzz_targets/fuzz_partial.rs`
- `facet-value/fuzz/fuzz_targets/fuzz_value.rs`

### Tests
- `facet-reflect/tests/partial/*.rs`
- `facet-reflect/tests/leak_repro.rs`
- Various integration tests in other crates

---

## Migration Checklist

1. [x] Update `Partial` method signatures in `partial_api/*.rs`
2. [x] Remove `poison_and_cleanup()` and related logic
3. [x] Remove `require_active()` checks
4. [x] Remove `TypedPartial` wrapper (use `Partial` directly with `build()?.materialize()`)
5. [x] Update fuzzer to use `Option<Partial>` pattern
6. [x] Update each deserializer (JSON, YAML, TOML, etc.)
7. [x] Update all tests
8. [x] Run full test suite
9. [ ] Run fuzzers to verify no crashes
