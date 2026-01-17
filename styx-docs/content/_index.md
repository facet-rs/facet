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

<style>
.layers-section {
  background: #dc143c;
  margin: 0rem 0;
  margin-left: calc(-50vw + 50%);
  margin-right: calc(-50vw + 50%);
  padding: 4rem 2rem;
}

.layers-section .section-header {
  text-align: center;
  margin-bottom: 2.5rem;
  max-width: 600px;
  margin-left: auto;
  margin-right: auto;
}

.layers-section .section-title {
  font-family: var(--font-heading);
  font-size: 2.5rem;
  font-weight: 400;
  letter-spacing: -0.02em;
  text-transform: uppercase;
  color: #fff;
  margin: 0 0 0.1rem 0;
  line-height: 1.1;
}

.layers-section .section-subtitle {
  font-family: "Lato", sans-serif;
  font-size: 1.75rem;
  font-weight: 200;
  color: rgba(255,255,255,0.95);
  margin: 0 0 1.25rem 0;
}

.layers-section .section-desc {
  font-size: 1rem;
  line-height: 1.6;
  color: rgba(255,255,255,0.8);
  margin: 0;
}

.layers-section .section-desc em {
  color: #fff;
  font-style: normal;
  font-weight: 700;
}

.hero-intro h1 {
  font-family: var(--font-heading);
  font-weight: 400;
  text-transform: uppercase;
}

.feature-text h2 {
  font-family: var(--font-heading);
  font-weight: 400;
  text-transform: uppercase;
}

.layers-diagram {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0;
  max-width: 650px;
  margin: 0 auto;
}

.layer-box {
  background: var(--bg-code);
  border: 1px solid var(--border);
  border-radius: 8px;
  max-width: 400px;
  width: 100%;
  overflow: hidden;
}

.layer-box .layer-title {
  display: block;
  font-size: 0.7rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-muted);
  padding: 0.6rem 1rem;
  border-bottom: 1px solid var(--border);
  background: var(--bg-subtle);
}

.layer-box pre {
  margin: 0;
  padding: 0.75rem 1rem;
  background: none;
  border: none;
  border-radius: 0;
}

.layer-box code {
  font-family: var(--font-mono);
  font-size: 0.85rem;
  background: none;
  border: none;
  padding: 0;
  white-space: pre;
}

.layer-arrow {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 0.4rem 0;
}

.layer-arrow::before {
  content: "";
  width: 3px;
  height: 18px;
  background: rgba(255,255,255,0.8);
}

.layer-arrow::after {
  content: "";
  width: 0;
  height: 0;
  border-left: 8px solid transparent;
  border-right: 8px solid transparent;
  border-top: 10px solid rgba(255,255,255,0.8);
}

</style>

<div class="layers-section">
<div class="section-header">
<p class="section-title">Get those types out of your document</p>
<p class="section-subtitle">and into your schema.</p>
<p class="section-desc">Styx schemas aren't just for objects and arrays — they're for <em>every scalar</em>.</p>
</div>

<div class="layers-diagram">
  <div class="layer-box">
    <span class="layer-title">Styx source — opaque scalars</span>

```styx
host localhost
port 8080
```

</div>

  <div class="layer-arrow"></div>

  <div class="layer-box">
    <span class="layer-title">Schema — types & constraints</span>

```styx
host @string
port @int
```

</div>

  <div class="layer-arrow"></div>

  <div class="layer-box">
    <span class="layer-title">Rust struct, JS object, etc.</span>

```rust
Server {
    host: "localhost",
    port: 8080,
}
```

</div>
</div>
</div>

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

Parse Styx in the browser or Node.js. With a schema, you get a plain JavaScript object with real types — `number`, `string`, `Date`, not just opaque scalars.

</div>
<div class="feature-code">

```typescript
import { parse } from "@bearcove/styx";

const config = parse(input, schema);

// config.server.port is a number
// config.server.host is a string
// config.createdAt is a Date
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
