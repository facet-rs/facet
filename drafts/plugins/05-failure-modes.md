# Failure Modes and Risks

The handshake pattern has inherent fragility because there's no central coordinator—just
a chain of trust between plugins. If any plugin in the chain breaks, everything fails.

## 1. Plugin Bug Breaks the Chain

If `facet_error::__facet_invoke!` has a bug and emits malformed output, the next plugin
in the chain never gets invoked:

```
derive(Facet) → error_invoke!(corrupted) → ??? (debug_invoke never called)
```

The user sees a cryptic error about unexpected tokens, pointing at the derive macro
rather than the buggy plugin.

## 2. Protocol Version Mismatch

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

## 3. Poor Error Attribution

When something goes wrong, the compiler error points at the `#[derive(Facet)]` line,
not at the plugin that caused the problem. Users have no way to know which plugin
in the chain failed.

## Mitigations

### Protocol Versioning

Include a version marker in the invocation format:

```rust
plugin_invoke! { @version { 1 } @tokens {...} ... }
```

Plugins can detect version mismatches and emit clear errors.

### Validation in Finalize

`__facet_finalize!` should validate its input and emit helpful diagnostics if something
looks wrong (e.g., "expected @plugins section").

### Plugin Testing

Each plugin should have integration tests that verify correct forwarding through the chain.

### Diagnostic Breadcrumbs

Each plugin could append to a `@trace` section:

```rust
@trace { "facet-error v0.1.0", "facet-display v0.2.0" }
```

On error, this trace could help identify which plugin was last to touch the chain.

## Fundamental Tradeoff

These mitigations reduce but don't eliminate the risk. The fundamental tradeoff is:
deterministic expansion order (handshake) vs. robustness to individual plugin failures
(shared state, which has its own problems with load order).
