# Facet Plugin System Design

This directory contains the design documents for facet's plugin system.

## Documents

- [01-background.md](01-background.md) — Problem statement and failed approaches
- [02-deferred-parsing.md](02-deferred-parsing.md) — The core insight: defer parsing to a single finalize step
- [03-scripting-language.md](03-scripting-language.md) — Template/script language for plugins (WIP)
- [04-examples.md](04-examples.md) — Concrete examples: facet-display, facet-error
- [05-failure-modes.md](05-failure-modes.md) — Risks and mitigations
- [06-error-messages.md](06-error-messages.md) — Better errors via `#[diagnostic::on_unimplemented]`
- [07-open-questions.md](07-open-questions.md) — Unresolved design questions

## Proof of Concept

See `poc/plugin-poc/` for a working proof-of-concept demonstrating the macro handshake pattern.

## References

- [PR #1143: Failed dylib approach](https://github.com/facet-rs/facet/pull/1143)
- [proc-macro-handshake](https://github.com/alexkirsz/proc-macro-handshake)
- [Issue #1139: facet-error vision](https://github.com/facet-rs/facet/issues/1139)
