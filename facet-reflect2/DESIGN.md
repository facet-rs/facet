# facet-reflect2 Design (Unified Path Model)

Partial value construction for facet: build values incrementally when data arrives out of order, with a unified path model and explicit frame tracking.

This document is the single source of truth for the API and semantics.

---

## 1. Overview

The core abstraction is a **frame tree**. Each frame represents a value being constructed — it holds a pointer to memory, type metadata (shape), and tracking state. A `Partial` manages this tree and has a **cursor** pointing at the current frame. Operations navigate and mutate the tree; `build()` validates and returns the final value.

Key goals:
- Build nested values incrementally.
- Support out-of-order input (deferred mode).
- Provide stable re-entry into partially built nodes.
- Keep semantics explicit and deterministic.

---

## 2. API

```rust
enum PathSegment {
    Field(u32),      // Struct field, tuple element, array index, enum variant
    Append,          // New list element, set element, or map entry
    Root,            // Jump to root frame (non-recursive deserializers)
}

type Path = &[PathSegment];

enum Source {
    Imm(Imm),               // Copy bytes from existing value
    Stage(Option<usize>),   // Push a frame (optional capacity hint)
    Default,                // Call Default::default() in place
}

enum Op {
    Set { dst: Path, src: Source },
    End,
}

// Function signature
fn apply(op: Op) -> Result<(), Error>;
```

---

## 3. Path Semantics

Paths are **relative to the current frame**.

| Segment | Meaning |
|---------|---------|
| `Field(n)` | Field/element `n` of the current frame. |
| `Append` | Create a new element at the end of a collection. |
| `Root` | Jump to the root frame (see rules below). |

### Multi-Level Paths

Paths can have multiple segments. **Multi-level paths implicitly create frames for all intermediate segments** as if `Stage` had been applied at each step.

Example:

- `Set { dst: &[Field(0), Field(1)], src: Imm(&20) }`

Is equivalent to:

- `Set { dst: &[Field(0)], src: Stage(None) }`
- `Set { dst: &[Field(1)], src: Imm(&20) }`

The cursor ends at the deepest frame implied by the final segment.

### Root

- `Root` is **only legal as the first segment** in a path.
- If `Root` appears mid-path, it is an error.
- `Root` **implicitly climbs** from the current frame to the root, as if repeatedly applying `End`.
- Climbing can trigger deferred validation (see §8).

---

## 4. Sources

| Source | Effect |
|--------|--------|
| `Imm` | Copy bytes from an existing value (caller must ensure safety). |
| `Stage(cap)` | Push a frame to build incrementally. `cap` hints at container size. |
| `Default` | Call `Default::default()` in place for the target. |

### Overwrite Semantics

If a target location already has a partially initialized subtree, and you apply:

- `Source::Imm` or `Source::Default` at that location:
  - The existing subtree is dropped.
  - The new value overwrites it.
  - The target is treated as **complete**.

This applies even in deferred mode.

---

## 5. Containers and Staging

### Lists (Vec, etc.)

- `Append` creates a new element frame (staged).
- Caller tracks indices for later re-entry (e.g., via `Field(n)`).
- Finalization converts staged elements to the list on `End` of the list frame (strict) or on exiting the deferred subtree (deferred).

### Maps

Maps use a **Direct Fill staging buffer**.

- `Append` creates a new entry frame containing `(Key, Value)` fields.
- You may build **key and value simultaneously**, in any order.
- Caller tracks the entry index for re-entry (`Field(n)` on the container).
- `Set { dst: &[Append], src: Imm }` is **invalid** (no tuple shortcut).

**Duplicate keys:** at finalization, the behavior is “last wins,” as defined by the map’s iterator/insertion semantics.

### Sets

Sets also use staging buffers.

- `Append` creates a new element frame.
- Elements can be partially built and re-entered by index.
- On finalization, elements are inserted into the set.

**Note:** Sets treat elements like staged entries; index is preserved on overwrite.

---

## 6. Enums and Variants

`Field(n)` at an enum frame selects variant `n`.

