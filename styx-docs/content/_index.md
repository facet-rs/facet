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
<h2>No implicit typing</h2>
<p>Values are text until you say otherwise. No silent coercion of <code>NO</code> to <code>false</code> or <code>3.10</code> to <code>3.1</code>. Types come from your schema or your code — not from the parser guessing.</p>
</div>
<div class="feature-code">

```yaml
# YAML:
country: NO   # boolean false
version: 3.10 # 3.1
```

```styx
// Styx
country NO   // opaque scalar "NO"
version 3.10 // opaque scalar "3.10"
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Use it your way</h2>
<p>Parse into an untyped tree and walk it. Add a schema and get typed dynamic values. Or deserialize straight into native types — Rust structs, TypeScript interfaces, whatever your language offers.</p>
</div>
<div class="feature-code">

```rust
// Untyped: walk the tree
let tree = styx::parse(input)?;

// With schema: dynamic typed values
let value = styx::validate(tree, schema)?;

// Native: straight into your types
let config: MyConfig = styx::from_str(input)?;
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Schemas that fit your workflow</h2>
<p>Write schemas by hand, or generate them from your type definitions. Either way, you get validation — and you're not maintaining two sources of truth if you don't want to.</p>
</div>
<div class="feature-code">

```styx
/// A server configuration
server @object {
  host @string
  port @int
  tls @optional(@bool)
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Validation everywhere</h2>
<p>LSP brings errors and autocomplete to your editor. CLI validates in CI. Same schema, same rules, whether you're writing or shipping.</p>
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
<h2>Terse, not suffocating</h2>
<p>One-liners when you want them. Newlines when you need to breathe. No ceremony for simple things, no contortions for complex ones.</p>
</div>
<div class="feature-code">

```styx
// compact
server host=localhost port=8080 tls=true

// or breathe
server {
  host localhost
  port 8080
  tls true
}
```

</div>
</section>

<section class="feature">
<div class="feature-text">
<h2>Errors that help</h2>
<p>When something's wrong, you get the location, what was expected, and often a "did you mean?" Colors in your terminal, structure in your editor.</p>
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
