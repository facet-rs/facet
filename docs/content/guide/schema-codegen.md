+++
title = "Schema & code generation"
weight = 11
insert_anchor_links = "heading"
+++

Your Rust types are already a schema. These crates project that schema into
other type systems so a frontend, an API contract, or another language stays in
sync with one source of truth.

## Setup

```bash
cargo add facet
cargo add facet-typescript facet-json-schema
```

`facet-zod` isn't on crates.io yet — until it lands, depend on it by git:

```toml
facet-zod = { git = "https://github.com/facet-rs/facet" }
```

Each is a single call over a `Facet` type.

## TypeScript interfaces

[`facet-typescript`](https://docs.rs/facet-typescript) emits plain TS
interfaces:

```rust,noexec
use facet::Facet;

#[derive(Facet)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

let ts = facet_typescript::to_typescript::<User>();
```

```typescript
export interface User {
  name: string;
  age: number;
  email?: string;
}
```

For a bundle of related types, build them up and emit once:

```rust,noexec
use facet_typescript::TypeScriptGenerator;

let mut gen = TypeScriptGenerator::new();
gen.add_type::<User>();
gen.add_type::<Post>();
let code = gen.finish();
```

## Zod schemas

[`facet-zod`](https://github.com/facet-rs/facet/tree/main/facet-zod) emits
[Zod](https://zod.dev) schemas — runtime validation *and* inferred TS types in
one:

```rust,noexec
let schema = facet_zod::generate::<User>();
```

```typescript
export const UserSchema = z.object({
  name: z.string(),
  age: z.number(),
  email: z.string().optional(),
});
```

`facet_zod::generate_with_config::<User>(config)` adjusts output style (e.g.
exported consts vs. a namespace).

## JSON Schema

[`facet-json-schema`](https://docs.rs/facet-json-schema) emits Draft-07 JSON
Schema, for OpenAPI, validators, or editor tooling:

```rust,noexec
let schema = facet_json_schema::to_schema::<User>();
```

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "age":  { "type": "integer" },
    "email": { "type": "string" }
  },
  "required": ["name", "age"]
}
```

## A full-stack workflow

Put one of these calls in a small generator binary (or a `build.rs`/test that
writes the output and fails if it drifts), run it in CI, and your frontend types
can never silently diverge from your Rust types. `facet-python` does the same
for Python projects.

## Related

- [JSON](@/guide/json.md) — serialize the very types you generated schemas for
- [Ecosystem](@/ecosystem/_index.md) — the full schema & codegen group
