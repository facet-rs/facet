# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/bearcove/vox/compare/vox-jit-abi-v0.4.0...vox-jit-abi-v0.5.0) - 2026-05-15

### Added

- Cranelift translation JIT for postcard decode

### Other

- Share codec scaffolding between Rust JIT and Swift codec
- Fix macOS JIT profiling support
- blind aarch64-darwin path for __rust_alloc resolver
- JIT decode: inline Box<T> alloc and bypass __rust_alloc shim
- JIT helpers: drop no_mangle to avoid duplicate-symbol with dev-dep cycle
- Decode ABI: pass consumed in/out via registers, not memory
- Vec<bool> bulk validate + memcpy fast path
- Bulk memcpy fast path for fixed-LE Vec<T> decode/encode
- clippy warnings-- + ... default helpers?
- Fix JIT Option and Result decode for benches
- Wire pure JIT through outer RPC frames
