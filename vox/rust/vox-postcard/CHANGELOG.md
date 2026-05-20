# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0](https://github.com/bearcove/vox/compare/vox-postcard-v0.6.1...vox-postcard-v0.7.0) - 2026-05-20

### Other

- vox-postcard + vox-jit: Def::DynamicValue codec + actionable JIT errors

## [0.5.0](https://github.com/bearcove/vox/compare/vox-postcard-v0.4.0...vox-postcard-v0.5.0) - 2026-05-15

### Added

- Cranelift translation JIT for postcard decode

### Fixed

- detect cycles in build_plan so recursive types stop hanging

### Other

- apply dependency upgrades ([#308](https://github.com/bearcove/vox/pull/308))
- VOX_JIT_TRACE_LOWER trace for cycle-detector emissions
- Share codec scaffolding between Rust JIT and Swift codec
- Take care of clippy warnings
- No zero initial credit
- JIT encode: Box<T> + recursive type self-recursion
- JIT decode: tail-call trailing self-recursion
- Recursive type decode: cycle-detect lowerer + CallSelf op
- Vec<bool> bulk validate + memcpy fast path
- Bulk memcpy fast path for fixed-LE Vec<T> decode/encode
- niche-aware fast path for Option<&T>
- IR interpreter: actually allocate + loop in AllocBacking
- JIT decode: emit ReadStrRef for &str, kill the wrong-descriptor fallthrough
- JIT encode: inline Option discriminator + small-copy ladder
- JIT encode: seed buffer from per-stub size hint + byte-list fast path
- write postcard variant index, not in-memory discriminant
- Add VOX_CODEC={reflect,interp,jit} codec selector
- clippy warnings-- + ... default helpers?
- Strip JIT encode helpers and calibrate Vec<T> once per family
- Fast(er) byte/varint writing
- Fix JIT Option and Result decode for benches
- more JIT
- Tighten pure JIT RPC frame coverage
- Wire pure JIT through outer RPC frames

## [0.4.0](https://github.com/bearcove/vox/compare/vox-postcard-v0.3.1...vox-postcard-v0.4.0) - 2026-04-15

### Other

- Add awaitable vox::connect builder and fix all-features regressions

## [0.3.0](https://github.com/bearcove/vox/compare/vox-postcard-v0.2.2...vox-postcard-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))
