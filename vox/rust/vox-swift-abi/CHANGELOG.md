# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/bearcove/vox/compare/vox-swift-abi-v0.4.0...vox-swift-abi-v0.5.0) - 2026-05-15

### Other

- apply dependency upgrades ([#308](https://github.com/bearcove/vox/pull/308))
- Multi-field struct codec FFI: Point<u32, u64, bool> via Rust codec
- Niche-filled enum FFI round-trip: Optional<UnsafeRawPointer> via Rust codec
- Codec FFI: Swift round-trips a Foo through the Rust postcard codec
- End-to-end Swift demo: probe a Swift enum, write .ok(31) layout-driven
- per-variant match/store patterns (replaces simple-tag fields)
- Codec architecture reference + calibration-only Swift FFI surface
- Share codec scaffolding between Rust JIT and Swift codec
- swift ffi codec preparation
- Add Swift codec descriptor entrypoint
- Add Swift value descriptor ABI
