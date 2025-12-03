+++
title = "Development Setup"
weight = 1
insert_anchor_links = "heading"
+++

## Prerequisites

Install [just](https://github.com/casey/just):

```bash
# macOS
brew install just

# or with cargo
cargo install just
```

## Quick Start

```bash
git clone https://github.com/facet-rs/facet
cd facet
just
```

If `just` succeeds, CI will most likely pass.

## Running Tests

facet uses [cargo-nextest](https://nexte.st/):

```bash
# All tests
just test

# Specific crate
cargo nextest run -p facet-json

# With logging
RUST_LOG=debug cargo nextest run -p facet-reflect
```

## Other Commands

```bash
# Miri (undefined behavior check)
just miri

# no_std compatibility
just nostd-ci

# Clippy
cargo clippy --workspace --all-features

# Build docs
just docs
```

## Testing Tips

```bash
# Filter by test name
cargo nextest run -p facet-reflect partial

# With output
RUST_LOG=trace cargo nextest run -- --nocapture
```

### Miri

Always run Miri when modifying unsafe code:

```bash
just miri
```

### Snapshot Testing

Some crates use [insta](https://docs.rs/insta). Update snapshots with:

```bash
cargo insta review
```
