# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
