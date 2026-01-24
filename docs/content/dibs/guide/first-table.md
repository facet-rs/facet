+++
title = "Your first table"
description = "Define tables as Rust structs"
weight = 3
+++

In dibs, tables are regular Rust structs annotated with facet attributes.

## Example: A users table

Add this to `crates/my-app-db/src/lib.rs`:

```rust
use facet::Facet;

#[derive(Facet)]
#[facet(dibs::table = "users")]
pub struct User {
    #[facet(dibs::pk)]
    pub id: i64,

    #[facet(dibs::unique)]
    pub email: String,

    #[facet(dibs::column = "display_name")]
    pub name: String,
}
```

This creates a `users` table with:
- An `id` column as the primary key
- A unique `email` column
- A `name` field that maps to a `display_name` column in Postgres

## Verify the schema

```bash
dibs schema
```

You should see your `users` table in the output.

## Attributes

### Schema (affects database structure)

**`dibs::table = "name"`** (table-level)
Marks a struct as a database table.

**`dibs::pk`**
Marks this column as the primary key.

**`dibs::unique`**
Adds a unique constraint to this column.

**`dibs::fk = "table.column"`**
Creates a foreign key reference to another table's column.

**`dibs::not_null`**
Explicit NOT NULL constraint (usually inferred from non-`Option<T>` types).

**`dibs::default = "expr"`**
Sets a default value expression (e.g., `"now()"`, `"true"`, `"'draft'"`).

**`dibs::column = "name"`**
Overrides the column name in the database (if different from the struct field).

**`dibs::index`**
Creates an index on this column.

**`dibs::auto`**
Marks the column as auto-increment / generated.

### Admin UI (affects TUI and tooling)

**`dibs::icon = "name"`** (table or column level)
Sets an icon for display in the TUI and other tooling.

**`dibs::label`**
Marks this column as the display label for records (shown in lists, references, etc.).

**`dibs::subtype = "type"`**
Specifies a semantic subtype for better UI rendering (e.g., `"slug"`, `"sku"`, `"money"`).

**`dibs::long`**
Indicates this is a long text field (rendered as a textarea instead of input).

**`dibs::lang = "language"`**
Specifies the content language/format for syntax highlighting (e.g., `"markdown"`).

---

If you're unsure what dibs thinks your schema is, run `dibs schema --plain` or `dibs schema --sql`.
