# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/bearcove/vox/compare/vox-jit-cal-v0.4.0...vox-jit-cal-v0.5.0) - 2026-05-15

### Added

- Cranelift translation JIT for postcard decode

### Other

- apply dependency upgrades ([#308](https://github.com/bearcove/vox/pull/308))
- data-driven encode/decode against a ValueLayout
- End-to-end Swift demo: probe a Swift enum, write .ok(31) layout-driven
- niche probe for Option<Box<T>>-shaped niche-filled enums
- per-variant match/store patterns (replaces simple-tag fields)
- repr(C) + FFI-safe with arena-backed storage
- Layout-driven enum init: probe Result<u64,()> and write Ok(31) directly
- Take care of clippy warnings
- Pre-resolve conduit Tx/Rx encoders/decoders at construction
- More cleanups
- clippy warnings-- + ... default helpers?
- Strip JIT encode helpers and calibrate Vec<T> once per family
- Fix JIT Option and Result decode for benches
