+++
title = "Variance and Soundness"
weight = 15
+++

Variance is a fundamental concept in Rust's type system that affects how lifetime parameters interact with subtyping. Facet tracks variance at runtime to enable safe reflection APIs.

## What is variance?

Variance determines whether you can substitute a type with a different lifetime:

- **Covariant**: A longer lifetime can be used where a shorter one is expected. `&'static str` can be used as `&'a str`.
- **Contravariant**: A shorter lifetime can be used where a longer one is expected. `fn(&'a str)` can accept `fn(&'static str)`.
- **Invariant**: No substitution allowed. The lifetime must match exactly.

```rust,noexec
// Covariant: &'a T
fn takes_ref<'a>(r: &'a str) {}
let s: &'static str = "hello";
takes_ref(s);  // OK: 'static -> 'a

// Contravariant: fn(&'a T)
fn takes_fn<'a>(f: fn(&'a str)) {}
fn static_fn(s: &'static str) {}
// takes_fn(static_fn);  // Would need 'a -> 'static

// Invariant: &'a mut T
fn takes_mut<'a>(r: &'a mut String) {}
// Cannot change the lifetime of a mutable reference
```

## Why does this matter for reflection?

Facet's `Peek` type lets you read values at runtime. Without careful design, reflection could allow **lifetime laundering** — converting a value with one lifetime to a different lifetime, leading to use-after-free bugs.

### The problem

Consider a type that contains a function pointer:

```rust,noexec
#[derive(Facet)]
struct FnWrapper<'a> {
    f: fn(&'a str),  // Contravariant in 'a
}
```

If reflection allowed lifetime changes, you could:
1. Create a `FnWrapper<'static>` containing `fn(&'static str)`
2. Use reflection to "cast" it to `FnWrapper<'short>`
3. Call the function with a `&'short str` that goes out of scope
4. The function still expects `&'static str` — **use-after-free!**

### The solution

`Peek` is **invariant** over its `'facet` lifetime parameter. This means you cannot change the lifetime through reflection at all:

```rust,noexec
// This is enforced at compile time
fn launder<'a>(p: Peek<'_, 'static>) -> Peek<'_, 'a> {
    p  // ERROR: cannot coerce Peek<'_, 'static> to Peek<'_, 'a>
}
```

## Variance tracking in Shape

Every `Shape` in facet has a `variance` field that records the type's variance:

```rust,noexec
pub struct Shape {
    // ... other fields ...

    /// Variance of this type with respect to its lifetime parameter.
    pub variance: Variance,
}

pub enum Variance {
    Covariant,
    Contravariant,
    Invariant,  // Default
}
```

Currently, all types default to `Invariant` (the safe choice). Future versions may compute variance automatically based on field types.

## Variance rules

When combining types, variance follows these rules:

| Type A | Type B | Combined |
|--------|--------|----------|
| Covariant | Covariant | Covariant |
| Contravariant | Contravariant | Contravariant |
| Covariant | Contravariant | Invariant |
| Any | Invariant | Invariant |

The `Variance::combine()` method implements these rules:

```rust,noexec
let struct_variance = field1_variance.combine(field2_variance);
```

## Examples of variance

| Type | Variance | Why |
|------|----------|-----|
| `&'a T` | Covariant | Longer refs can substitute for shorter |
| `&'a mut T` | Invariant | Mutable refs can't change lifetime |
| `fn(&'a T)` | Contravariant | Functions taking refs are contravariant |
| `fn() -> &'a T` | Covariant | Return positions are covariant |
| `Cell<&'a T>` | Invariant | Interior mutability forces invariance |
| `Box<&'a T>` | Covariant | Box is transparent, inner ref is covariant |
| `Vec<fn(&'a T)>` | Contravariant | Vec is transparent, fn is contravariant |

## Further reading

- [The Rustonomicon: Subtyping and Variance](https://doc.rust-lang.org/nomicon/subtyping.html)
- [GitHub Issue #1168](https://github.com/facet-rs/facet/issues/1168) — The original soundness discussion
