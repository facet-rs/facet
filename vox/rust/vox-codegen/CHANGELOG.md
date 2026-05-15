# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/bearcove/vox/compare/vox-codegen-v0.4.0...vox-codegen-v0.5.0) - 2026-05-15

### Other

- apply dependency upgrades ([#308](https://github.com/bearcove/vox/pull/308))
- add nonisolated to Swift value witness functions
- per-variant match/store patterns (replaces simple-tag fields)
- Add Swift codec descriptor entrypoint
- Take care of clippy warnings
- channel capacity etc.
- typed initiator service routing — service name in codegen + Session.initiator(expecting:)
- swift codegen: emit decode<Name>(from:) for named types
- swift codegen: keyword-escape identifiers, dedupable types, fix Unit decode
- swift codegen: emit valid identifiers for tuple-struct positional fields
- migrate channel binding from BindingSchema to Schema/SchemaKind/TypeRef
- Fix TypeScript channel lifetime semantics
- Cache args_have_channels on MethodDescriptor, drop the per-request walk

## [0.4.0](https://github.com/bearcove/vox/compare/vox-codegen-v0.3.1...vox-codegen-v0.4.0) - 2026-04-15

### Fixed

- *(vox-codegen)* remove dead code and fix unused import warnings
- *(wasm-inprocess-tests)* sync impl with updated Testbed trait and APIs
- *(swift)* store wire schema CBOR as binary resource instead of inline literal
- *(swift-codegen)* break encoder out of taskSender call to fix Swift type-checker timeout

### Other

- Split payload decode errors
- Add nonisolated
- swift codegen: extract response envelope decoding into runtime helpers
- *(swift)* replace [UInt8] concatenation and Data with ByteBuffer throughout
- rename ChannelingDispatcher -> Dispatcher
- Add awaitable vox::connect builder and fix all-features regressions
- reports, yada yada
- Add gnarly benchmark workload and regenerate Swift bindings
- Add vox-service metadata to TypeScript and Swift handshakes
- *(ts)* avoid parameter properties and type catch bindings

## [0.3.0](https://github.com/bearcove/vox/compare/vox-codegen-v0.2.2...vox-codegen-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))
