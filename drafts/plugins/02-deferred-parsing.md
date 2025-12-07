# Deferred Parsing

## Core Insight

The trick is: **don't parse the struct immediately**. Instead:

1. Pass the raw token stream through plugin invocations
2. Each plugin declares what it wants (via a "script")
3. Only at the final step do we parse the struct once
4. Generate all code (facet impl + plugin outputs) together

This means parsing happens exactly once, no matter how many plugins.

## The Flow

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
│     @remaining { }  // more plugins to invoke           │
│     @plugins { }    // accumulator for scripts          │
│ );                                                      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
Step 2: facet_error's __facet_invoke! adds its script and forwards:
┌─────────────────────────────────────────────────────────┐
│ ::facet::__facet_finalize!(                             │
│     @tokens { /* same raw token stream */ }             │
│     @plugins {                                          │
│         error => { /* script tokens */ }                │
│     }                                                   │
│ );                                                      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
Step 3: facet's __facet_finalize! parses tokens + evaluates scripts:
┌─────────────────────────────────────────────────────────┐
│ // 1. Parse struct/enum into PStruct/PEnum (once!)      │
│ // 2. Evaluate each plugin's script                     │
│ // 3. Emit all generated code:                          │
│                                                         │
│ impl Facet for DataStoreError { ... }                   │
│                                                         │
│ // From error plugin's script:                          │
│ impl std::fmt::Display for DataStoreError { ... }       │
│ impl std::error::Error for DataStoreError { ... }       │
│ impl From<std::io::Error> for DataStoreError { ... }    │
└─────────────────────────────────────────────────────────┘
```

## Multiple Plugins

With multiple plugins, they chain:

```rust
#[derive(Facet)]
#[facet(error, builder)]
struct Foo { ... }
```

Expands to:

```rust
::facet_error::__facet_invoke!(
    @tokens { ... }
    @remaining { ::facet_builder::__facet_invoke }
    @plugins { }
);
```

Each plugin adds its script and forwards to the next, until `__facet_finalize!`
receives all scripts and does the single parse + codegen.

## Design Considerations

1. **Single parse**: `__facet_finalize!` parses the struct exactly once
2. **No load order issues**: Expansion is deterministic (follows macro call graph)
3. **Plugins are scripts**: They emit code generation instructions
4. **No runtime overhead**: All codegen happens at compile time
