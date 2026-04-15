# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/bearcove/vox/compare/vox-types-v0.3.1...vox-types-v0.4.0) - 2026-04-15

### Added

- *(vox)* classify VoxError and SessionError retryability

### Other

- Remove link permits and queue outbound sends ([#283](https://github.com/bearcove/vox/pull/283))
- Add awaitable vox::connect builder and fix all-features regressions
- Remove SHM transport and fix workspace builds
- Return Arc<ExtractedSchemas> from cache, eliminate HashSet clones
- Use &'static Shape directly as cache key instead of pointer casts
- Cache extract_schemas() globally by Shape pointer
- Cache translation plans per (method, direction, type) on SchemaRecvTracker
- Fix lifetime for Peek around zerocopy args
- guard-based channel binder, borrowed arg types in macro
- fix forwarded_payload/RequestContext lifetimes, macro double-get
- Add Reborrow for primitive/string types and clean remaining selfref callsites
- SelfRef get() migration — trait, macro, impls done, call sites in progress
- Fix SelfRef soundness: replace Deref with Reborrow + get()
- Fix CI: add job timeouts, fix clippy warnings, clean up examples
- metadata ergonomics, ConnectionRequest, simplified ConnectionAcceptor
- No fully-qualified fetish here
- Service routing: session.open::<Client>(), ServiceFactory, metadata helpers
- Add metadata to CBOR handshake (Hello/HelloYourself)
- Use Cow in MetadataEntry and MetadataValue
- Concrete Caller struct, kill Caller trait and friends
- Add VoxClient trait, move closed/is_connected to free functions
- always expose transport module and default tcp/local
- implement Display and Error for VoxError
- resolve workspace warnings under strict linting

## [0.3.0](https://github.com/bearcove/vox/compare/vox-types-v0.2.2...vox-types-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))

## [7.0.0-alpha.3](https://github.com/bearcove/vox/compare/vox-types-v7.0.0-alpha.2...vox-types-v7.0.0-alpha.3) - 2026-03-03

### Other

- Add MaybeSend bound on erased caller
