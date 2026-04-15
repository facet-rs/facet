# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/bearcove/vox/compare/vox-core-v0.3.1...vox-core-v0.4.0) - 2026-04-15

### Added

- *(vox)* make VoxListener, ChannelListener, and ConnectionAcceptor WASM-compatible
- *(vox-core)* enable retries and reconnects on WebAssembly
- *(vox)* classify VoxError and SessionError retryability

### Fixed

- *(swift-codegen)* break encoder out of taskSender call to fix Swift type-checker timeout
- inject vox-service from Client::SERVICE_NAME in root session establish
- address all clippy warnings across workspace

### Other

- bump facet and facet-core to 0.45 ([#285](https://github.com/bearcove/vox/pull/285))
- Remove link permits and queue outbound sends ([#283](https://github.com/bearcove/vox/pull/283))
- default fresh initiators to non-resumable
- remove IntoConduit
- Add awaitable vox::connect builder and fix all-features regressions
- Remove SHM transport and fix workspace builds
- Commit remaining session and Swift package updates
- Return Arc<ExtractedSchemas> from cache, eliminate HashSet clones
- Remove unused Conduit import
- Remove SessionHandle::resume(), rewrite resume tests with recoverers
- Fix remaining test failures: resume, recoverer metadata, snapshots
- Accept macro snapshots, add registry to manual resume test
- Inject vox-service metadata on acceptor side too
- More clippyd eref
- Clippy deref
- fix forwarded_payload/RequestContext lifetimes, macro double-get
- Add Reborrow for primitive/string types and clean remaining selfref callsites
- SelfRef get() migration — trait, macro, impls done, call sites in progress
- Use SelfRef::get() at callsites after deref removal
- Fix SelfRef soundness: replace Deref with Reborrow + get()
- Fix CI: add job timeouts, fix clippy warnings, clean up examples
- Inject vox-connection-kind metadata (root/virtual)
- Unify root + virtual connections through single ConnectionAcceptor
- debug cleanup, still investigating vconn failures
- unify root + virtual through ConnectionAcceptor
- PendingConnection + simplified ConnectionAcceptor
- metadata ergonomics, ConnectionRequest, simplified ConnectionAcceptor
- Connect timeout + recovery timeout, SessionConfig refactor
- Saving work before claude decides to git stash the world
- test split
- No fully-qualified fetish here
- Replace ServiceFactory with blanket ConnectionAcceptor impl for closures
- Service routing: session.open::<Client>(), ServiceFactory, metadata helpers
- Auto-inject vox-service metadata from client type
- Add metadata to CBOR handshake (Hello/HelloYourself)
- Use Cow in MetadataEntry and MetadataValue
- Remove redundant SessionHandle from establish return types
- Extract SessionConfig to deduplicate 5 builder structs
- Re-enable driver tests with pub(crate) escape hatch
- Concrete Caller struct, kill Caller trait and friends
- Store SessionHandle in generated clients, add FromVoxSession trait
- Add VoxClient trait, default resumable to false, modernize examples
- Add vox::connect() convenience function and design doc
- resolve workspace warnings under strict linting
- establish_noop => establish_call_only
- Introduce establish_noop, some code comments

## [0.3.1](https://github.com/bearcove/vox/compare/vox-core-v0.3.0...vox-core-v0.3.1) - 2026-03-30

### Other

- Driver inflight cleanup and idem store (+ reflective server middleware tracing etc.) ([#271](https://github.com/bearcove/vox/pull/271))

## [0.3.0](https://github.com/bearcove/vox/compare/vox-core-v0.2.2...vox-core-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))

### Changed

- Remove the implicit `From<DriverCaller> for ()` conversion and add `NoopCaller` for liveness-only root handles.

## [7.0.0-alpha.3](https://github.com/bearcove/vox/compare/vox-core-v7.0.0-alpha.2...vox-core-v7.0.0-alpha.3) - 2026-03-03

### Other

- Add MaybeSend bound on erased caller
