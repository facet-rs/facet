# Facet Plugin System Design

## Background

### Problem: Extensibility without Double-Parsing

Facet wants to support extensions like `facet-error` (thiserror replacement), `facet-builder`,
`facet-partial`, etc. The challenge: how do these extensions generate code without each one
parsing the struct independently?

### Failed Approach: Shared Dylib Registry (PR #1143)

The idea was to have proc-macros share state via a common dylib dependency. This failed because:

> "There is no guarantee on proc-macro load order. Rust-analyzer loads them in whatever order,
> while rustc is currently working on parallelization of macro expansion."
> — @bjorn3

### Promising Approach: Macro Expansion Handshake

[proc-macro-handshake](https://github.com/alexkirsz/proc-macro-handshake) demonstrates a
different pattern: use macro expansion itself as the communication channel.

The key insight: instead of sharing runtime state, we chain macro expansions:

```
register!(::plugin)
    → plugin::instantiate!(::plugin, ::registry)
        → registry::register_plugin!(::plugin, "data")
            → stores in static REGISTRY
```

This is deterministic because expansion order follows the call graph.

## Proposed Design for Facet

### Core Insight: Defer Parsing

The trick is: **don't parse the struct immediately**. Instead:

1. Pass the raw token stream through plugin invocations
2. Each plugin declares what it wants (via a "script")
3. Only at the final step do we parse the struct once
4. Generate all code (facet impl + plugin outputs) together

This means parsing happens exactly once, no matter how many plugins.

### The Flow

```
User code:
┌─────────────────────────────────────────────────────────┐
│ #[derive(Facet)]                                        │
│ #[facet(error)]  // enables facet-error plugin          │
│ pub enum DataStoreError {                               │
│     /// data store disconnected                         │
│     #[facet(from)]                                      │
│     Disconnect(std::io::Error),                         │
│                                                         │
│     /// the data for key `{0}` is not available         │
│     Redaction(String),                                  │
│ }                                                       │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
Step 1: Facet's derive does NOT parse. It just expands to:
┌─────────────────────────────────────────────────────────┐
│ ::facet_error::__facet_invoke!(                         │
│     @tokens { /* raw token stream of the enum */ }      │
│     @next {                                             │
│         ::facet::__facet_finalize!                      │
│     }                                                   │
│     @plugins { }  // accumulator, starts empty          │
│ );                                                      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
Step 2: facet_error's __facet_invoke! adds its template and forwards:
┌─────────────────────────────────────────────────────────┐
│ ::facet::__facet_finalize!(                             │
│     @tokens { /* same raw token stream */ }             │
│     @plugins {                                          │
│         r#"                                             │
│         impl Display for {{ Self }} { ... }             │
│         impl Error for {{ Self }} { ... }               │
│         {% for v in variants %}                         │
│           {% if v.fields[0].has_attr("from") %}         │
│             impl From<{{ ... }}> for {{ Self }} { ... } │
│           {% endif %}                                   │
│         {% endfor %}                                    │
│         "#                                              │
│     }                                                   │
│ );                                                      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
Step 3: facet's __facet_finalize! parses tokens + evaluates templates:
┌─────────────────────────────────────────────────────────┐
│ // 1. Parse struct/enum into PStruct/PEnum (once!)      │
│ // 2. Evaluate each plugin's Jinja template             │
│ // 3. Emit all generated code:                          │
│                                                         │
│ impl Facet for DataStoreError { ... }                   │
│                                                         │
│ // From error plugin's template:                        │
│ impl std::fmt::Display for DataStoreError { ... }       │
│ impl std::error::Error for DataStoreError { ... }       │
│ impl From<std::io::Error> for DataStoreError { ... }    │
└─────────────────────────────────────────────────────────┘
```

### Multiple Plugins

With multiple plugins, they chain:

```
#[derive(Facet)]
#[facet(error, builder)]
struct Foo { ... }
```

Expands to:

```
::facet_error::__facet_invoke!(
    @tokens { ... }
    @next {
        ::facet_builder::__facet_invoke!(
            @next { ::facet::__facet_finalize! }
        )
    }
    @plugins { }
);
```

Each plugin adds its script and forwards to the next, until `__facet_finalize!`
receives all scripts and does the single parse + codegen.

### The "Scripting Language"

The script must be **imperative enough** that facet-core doesn't need to know about
specific plugins. It's instructions for code generation, not declarations.

Let's work backwards from concrete examples:

---

#### Example 1: `facet-display` (displaydoc replacement)

User writes:
```rust
#[derive(Facet)]
#[facet(display)]
pub enum MyError {
    /// Failed to connect to {host}:{port}
    Connection { host: String, port: u16 },

    /// File not found: {0}
    NotFound(PathBuf),
}
```

What we need to generate:
```rust
impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection { host, port } =>
                write!(f, "Failed to connect to {}:{}", host, port),
            Self::NotFound(v0) =>
                write!(f, "File not found: {}", v0),
        }
    }
}
```

So the script needs to express:
- "impl Display for this type"
- "match on variants"
- "use doc comment as format string, interpolating fields"

---

#### Example 2: `facet-error` (thiserror replacement)

User writes:
```rust
#[derive(Facet)]
#[facet(error)]  // implies display
pub enum DataStoreError {
    /// data store disconnected
    #[facet(from)]
    Disconnect(std::io::Error),

    /// invalid header (expected {expected:?}, found {found:?})
    InvalidHeader { expected: String, found: String },

    /// unknown error
    #[facet(source)]
    Unknown { source: Box<dyn std::error::Error> },
}
```

What we need to generate:
```rust
// Display impl (same as facet-display)
impl std::fmt::Display for DataStoreError { ... }

// Error impl
impl std::error::Error for DataStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            // #[facet(from)] fields are implicitly sources
            Self::Disconnect(e) => Some(e),
            // #[facet(source)] explicitly marks source
            Self::Unknown { source } => Some(source.as_ref()),
            _ => None,
        }
    }
}

// From impl for #[facet(from)] fields
impl From<std::io::Error> for DataStoreError {
    fn from(source: std::io::Error) -> Self {
        Self::Disconnect(source)
    }
}
```

So the script needs to express:
- Everything from facet-display, plus:
- "impl Error for this type"
- "for source(), find fields with #[facet(from)] or #[facet(source)]"
- "for each #[facet(from)] field, generate a From impl"

---

#### What primitives does the script language need?

Looking at these examples, the script needs to:

1. **Declare trait impls**: `impl Trait for Self { ... }`
2. **Match on structure**: `match self { variants... }` or `self.field`
3. **Access metadata**: doc comments, attributes, field names, field types
4. **Conditionals**: "if field has attr X" or "if variant is tuple vs struct"
5. **Iterate**: "for each field", "for each variant"
6. **String interpolation**: parse `{field}` in doc comments
7. **Emit code fragments**: the actual `write!(...)` calls, etc.

This is starting to look like a real templating language. Using **Jinja-like syntax**:

- `{{ expr }}` — interpolate a value
- `{% for x in y %}...{% endfor %}` — iteration
- `{% if cond %}...{% endif %}` — conditionals

#### facet-display template

```jinja
impl std::fmt::Display for {{ Self }} {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            {% for v in variants %}
            Self::{{ v.name }}{{ v.pattern }} => {
                write!(f, {{ v.doc | format_string }})
            }
            {% endfor %}
        }
    }
}
```

#### facet-error template

```jinja
{# Display impl - could include facet-display or inline it #}
impl std::fmt::Display for {{ Self }} {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            {% for v in variants %}
            Self::{{ v.name }}{{ v.pattern }} => {
                write!(f, {{ v.doc | format_string }})
            }
            {% endfor %}
        }
    }
}

{# Error impl #}
impl std::error::Error for {{ Self }} {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            {% for v in variants %}
            {% for f in v.fields %}
            {% if f.has_attr("from") or f.has_attr("source") %}
            Self::{{ v.name }} { {{ f.name }}: ref e, .. } => Some(e),
            {% endif %}
            {% endfor %}
            {% endfor %}
            _ => None,
        }
    }
}

{# From impls for #[facet(from)] fields #}
{% for v in variants %}
{% for f in v.fields %}
{% if f.has_attr("from") %}
impl From<{{ f.ty }}> for {{ Self }} {
    fn from(source: {{ f.ty }}) -> Self {
        Self::{{ v.name }}(source)
    }
}
{% endif %}
{% endfor %}
{% endfor %}
```

#### Available template variables

- `{{ Self }}` — the type name (with generics)
- `{{ self }}` — the type name (without generics, for patterns)
- `variants` — list of enum variants (empty for structs)
- `fields` — list of struct fields (for structs)
- `v.name` — variant name
- `v.doc` — variant doc comment
- `v.pattern` — destructuring pattern like `{ field1, field2 }` or `(v0, v1)`
- `v.fields` — fields of this variant
- `f.name` — field name (or index for tuple)
- `f.ty` — field type
- `f.doc` — field doc comment
- `f.has_attr("x")` — check for `#[facet(x)]` attribute
- `f.attr("x")` — get value of `#[facet(x = value)]`

#### Filters

- `{{ v.doc | format_string }}` — converts doc comment with `{field}` placeholders
  into a format string + args: `"msg: {}", field`

### Design Considerations

1. **Single parse**: `__facet_finalize!` parses the struct exactly once
2. **No load order issues**: Expansion is deterministic (follows macro call graph)
3. **Plugins are templates**: They emit code templates with holes for metadata
4. **No runtime overhead**: All codegen happens at compile time

### Failure Modes and Risks

The handshake pattern has inherent fragility because there's no central coordinator—just
a chain of trust between plugins. If any plugin in the chain breaks, everything fails.

#### 1. Plugin Bug Breaks the Chain

If `facet_error::__facet_invoke!` has a bug and emits malformed output, the next plugin
in the chain never gets invoked:

```
derive(Facet) → error_invoke!(corrupted) → ??? (debug_invoke never called)
```

The user sees a cryptic error about unexpected tokens, pointing at the derive macro
rather than the buggy plugin.

#### 2. Protocol Version Mismatch

If the internal protocol changes (e.g., `@remaining` renamed to `@next_plugins`), older
plugins will produce output that newer plugins can't parse:

```rust
// Old plugin emits:
next_plugin! { @tokens {...} @next {...} @plugins {...} }

// New plugin expects:
next_plugin! { @tokens {...} @remaining {...} @plugins {...} }
```

This is especially problematic for third-party plugins that may not update in lockstep
with facet-core.

#### 3. Poor Error Attribution

When something goes wrong, the compiler error points at the `#[derive(Facet)]` line,
not at the plugin that caused the problem. Users have no way to know which plugin
in the chain failed.

#### Mitigations

1. **Protocol versioning**: Include a version marker in the invocation format:
   ```rust
   plugin_invoke! { @version { 1 } @tokens {...} ... }
   ```
   Plugins can detect version mismatches and emit clear errors.

2. **Validation in finalize**: `__facet_finalize!` should validate its input and emit
   helpful diagnostics if something looks wrong (e.g., "expected @plugins section").

3. **Plugin testing**: Each plugin should have integration tests that verify correct
   forwarding through the chain.

4. **Diagnostic breadcrumbs**: Each plugin could append to a `@trace` section:
   ```rust
   @trace { "facet-error v0.1.0", "facet-display v0.2.0" }
   ```
   On error, this trace could help identify which plugin was last to touch the chain.

These mitigations reduce but don't eliminate the risk. The fundamental tradeoff is:
deterministic expansion order (handshake) vs. robustness to individual plugin failures
(shared state, which has its own problems with load order).

### Optimization: Template AST Caching

Since templates are static strings, `facet-macros` can cache the parsed template AST:

```rust
// In facet-macros
static TEMPLATE_CACHE: Mutex<HashMap<u64, Arc<TemplateAst>>> = ...;

fn get_or_parse_template(template_str: &str) -> Arc<TemplateAst> {
    let hash = hash(template_str);
    let mut cache = TEMPLATE_CACHE.lock().unwrap();
    cache.entry(hash).or_insert_with(|| {
        Arc::new(parse_jinja_template(template_str))
    }).clone()
}
```

This means:
- First invocation of a plugin: parse template, cache AST
- Subsequent invocations: reuse cached AST, only evaluate with new struct data
- Across a crate with 100 error types using `#[facet(error)]`: template parsed once

### Better Error Messages via `#[diagnostic::on_unimplemented]`

When a user writes `#[facet(error)]` but hasn't added `facet-error` to their dependencies,
we want a helpful error message, not rustc's generic "cannot find macro" error.

We can achieve this using scope shadowing and the `on_unimplemented` diagnostic attribute
([reference](https://doc.rust-lang.org/reference/attributes/diagnostics.html#r-attributes.diagnostic.on_unimplemented)).

The trick: instead of directly invoking `::facet_error::__facet_invoke!`, we generate
validation code that checks for a marker trait the plugin must export:

```rust
// In facet-core (always available)
#[diagnostic::on_unimplemented(
    message = "Plugin `{Self}` not found",
    label = "add `{Self}` to your Cargo.toml dependencies",
    note = "facet plugins must be explicitly added as dependencies"
)]
pub trait FacetPlugin {
    // Marker type that plugins export
    type Marker;
}

// Each plugin exports this:
// pub struct PluginMarker;
// impl facet::FacetPlugin for facet_error::PluginMarker {
//     type Marker = ();
// }
```

Then `#[derive(Facet)]` generates:

```rust
{
    // This block validates the plugin exists before invoking it
    struct ErrorPluginCheck;

    // If facet-error is in scope, this import succeeds and shadows our struct
    #[allow(unused_imports)]
    use ::facet_error::PluginMarker as ErrorPluginCheck;

    // This trait bound fails with our custom message if the plugin isn't found
    const _: () = {
        fn check<T: ::facet::FacetPlugin>() {}
        check::<ErrorPluginCheck>();
    };
}

// Now safe to invoke the plugin
::facet_error::__facet_invoke! { ... }
```

If `facet-error` isn't in dependencies, the user sees:

```
error[E0277]: Plugin `ErrorPluginCheck` not found
  --> src/main.rs:3:10
   |
3  | #[derive(Facet)]
   |          ^^^^^ add `facet-error` to your Cargo.toml dependencies
   |
   = note: facet plugins must be explicitly added as dependencies
```

Much better than "cannot find crate `facet_error`"!

### Open Questions

1. **How are templates stored/transmitted?**

   Options:
   - String literal in the plugin's `__facet_invoke!` expansion
   - Separate `.facet` template files (loaded at compile time?)
   - Embedded in the proc-macro crate somehow

2. **Template parsing: when and how?**

   The template needs to be parsed and evaluated. Options:
   - Parse at compile time in `__facet_finalize!` (needs a Jinja parser in Rust)
   - Use an existing Rust templating crate (tera, minijinja, askama?)
   - Build a minimal custom parser (just the subset we need)

3. **How do plugins compose?**

   If `facet-error` implies `facet-display`, how is that expressed?
   - Template inclusion: `{% include "facet-display" %}`
   - Plugin chaining: error's `__facet_invoke!` also invokes display
   - Explicit in user code: `#[facet(display, error)]`

4. **Error messages**

   If a template refers to something invalid (wrong field name, missing attr),
   where does the error point? Can we get good spans?

5. **Escaping and edge cases**

   - What if a doc comment contains `{{` literally?
   - What about generics with complex bounds?
   - What about `where` clauses?

6. **Template validation**

   Can we validate templates at plugin compile time, before they're used?
   (e.g., check that `v.nonexistent` would fail early)

## References

- [PR #1143: Failed dylib approach](https://github.com/facet-rs/facet/pull/1143)
- [proc-macro-handshake](https://github.com/alexkirsz/proc-macro-handshake)
- [Issue #1139: facet-error vision](https://github.com/facet-rs/facet/issues/1139)
