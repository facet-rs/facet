# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/bearcove/vox/compare/vox-macros-core-v0.3.1...vox-macros-core-v0.4.0) - 2026-04-15

### Other

- Accept macro snapshots, add registry to manual resume test
- Fix lifetime for Peek around zerocopy args
- guard-based channel binder, borrowed arg types in macro
- fix forwarded_payload/RequestContext lifetimes, macro double-get
- Fix SelfRef soundness: replace Deref with Reborrow + get()
- PendingConnection + simplified ConnectionAcceptor
- metadata ergonomics, ConnectionRequest, simplified ConnectionAcceptor
- Fix test split compilation: ConnectionSetup enum, shared utils, snapshot updates
- Saving work before claude decides to git stash the world
- test split
- No fully-qualified fetish here
- Auto-inject vox-service metadata from client type
- Concrete Caller struct, kill Caller trait and friends
- Store SessionHandle in generated clients, add FromVoxSession trait
- Add VoxClient trait, default resumable to false, modernize examples
- Add VoxClient trait, move closed/is_connected to free functions
- Improve service macro diagnostics and doctest behavior
- Move service trait bounds

## [0.3.0](https://github.com/bearcove/vox/compare/vox-macros-core-v0.2.2...vox-macros-core-v0.3.0) - 2026-03-29

### Other

- Expose reflective server middleware payloads and improve Vox runtime tracing ([#267](https://github.com/bearcove/vox/pull/267))
