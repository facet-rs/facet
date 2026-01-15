+++
title = "Tracing"
weight = 6
insert_anchor_links = "heading"
+++

## How It Works

Facet uses [tracing](https://docs.rs/tracing) as an optional dependency with crate-level forwarding macros that compile to nothing when the feature is disabled. We avoid `#[instrument]` because it pulls in `syn`.

### The Forwarding Macros

Each crate that uses tracing defines forwarding macros like this:

```rust
// src/tracing_macros.rs

/// Emit a trace-level log message.
#[cfg(any(test, feature = "tracing"))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        tracing::trace!($($arg)*);
    };
}

/// Emit a trace-level log message (no-op version).
#[cfg(not(any(test, feature = "tracing")))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {};
}
```

**Why forwarding macros?** The `tracing` crate itself already compiles to zero runtime cost when no subscriber is registered. The reason we use forwarding macros is to **avoid pulling in the `tracing` dependency at all** when the feature is disabled. This keeps the dependency tree small and compile times fast for users who don't need tracing.

This pattern:
- Avoids the `tracing` dependency entirely when the feature is disabled
- Automatically enables tracing in tests via `cfg(test)`
- Forwards to the real `tracing::trace!` when enabled
- The no-op version expands to nothing (tracing's special syntax like `%` and `?` can't be consumed by `format_args!()`)

You can add macros for other levels (`debug!`, `info!`, `warn!`, `error!`) following the same pattern.

### Cargo.toml Setup

Here's the pattern used in facet crates (example from `facet-xml`):

```toml
[dependencies]
# Tracing (optional - compiles to nothing when disabled)
tracing = { workspace = true, optional = true }

[dev-dependencies]
# Required for tests - makes tracing macros resolve
tracing = { workspace = true }

# Enables tracing in dependencies during tests
facet-dom = { path = "../facet-dom", features = ["tracing"] }
facet-reflect = { path = "../facet-reflect", features = ["tracing"] }

# Test helpers set up the tracing subscriber
facet-testhelpers = { path = "../facet-testhelpers" }

[features]
# Propagate tracing to dependencies
tracing = ["dep:tracing", "facet-dom/tracing", "facet-reflect/tracing"]
```

Key points:
1. **Optional dependency** in `[dependencies]` — production builds don't pay for tracing
2. **Non-optional dev-dependency** — tests always have access to `tracing` macros
3. **Feature propagation** — the `tracing` feature enables tracing in all dependencies
4. **facet-testhelpers** — sets up the tracing subscriber automatically

### Using facet-testhelpers

All tests should use `facet_testhelpers::test` instead of the standard `#[test]` attribute:

```rust
use facet_testhelpers::test;

#[test]
fn my_test() {
    // Tracing subscriber is automatically set up
    // Default is trace level — use FACET_LOG to filter if too verbose
}
```

For tests using `libtest-mimic` or `datatest`, call `facet_testhelpers::setup()` directly:

```rust
fn main() {
    facet_testhelpers::setup();
    // ... run your custom test harness
}
```

### Using Tracing in Code

```rust
use crate::trace; // Use the crate-local forwarding macro

fn process_field(field: &Field, value: &Value) -> Result<(), Error> {
    trace!(field.name, ?value, "processing field");
    
    let result = do_work(value)?;
    trace!(?result, "field processed successfully");
    
    Ok(result)
}
```

## Filtering Output

The default is `trace` level (very verbose). Use `FACET_LOG` to filter:

```bash
# Only facet_format at debug level
FACET_LOG=facet_format=debug cargo nextest run -p facet-json

# Multiple targets
FACET_LOG=facet_format=trace,facet_reflect=debug cargo nextest run
```

See [Targets](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/targets/struct.Targets.html) for the full syntax.
