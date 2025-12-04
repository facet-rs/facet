+++
title = "Development Setup"
weight = 1
insert_anchor_links = "heading"
+++

## Prerequisites

### cargo-nextest

We use [cargo-nextest](https://nexte.st/) instead of `cargo test`. It's faster and runs each test in its own process, which lets us install a process-wide tracing subscriber without conflicts.

If you try to run tests with `cargo test`, you'll see a banner telling you to use `cargo nextest run` instead.

```bash
cargo install cargo-nextest
```

### cargo-insta

We use [insta](https://insta.rs/) for snapshot testing. When a snapshot changes, review it with:

```bash
cargo insta review
```

### just (optional)

[just](https://github.com/casey/just) is a task runner that makes it easy to run common commands. It's not required — you can run the underlying commands directly.

```bash
# macOS
brew install just

# or with cargo
cargo install just
```

## Quick start

```bash
git clone https://github.com/facet-rs/facet
cd facet
./scripts/install-hooks  # Install pre-push hook
just ci
```

`just ci` runs locally what CI runs remotely. If it passes, your PR will likely pass CI.

## Git hooks

We provide a pre-push hook that catches common CI failures before you push:

```bash
./scripts/install-hooks
```

This installs a hook that runs before each `git push`:
- `cargo clippy --all-targets --all-features` — catches lint errors
- `cargo test --doc` — ensures doc examples compile

If clippy fails, the hook will suggest running `cargo clippy --fix` to auto-fix issues.

The hook is optional but recommended — it saves the round-trip of pushing, waiting for CI, and fixing issues.

## Common commands

```bash
# Run all tests
cargo nextest run

# Run tests for a specific crate
cargo nextest run -p facet-json

# With tracing output
RUST_LOG=debug cargo nextest run -p facet-reflect

# Review snapshot changes
cargo insta review

# Check for undefined behavior
just miri

# Check no_std compatibility
just nostd-ci
```
