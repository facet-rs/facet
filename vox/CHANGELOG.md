# Changelog

All notable changes to Vox are documented here.

## [Unreleased]

### Changed

- Remove the implicit `From<DriverCaller> for ()` conversion. Use `NoopCaller` when you need to retain root connection liveness without exposing root RPC methods.

- No entries yet.

## [7.0.0] - 2026-03-02

### Breaking

- Service trait implementations no longer receive `&Context`; methods now either:
  - return owned values directly, or
  - receive `call: impl vox::Call<T, E>` for explicit `'vox` borrowed response paths.
- Generated service trait naming and generated descriptor access changed:
  - Generated service traits are still named `{Service}`.
  - Codegen inputs moved from `*_service_detail()` to `*_service_descriptor()`.
- Client construction and session setup were updated:
  - `ConnectionHandle` is no longer a `Caller`; clients are created from `driver.caller()`.
  - Session bootstrapping moved from `accept_framed` / `initiate_framed` to `session::acceptor` / `session::initiator` with `.establish()`.

### Changed

- Client return types were standardized:
  - Owned returns remain direct values (e.g. `T`).
  - Borrowed `'vox` returns now return `SelfRef<T>`.
  - Generated Rust clients do not expose response metadata in return types.
- Channel APIs now use const-generic credit: `Tx<T, N>` / `Rx<T, N>`, with default credit `N = 16`.
- SHM hosting moved to a smaller, lower-level v7 API surface:
  - `ShmHost`, `bootstrap`, `driver`, `AddPeerOptions`, `MultiPeerHostDriver` are no longer part of the API surface used for orchestration.
  - New primitives are `vox_shm::segment::{Segment, SegmentConfig}` and related methods (`reserve_peer`, `claim_peer`, `attach_peer`, `detach_peer`, `recover_crashed_peer`).
  - Orchestration helpers are now focused and explicit via `vox_shm::ShmLink`, `vox_shm::host::HostHub`, and `vox_shm::host` ticket/link helpers.

### Added

- Explicit v7 virtual-connection model in session API:
  - Root sessions are established via `session::{initiator,acceptor}.establish()`.
  - Virtual connections are opened through `SessionHandle::open_connection(...)`.
  - `on_connection(...)` hooks are now opt-in; inbound virtual opens are rejected by default when not configured.
- Added migration guidance for borrowed-response methods:
  - Use `call.ok(...)`, `call.err(...)`, and `call.reply(...)` on `call: impl Call<...>` for explicit borrow-based replies.

### Removed

- `client.method(...).with_metadata(...)` does not have a generated call-builder equivalent in v7; use lower-level request construction when metadata injection is required.
