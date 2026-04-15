# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/bearcove/vox/compare/vox-v0.3.1...vox-v0.4.0) - 2026-04-15

### Added

- *(vox)* add initial-connect waiting via ConnectBuilder::wait_for_service
- *(vox)* make VoxListener, ChannelListener, and ConnectionAcceptor WASM-compatible
- *(swift)* implement ConnectionAcceptor / ConnectionRequest / PendingConnection

### Fixed

- *(vox)* cap each wait_for_service attempt by remaining deadline
- *(windows)* gate Unix-only lock logic in serve_local behind #[cfg(unix)]
- address all clippy warnings across workspace

### Other

- Remove link permits and queue outbound sends ([#283](https://github.com/bearcove/vox/pull/283))
- Rust->Rust FfiLink tests
- Restore benchmark, take care of lints
- *(vox)* verify schema incompatibility is non-retryable
- Add awaitable vox::connect builder and fix all-features regressions
- Remove SHM transport and fix workspace builds
- rip out buf_pool, mpsc channel, and background writer task
- Tighten benchmark harness and profiling workflow
- Fix Swift/Rust SHM bootstrap interop and benchmark wiring
- Return Arc<ExtractedSchemas> from cache, eliminate HashSet clones
- Use &'static Shape directly as cache key instead of pointer casts
- Cache translation plans per (method, direction, type) on SchemaRecvTracker
- SHM + WSS serve() simplifications
- Split highlevel.rs into per-transport modules, add SHM listener
- gate parse_query_params on transport-websocket-tls only
- Gate parse_query_params behind feature flags
- Add WSS (WebSocket over TLS) support to vox::serve()
- Add WsListener, ChannelListener, and ws:// support to vox::serve()
- Replace generic io::Error with typed ServeError for vox::serve()
- Health-check existing server when local lock is held
- Add local:// support to vox::serve() with flock-based exclusivity
- Add string-based vox::serve(), rename listener-based to serve_listener()
- Rewrite schema_resume_tests with source-based recoverer
- Accept macro snapshots, add registry to manual resume test
- Fix lifetime for Peek around zerocopy args
- Clippy deref
- guard-based channel binder, borrowed arg types in macro
- Fix SelfRef call sites to use get() instead of deref
- Fix SelfRef soundness: replace Deref with Reborrow + get()
- Fix CI: add job timeouts, fix clippy warnings, clean up examples
- Replace glob vox_core re-export with curated public API surface
- VoxListener trait: serve() accepts TcpListener, LocalLinkAcceptor
- Add vox::serve() for accepting connections in a loop
- Add vox::serve() for accepting connections in a loop
- Inject vox-connection-kind metadata (root/virtual)
- Unify root + virtual connections through single ConnectionAcceptor
- debug cleanup, still investigating vconn failures
- unify root + virtual through ConnectionAcceptor
- PendingConnection + simplified ConnectionAcceptor
- Connect timeout + recovery timeout, SessionConfig refactor
- Saving work before claude decides to git stash the world
- test split
- Replace ServiceFactory with blanket ConnectionAcceptor impl for closures
- Service routing: session.open::<Client>(), ServiceFactory, metadata helpers
- Add metadata to CBOR handshake (Hello/HelloYourself)
- Use Cow in MetadataEntry and MetadataValue
- Remove redundant SessionHandle from establish return types
- Add middleware pipeline tests, remove bogus cancel test
- Extract SessionConfig to deduplicate 5 builder structs
- Port proxy_connections test to generated clients
- Port vconn, cancellation, and transport tests to generated clients
- Concrete Caller struct, kill Caller trait and friends
- Add ShmLinkSource and wire shm:// into vox::connect()
- resolve DNS on each connect with configurable timeouts
- Add WsLinkSource for reconnectable WebSocket connections
- Parse TCP address properly, add ws/shm error messages in connect()
- tcp_link_source, better types for connect etc.
- Accept impl Display in connect(), default bare addresses to TCP
- Require scheme:// in connect(), factor out connect_bare helper
- Move connect into its own submodule
- Support local:// transport in vox::connect()
- Store SessionHandle in generated clients, add FromVoxSession trait
- Add VoxClient trait, move closed/is_connected to free functions
- Add connect example, proper address parsing, document inherent method clashes
- Add vox::connect() convenience function and design doc
- *(vox)* add minimal tcp transport rustdoc example
- Improve service macro diagnostics and doctest behavior
- Manual docs writing
- always expose transport module and default tcp/local
- add feature-gated transport facade modules

## [0.3.0](https://github.com/bearcove/vox/compare/vox-v0.2.2...vox-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))

### Changed

- Remove the implicit `From<DriverCaller> for ()` conversion. Use `NoopCaller` with `establish::<NoopCaller>(...)` when you want root liveness without a root client API.
