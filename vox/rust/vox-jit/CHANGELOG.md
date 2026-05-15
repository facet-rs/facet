# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/bearcove/vox/compare/vox-jit-v0.4.0...vox-jit-v0.5.0) - 2026-05-15

### Added

- *(jit)* emit jitdump sub-range symbols for inlined element bodies
- Cranelift translation JIT for postcard decode

### Other

- apply dependency upgrades ([#308](https://github.com/bearcove/vox/pull/308))
- Make jitdump work with samply
- *(jit)* drop OnceLock + dead writeback from decode_owned_with hot path
- inline borrowed byte slice decode
- Share codec scaffolding between Rust JIT and Swift codec
- Take care of clippy warnings
- No zero initial credit
- fast-path for enum discriminants
- wip optim in codegen
- Fix macOS JIT profiling support
- JIT encode: Box<T> + recursive type self-recursion
- JIT decode: inline Box<T> alloc and bypass __rust_alloc shim
- JIT helpers: drop no_mangle to avoid duplicate-symbol with dev-dep cycle
- JIT decode: tail-call trailing self-recursion
- Decode ABI: pass consumed in/out via registers, not memory
- Recursive type decode: cycle-detect lowerer + CallSelf op
- Vec<bool> bulk validate + memcpy fast path
- JIT decode cache: drop BorrowMode key dim, swap SipHash for museair
- Bulk memcpy fast path for fixed-LE Vec<T> decode/encode
- JIT cache: drop slow-path Mutex; fall back to IR on compile failure
- Pre-resolve conduit Tx/Rx encoders/decoders at construction
- leak compiled encoders/decoders, drop the Arc
- Rename CompiledStub → CompiledEncoder/CompiledDecoder
- Drop pre-decode zero-fill of MaybeUninit output
- Drop pointer-identity hashing; lock-free fast cache via ArcSwap
- JIT decode: emit ReadStrRef for &str, kill the wrong-descriptor fallthrough
- Inline field decoders
- emit Linux jitdump so perf can annotate JIT'd encoders
- JIT encode: inline Option discriminator + small-copy ladder
- JIT encode: seed buffer from per-stub size hint + byte-list fast path
- write postcard variant index, not in-memory discriminant
- Add VOX_CODEC={reflect,interp,jit} codec selector
- More cleanups
- clippy warnings-- + ... default helpers?
- Less-stupid encoding
- Name JIT stubs by shape so profilers can symbolicate them
- Fix encode of struct fields after an enum
- Add shape-pointer fast-path cache for encode/decode stubs
- Strip JIT encode helpers and calibrate Vec<T> once per family
- Fast(er) byte/varint writing
- Fix JIT Option and Result decode for benches
- more JIT
- Tighten pure JIT RPC frame coverage
- Wire pure JIT through outer RPC frames
