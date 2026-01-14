+++
title = "Tracing"
weight = 6
insert_anchor_links = "heading"
+++

## Philosophy

We're building software for the next 20 years. The test that's failing today will fail again someday — maybe in 6 months, maybe in 5 years. When it does, we want to flip a feature flag and immediately see what's happening inside, not rediscover the debugging journey from scratch.

Every time you add a `println!` to understand what's going on, you're doing valuable work — you're identifying the points in the code where visibility matters. That knowledge shouldn't be thrown away after the bug is fixed.

**Tracing calls are observability infrastructure for your future self.**

The pattern described here makes that observability zero-cost: when the feature is off, the macros compile to nothing. So there's no reason to remove them. Keep them. Accumulate them. Build a codebase that can be illuminated on demand.

## The Rule

**Never use `println!` or `eprintln!` for debugging. They are evil.**

**Never use `#[instrument]`**. It requires `tracing-attributes` which pulls in `syn`, adding 15-20 seconds to compile times.

## How It Works

Facet uses [tracing](https://docs.rs/tracing) as an optional dependency with crate-level forwarding macros that compile to nothing when the feature is disabled.

### The Forwarding Macros

Each crate that uses tracing defines forwarding macros like this:

```rust
// src/tracing_macros.rs

/// Emit a trace-level log message.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        #[cfg(any(test, feature = "tracing"))]
        tracing::trace!($($arg)*);
    };
}
```

This pattern:
- Compiles to **nothing** when `tracing` feature is disabled (zero runtime cost)
- Automatically enables tracing in tests via `cfg(test)`
- Forwards to the real `tracing::trace!` when enabled

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
    // FACET_LOG=trace will show trace output
}
```

The test helper:
- Initializes a tracing subscriber with nice formatting
- Shows elapsed time since test start
- Installs color-backtrace for better panic output
- Respects the `FACET_LOG` environment variable for filtering

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

Use appropriate levels:
- `trace!` — Very verbose: function entry/exit, loop iterations, detailed state
- `debug!` — Intermediate values, decision points, key milestones
- `info!` — High-level operations (usually for production logs)
- `warn!` / `error!` — Problems and failures

**Prefer `debug!` and `trace!`** for debugging instrumentation. These are the levels you'll use most when investigating test failures.

## Running Tests with Tracing

```bash
# Run tests with default trace-level output
cargo nextest run -p facet-json

# Filter to specific modules/crates
FACET_LOG=facet_format=trace cargo nextest run -p facet-json

# Run a specific test with full tracing
FACET_LOG=trace cargo nextest run -p facet-json -E 'test(rename)'
```

The `FACET_LOG` variable uses [tracing's filter syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html):

```bash
# Everything at trace level
FACET_LOG=trace

# Only facet_format at debug, everything else at warn
FACET_LOG=warn,facet_format=debug

# Multiple crates
FACET_LOG=facet_format=trace,facet_reflect=debug
```

## Production Use

To enable tracing in a release build:

```bash
RUST_LOG=info cargo run --release --features tracing
```

Or enable it in your application's `Cargo.toml`:

```toml
[dependencies]
facet-json = { version = "...", features = ["tracing"] }
```

## Key Principles

1. **Never remove tracing calls** — They're zero-cost when disabled. Keep them as documentation of important code paths.

2. **Use the crate-local macros** — Import `use crate::trace;`, not `use tracing::trace;`. This ensures the conditional compilation works.

3. **Prefer structured fields** — Use `trace!(field_name, ?debug_value, "message")` rather than format strings. Structured fields are more useful for filtering and analysis.

4. **Add tracing when debugging** — If you add a `println!` to understand something, convert it to a `trace!` or `debug!` call before committing. Your future self will thank you.
