+++
title = "Development Setup"
weight = 1
insert_anchor_links = "heading"
+++

## Prerequisites

### cargo-nextest

We use [cargo-nextest](https://nexte.st/) instead of `cargo test`. It runs each test in its own process, which lets us install a process-wide tracing subscriber without conflicts.

```bash
cargo install cargo-nextest
```

### cargo-insta

We use [insta](https://insta.rs/) for snapshot testing. When a snapshot changes, review it with:

```bash
cargo insta review
```

### just

[just](https://github.com/casey/just) is a task runner. The justfile defines common workflows.

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
just ci
```

`just ci` runs locally what CI runs remotely. If it passes, your PR will likely pass CI.

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
just nostd
```
