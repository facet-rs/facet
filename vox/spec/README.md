# Spec suite

This directory is for the **current** (non-legacy) compliance suite targeting the canonical spec in
`docs/content/`.

The old suite lives in `spec-legacy/`.

## How it runs

The `spec-tests` Rust crate (under `spec/`) is a `tests/`-based harness intended to be run with `cargo nextest run`.

Each test spawns a **subject** (an implementation under test) using `SUBJECT_CMD` and drives it via:
- env vars (e.g. `PEER_ADDR`, set by the harness)
- optional stdin commands (if a test needs the subject to initiate an action)

Subjects are expected to be thin adapters, one per implementation (e.g. Rust, Node, Swift).

## Running locally

Examples:
- `SUBJECT_CMD=./target/release/subject-rust cargo nextest run -p spec-tests`
- `SUBJECT_CMD='node subject.js' cargo nextest run -p spec-tests`

`spec-tests` will fail fast if `SUBJECT_CMD` is not set.

## CI

In CI, run the suite once per subject (either as 3 separate jobs, or a matrix over subjects), by setting `SUBJECT_CMD` per job.