- **Unit variant:** select with `Source::Default` or `Source::Stage` then immediate `End`.
- **Tuple variant:** `Field(n)` enters the variant; subsequent `Field(k)` selects the tuple element.
- **Struct variant:** `Field(n)` enters; subsequent `Field(k)` selects struct fields.

`Set { dst: &[Field(n)], src: Imm(...) }` is allowed only when the variant’s representation matches and is compatible with `Imm` semantics (e.g., single-field tuple variant).

---

## 7. Frame Lifecycle

### Creation

A frame is created when:
- `Source::Stage` is used,
- a path uses `Append`,
- a multi-level path enters an uninitialized child.

### Collapse

When a frame is **complete**, it may be collapsed (freed) and its parent is updated to mark completion.

In deferred mode, frames are not eagerly collapsed if they might be re-entered; memory usage scales with O(nodes) rather than O(depth).

---

## 8. Strict vs Deferred Mode

Each frame has a `deferred` flag, and **deferred state applies to all descendants**. The `Partial` tracks the **rootmost deferred boundary** for fast checks.

### End Behavior

- **Strict Mode**: `End` requires the current frame to be complete.
- **Deferred Mode**: `End` on an incomplete frame stores it for later and pops to parent.

### Exiting Deferred Mode

You cannot move the cursor upstream of a deferred subtree without validating it.

When the cursor climbs *out of* a deferred subtree:
1. The subtree is validated in full.
2. If any incomplete nodes remain, this is an error.
3. Deferred state is cleared for that subtree.

`Root` behaves like repeated `End`: it triggers deferred validation as it climbs.

### Nested Deferred Frames

Nested deferred frames are allowed (e.g., multi-level flatten). The `Partial` tracks the **rootmost deferred boundary**.

---

## 9. Validation and Defaults

### Validation Timing

- **Strict mode**: validation occurs at each `End`.
- **Deferred mode**: validation occurs when exiting the deferred subtree or at `build()`.

### Default Rules

Defaults are applied at `End` according to the historical facet-reflect rules:

- Fields annotated with `#[facet(default)]` are default-initialized when missing.
- **Option** fields are an exception and **implicitly behave as `#[facet(default)]`** (missing → `None`).
- Missing fields **without** defaults are errors.
- If a struct is partially initialized and missing fields without defaults, the struct’s own `Default` impl is **not** used to fill missing fields.

---

## 10. Error Handling and Poisoning

Any error during an operation or finalization **poisons** the entire `Partial`.
Once poisoned:
- All subsequent operations return `Poisoned`.
- No recovery or retry.

This includes failures during finalization (e.g., allocation failure or insertion errors).

---

## 11. Examples

### Structs

- Set fields individually:
  - `Set { dst: &[Field(0)], src: Imm(&10) }`
  - `Set { dst: &[Field(1)], src: Imm(&20) }`

### Maps (staged)

1. Enter map:
   - `Set { dst: &[Field(0)], src: Stage(Some(1)) }`
2. Append entry:
   - `Set { dst: &[Append], src: Stage(None) }`
3. Build key:
   - `Set { dst: &[Field(0), Field(0)], src: Imm(&"/api") }`
   - `Set { dst: &[Field(0), Field(1)], src: Imm(&"POST") }`
4. Build value:
   - `Set { dst: &[Field(1)], src: Imm(&handler) }`
5. End entry:
   - `End`
6. End map (strict):
   - `End`

### Lists with re-entry (deferred)

1. Enter list:
   - `Set { dst: &[Field(0)], src: Stage(None) }`
2. Append new element:
   - `Set { dst: &[Append], src: Stage(None) }`
3. Set field 0:
   - `Set { dst: &[Field(0)], src: Imm(&"host") }`
4. `End` (element incomplete, stored)
5. Re-enter by index:
   - `Set { dst: &[Field(0)], src: Stage(None) }`
6. Set field 1:
   - `Set { dst: &[Field(1)], src: Imm(&8080) }`
7. `End`

---

## 12. Build

`build()` always navigates to root, performing `End` semantics as it climbs.
- If it pops a deferred boundary, it validates that entire subtree.
- If any incomplete nodes remain at that point, it errors.
- On success, it returns the fully constructed value.