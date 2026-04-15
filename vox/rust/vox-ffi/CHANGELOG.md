# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/bearcove/vox/compare/vox-ffi-v0.3.1...vox-ffi-v0.4.0) - 2026-04-15

### Fixed

- address all clippy warnings across workspace

### Other

- Remove link permits and queue outbound sends ([#283](https://github.com/bearcove/vox/pull/283))
- Add tracing throughout vox-ffi endpoint lifecycle
- subject retry harness and ABI updates
- Rust->Rust FfiLink tests
- Fold vox-ffi bridge into lib.rs
- Rewrite Vox FFI link bridge
- UnixAcceptor changes, wip ffi link
- rip out buf_pool, mpsc channel, and background writer task
