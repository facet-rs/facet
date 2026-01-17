+++
title = "GitHub Actions"
weight = 3
slug = "github-actions"
insert_anchor_links = "heading"
+++

A GitHub Actions workflow in YAML vs Styx.

```compare
/// yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run tests
        run: cargo test --all-features

  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - name: Lint
        run: |
          cargo fmt --check
          cargo clippy --all-features -- -D warnings
          echo "All checks passed!"

  build:
    needs: [test, lint]
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Build
        run: cargo build --release
      - uses: actions/upload-artifact@v4
        with:
          name: binary-${{ matrix.os }}
          path: target/release/myapp*
/// styx
name CI

on {
  push branches>(main)
  pull_request branches>(main)
}

env {
  CARGO_TERM_COLOR always
  RUSTFLAGS "-D warnings"
}

jobs {
  test {
    runs-on ubuntu-latest
    steps (
      {uses actions/checkout@v4}
      {uses dtolnay/rust-toolchain@stable}
      {uses Swatinem/rust-cache@v2}
      {name "Run tests", run "cargo test --all-features"}
    )
  }

  lint {
    runs-on ubuntu-latest
    steps (
      {uses actions/checkout@v4}
      {uses dtolnay/rust-toolchain@stable, with components>"clippy, rustfmt"}
      {
        name Lint
        run <<SH,bash
          cargo fmt --check
          cargo clippy --all-features -- -D warnings
          echo "All checks passed!"
          SH
      }
    )
  }

  build {
    needs (test lint)
    runs-on "${{ matrix.os }}"
    strategy matrix>os>(ubuntu-latest macos-latest windows-latest)
    steps (
      {uses actions/checkout@v4}
      {uses dtolnay/rust-toolchain@stable}
      {name Build, run "cargo build --release"}
      {uses actions/upload-artifact@v4, with name>"binary-${{ matrix.os }}" path>"target/release/myapp*"}
    )
  }
}
```
