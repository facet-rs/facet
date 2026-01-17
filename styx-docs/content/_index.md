+++
title = "Styx"
insert_anchor_links = "heading"
+++

<div class="hero-intro">
<h1>Styx</h1>
<p class="tagline">A document language for mortals.</p>
</div>

```styx
styx is
a (fun document language) // with comments

/// and doc comments
supporting {
    key-value pairs
    untyped scalars // (until deserialization)
}

you-may @tag(any thing you want) // good for enums
quote "anything you want"
raw-quote r#"to "get meta" if you wish"#

and-if-needed <<HEREDOCS,bash
export RUST_LOG=trace
@echo "are here to save the day"
HEREDOCS
```

<div class="features">

<section class="feature">
<div class="feature-text">

## No implicit typing

Objects and sequences contain scalar key-values. Scalars are just opaque text at
this stage.

</div>
<div class="feature-code">

```yaml
# YAML:
country: NO   # boolean false
version: 3.10 # 3.1
comment: "This is a string for sure"
```

<div style="height: 1em;"></div>

```styx
// Styx
country NO   // opaque scalar "NO"
version 3.10 // opaque scalar "3.10"
comment "This is a string for sure" // nope, an opaque scalar
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Deserialize to Rust structs

Derive `Facet` on your types and deserialize directly. No schema files, no code generation — your types are the schema.

</div>
<div class="feature-code">

```rust
#[derive(Facet)]
struct Server {
    host: String,
    port: u16,
    tls: Option<bool>,
}

let server: Server = facet_styx::from_str(input)?;
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Parse and explore dynamically

Parse into an untyped tree and walk it. Get values by path, check types at runtime, transform as needed.

</div>
<div class="feature-code">

```rust
let doc = styx_tree::Document::parse(input)?;

let name = doc.get("server.host")
    .and_then(|v| v.as_str());

for entry in doc.root().as_object().unwrap() {
    println!("{}: {:?}", entry.key, entry.value);
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Validate with schemas

Write schemas by hand or generate them from types. Validate documents and get rich error messages with source locations.

</div>
<div class="feature-code">

```rust
let schema: SchemaFile = facet_styx::from_str(schema_src)?;
let doc = styx_tree::parse(input)?;

let result = styx_schema::validate(&doc, &schema);
if !result.is_valid() {
    result.write_report("config.styx", input, stderr());
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## JavaScript and beyond

Parse Styx in the browser or Node.js. Get a typed tree you can walk, transform, or validate against a schema.

</div>
<div class="feature-code">

```typescript
import { parse } from "@bearcove/styx";

const doc = parse(`server { host localhost port 8080 }`);

for (const entry of doc.entries) {
    if (entry.value.payload?.type === "object") {
        // walk the tree
    }
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## First-class schema support

Write schemas by hand for external contracts, or generate them from your Rust types. Either way works.

Doc comments become hover text in your editor and show up in error messages. Default values, constraints, deprecation warnings — it's all there.

</div>
<div class="feature-code">

```styx
/// A server configuration.
/// Used by the load balancer to route traffic.
Server @object {
  /// Hostname or IP address to bind to.
  host @default(localhost @string)

  /// Port number (1-65535).
  port @default(8080 @int{ min 1, max 65535 })

  /// Enable TLS. Defaults to false.
  tls @default(false @bool)

  /// Allowed origins for CORS.
  origins @seq(@string)
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Validation everywhere

LSP brings errors and autocomplete to your editor. CLI validates in CI. Same schema, same rules, whether you're writing or shipping.

</div>
<div class="feature-code">

```bash
# In CI
styx config.styx --validate

# In your editor
# → errors inline, autocomplete, hover docs
```

</div>
</section>

<section class="feature">
<div class="feature-text">

## Best-of-class errors

When something's wrong, you get the location, what was expected, and often a "did you mean?" Colors in your terminal, structure in your editor.

</div>
<div class="feature-code">

```
error: unknown field `hots`
  ┌─ config.styx:2:3
  │
2 │   hots localhost
  │   ^^^^ did you mean `host`?
```

</div>
</section>

</div>

<div class="hero-links">

[Learn Styx](/learn/primer) — a 5-minute primer

[Reference](/reference) — the spec

</div>
