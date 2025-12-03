+++
title = "Structural Diffing"
weight = 7
insert_anchor_links = "heading"
+++

[`facet-diff`](https://docs.rs/facet-diff) computes structural differences between any two `Facet` values — even values of **different types**. It's the engine behind `facet-assert`'s rich failure output.

## Basic usage

Use the `FacetDiff` trait to compare two values:

```rust
use facet::Facet;
use facet_diff::FacetDiff;

#[derive(Facet)]
struct User {
    name: String,
    email: String,
    age: u32,
}

let before = User {
    name: "Alice".into(),
    email: "alice@example.com".into(),
    age: 30,
};

let after = User {
    name: "Alice".into(),
    email: "alice@newdomain.com".into(),  // Changed
    age: 31,                               // Changed
};

let diff = before.diff(&after);
println!("{diff}");
```

Output:

```
{
  .. 1 unchanged field
  email: "alice@example.com" → "alice@newdomain.com"
  age: 30 → 31
}
```

## The diff type

`Diff::new()` returns a `Diff` enum with these variants:

```rust
pub enum Diff<'mem, 'facet> {
    /// Values are structurally equal
    Equal { value: Option<Peek<'mem, 'facet>> },

    /// Values differ but can't be compared structurally
    Replace { from: Peek, to: Peek },

    /// Both are structs/enums with comparable fields
    User {
        from: &'static Shape,
        to: &'static Shape,
        variant: Option<&'static str>,
        value: Value,  // Field-level diffs
    },

    /// Both are sequences (Vec, arrays, slices)
    Sequence {
        from: &'static Shape,
        to: &'static Shape,
        updates: Updates,  // Element-level diffs
    },
}
```

## Checking equality

Use `is_equal()` to check if values are structurally identical:

```rust
let diff = before.diff(&after);

if diff.is_equal() {
    println!("No changes");
} else {
    println!("Changes detected:\n{diff}");
}
```

## Cross-Type comparison

One of facet-diff's unique features: compare values of **different types**:

```rust
#[derive(Facet)]
struct UserV1 {
    name: String,
    email: String,
}

#[derive(Facet)]
struct UserV2 {
    name: String,
    email: String,
    phone: Option<String>,  // New field in v2
}

let v1 = UserV1 {
    name: "Bob".into(),
    email: "bob@example.com".into(),
};

let v2 = UserV2 {
    name: "Bob".into(),
    email: "bob@example.com".into(),
    phone: Some("555-1234".into()),
};

let diff = v1.diff(&v2);
println!("{diff}");
```

Output:

```
{
  .. 2 unchanged fields
  + phone: Some("555-1234")
}
```

The diff shows that `phone` was added (it exists in `to` but not `from`).

## Sequence diffing

facet-diff uses an optimal diff algorithm for sequences:

```rust
let before = vec!["apple", "banana", "cherry"];
let after = vec!["apple", "blueberry", "cherry", "date"];

let diff = before.diff(&after);
println!("{diff}");
```

Output:

```
[
  .. 1 unchanged item
  "banana" → "blueberry"
  .. 1 unchanged item
  + "date"
]
```

The algorithm:
- Finds the longest common subsequence
- Groups consecutive insertions/deletions
- Shows inline replacements for 1-to-1 changes
- Collapses unchanged items into `.. N unchanged items`

## Nested structure diffing

Diffs recurse into nested structures:

```rust
#[derive(Facet)]
struct Team {
    name: String,
    lead: User,
    members: Vec<User>,
}

let before = Team {
    name: "Engineering".into(),
    lead: User { name: "Alice".into(), email: "alice@example.com".into(), age: 30 },
    members: vec![
        User { name: "Bob".into(), email: "bob@example.com".into(), age: 25 },
    ],
};

let after = Team {
    name: "Engineering".into(),
    lead: User { name: "Alice".into(), email: "alice@example.com".into(), age: 31 },  // Age changed
    members: vec![
        User { name: "Bob".into(), email: "bob@example.com".into(), age: 25 },
        User { name: "Carol".into(), email: "carol@example.com".into(), age: 28 },  // Added
    ],
};

let diff = before.diff(&after);
println!("{diff}");
```

Output:

```
{
  .. 1 unchanged field
  lead: {
    .. 2 unchanged fields
    age: 30 → 31
  }
  members: [
    .. 1 unchanged item
    + User { name: "Carol", email: "carol@example.com", age: 28 }
  ]
}
```

## Enum diffing

Enum variants are compared structurally:

```rust
#[derive(Facet)]
#[repr(u8)]
enum Status {
    Pending,
    Active { since: String },
    Completed { at: String, result: i32 },
}

let before = Status::Active { since: "2024-01-01".into() };
let after = Status::Active { since: "2024-06-01".into() };

let diff = before.diff(&after);
println!("{diff}");
```

Output:

```
Active {
  since: "2024-01-01" → "2024-06-01"
}
```

If variants differ entirely, it's shown as a replacement:

```rust
let before = Status::Pending;
let after = Status::Active { since: "2024-01-01".into() };

let diff = before.diff(&after);
println!("{diff}");
```

Output:

```
Status::Pending → Status::Active { since: "2024-01-01" }
```

## Dynamic value comparison

facet-diff can compare `facet_value::Value` against typed structs:

```rust
use facet_value::Value;

#[derive(Facet)]
struct Config {
    host: String,
    port: u16,
}

let typed = Config {
    host: "localhost".into(),
    port: 8080,
};

let dynamic: Value = facet_json::from_str(r#"{"host": "localhost", "port": 9090}"#)?;

let diff = typed.diff(&dynamic);
println!("{diff}");
```

Output:

```
{
  .. 1 unchanged field
  port: 8080 → 9090
}
```

This is powerful for testing: compare expected typed values against parsed JSON/YAML without manual conversion.

## Option diffing

Options are compared by their inner values:

```rust
let before: Option<u32> = Some(42);
let after: Option<u32> = Some(100);

let diff = before.diff(&after);
println!("{diff}");
```

Output:

```
Some 42 → 100
```

When one is `Some` and the other is `None`:

```rust
let before: Option<u32> = Some(42);
let after: Option<u32> = None;

let diff = before.diff(&after);
println!("{diff}");
```

Output:

```
Some(42) → None
```

## Display formatting

The `Diff` type implements `Display` with colorized output using the Tokyo Night color scheme:

| Element | Color |
|---------|-------|
| Deletions (`-`) | Red |
| Insertions (`+`) | Green |
| Field names | Teal |
| Unchanged indicators | Gray/muted |
| Punctuation | Dimmed |

Colors are automatically applied when printing to a terminal that supports ANSI codes.

## Integration with facet-assert

`facet-assert` uses facet-diff internally. When `assert_same!` fails, you see the diff:

```rust
use facet_assert::assert_same;

let expected = User { name: "Alice".into(), email: "alice@example.com".into(), age: 30 };
let actual = User { name: "Alice".into(), email: "alice@other.com".into(), age: 30 };

assert_same!(expected, actual);
```

Failure output:

```
assertion `assert_same!(left, right)` failed

{
  .. 2 unchanged fields
  email: "alice@example.com" → "alice@other.com"
}
```

## Pointer dereferencing

Smart pointers (`Box`, `Rc`, `Arc`) and references are automatically dereferenced:

```rust
use std::rc::Rc;

let before = Rc::new(User { name: "Alice".into(), email: "a@example.com".into(), age: 30 });
let after = Rc::new(User { name: "Alice".into(), email: "b@example.com".into(), age: 30 });

let diff = before.diff(&after);
// Compares the inner User values, not the Rc pointers
```

## Closeness score

Internally, diffs compute a "closeness" score used to find optimal alignments in sequences. You generally don't need this directly, but it's how facet-diff decides whether two sequence elements are "the same element, modified" vs "one deleted, one inserted":

- Higher closeness = more fields/elements in common
- Used by the sequence diff algorithm to minimize noise
- Prefers showing field changes over delete+insert pairs

## Next steps

- See [Assertions](@/guide/showcases/assert.md) for `assert_same!` usage
- Check [Pretty Printing](@/guide/showcases/pretty.md) for value display
- Read about [Dynamic Values](@/guide/dynamic-values.md) for cross-type comparison with `Value`
