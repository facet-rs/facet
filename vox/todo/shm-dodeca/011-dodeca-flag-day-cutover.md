# Phase 011: Dodeca Flag-Day Cutover (rapace → roam + roam-shm)

## Goal

Switch dodeca (host + all cells) from the rapace runtime to the roam runtime,
using roam-shm as the transport.

This is a “flag day” in the sense that host and cells must move together.

## Preconditions

- Phases 001–007: SHM substrate complete (segment file, doorbells, spawn tickets,
  death handling, wakeups, variable slots, extents).
- Phase 008: roam driver works over SHM (no Hello/Credit frames).
- Phase 009: tracing across cells works (or is explicitly deferred).
- Phase 010: tunnel streams exist (or dodeca is refactored to avoid tunnels).

## Cutover Plan (High Level)

### 1. Replace Service Macros + Generated Types

- Replace `#[rapace::service]` with `#[roam::service]` in all `*-proto` crates.
- Update any codegen expectations (method IDs, request/response types).

### 2. Replace Cell Runtime

- Replace `rapace_cell::run*` usage in all cells with a roam equivalent:
  - parse spawn args (`--hub-path`, `--peer-id`, `--doorbell-fd`)
  - attach via `ShmGuest::attach_with_ticket`
  - establish driver + dispatcher

### 3. Replace Host Cell Spawner

- Replace `HubHost` + rapace SHM transport wiring in `crates/dodeca/src/cells.rs`
  with:
  - `roam_shm::ShmHost`
  - spawn tickets + doorbells
  - per-peer driver + `ConnectionHandle`

### 4. Replace Tunneling

- Migrate `cell-http` and host-side tunnel consumers to `Tx/Rx<Vec<u8>>` tunnels.

### 5. Replace Tracing

- Install roam tracing host collector and cell forwarder.
- Ensure failure modes are safe (lossy buffering, no deadlocks).

## Tasks

- [ ] One proto crate end-to-end (host + single cell) on roam-shm
- [ ] Migrate remaining proto crates
- [ ] Migrate all cells to roam runtime
- [ ] Migrate host cell orchestration
- [ ] Migrate tunneling
- [ ] Migrate tracing
- [ ] Delete rapace dependencies from dodeca (only after parity is proven)

## Notes

- Keep the migration incremental at the code level even if the end state is a
  flag day: get one cell working first, then scale out.
