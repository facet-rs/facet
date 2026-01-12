# Testing Architecture

This document describes the testing infrastructure for roam protocol conformance.

## Overview

The spec-tests crate provides a **protocol conformance test suite** that validates
roam implementations at the wire level. The goal is to test implementations as
black boxes, communicating only via the roam wire protocol.

## Subjects

A **subject** is an implementation being tested. Each subject implements the
`Testbed` service and can run in server or client mode.

| Language | Location | Entry Point |
|----------|----------|-------------|
| Rust | `rust/subject-rust/` | `target/release/subject-rust` |
| TypeScript | `typescript/subject/` | `subject-ts.sh` |
| Swift | `swift/subject/` | `subject-swift.sh` |

Subjects are spawned by the test harness and communicate via stdin/stdout or TCP.

## Test Categories

### 1. Wire-Level Protocol Tests

**Location:** `spec/spec-tests/tests/protocol.rs`

Tests fundamental protocol behavior:
- Hello handshake timing and ordering
- Unknown Hello variant handling
- Payload size limits
- Stream ID validation (zero reserved, unknown IDs)

**How it works:**
```
[Test Harness] ----raw COBS frames---> [Subject as Server]
       ^                                      |
       +----------raw COBS frames-------------+
```

The harness uses `CobsFramed` to send/receive raw wire messages. No roam
runtime types are used - just `roam_wire::Message`.

### 2. RPC Mechanics Tests

**Location:** `spec/spec-tests/tests/testbed.rs`

Tests RPC call mechanics:
- Request/Response roundtrip
- Unknown method → `RoamError::UnknownMethod`
- Invalid payload → `RoamError::InvalidPayload`
- Request pipelining (multiple in-flight requests)

**How it works:** Same as protocol tests - wire-level communication with subject.

### 3. Channeling Tests

**Location:** `spec/spec-tests/tests/channeling.rs`

Tests channeling (streaming) RPC methods:
- Client-to-server channel (`sum`: send numbers, get total)
- Server-to-client channel (`generate`: request N items, receive values)
- Bidirectional channels (`transform`: echo strings)

**How it works:** Same as protocol tests - wire-level communication with subject.

### 4. Client Mode Tests

**Location:** `spec/spec-tests/tests/client_mode.rs`

Tests the generated client code by running the subject as a client:
- Subject connects to harness
- Subject uses generated `TestbedClient` to make calls
- Harness validates the calls arrive correctly

**How it works:**
```
[Test Harness as Server] <----roam protocol----> [Subject as Client]
```

**Note:** The harness must implement the `Testbed` service to act as server.
This currently uses roam runtime types (`Rx`, `Tx`, `ServiceDispatcher`).
See "Design Considerations" below.

### 5. Browser/WebSocket Tests

**Location:** `typescript/tests/playwright/websocket.spec.ts`

Tests TypeScript client running in a browser:
- Playwright opens browser with test page
- Browser connects to `ws-echo-server` via WebSocket
- Runs Testbed service calls from browser environment

**How it works:**
```
[Browser (Playwright)] ---WebSocket---> [ws-echo-server (Rust)]
```

### 6. Cross-Language Matrix

**Location:** `spec/spec-tests/src/bin/cross_language_test.rs`

Tests interoperability between different language implementations:
- Spawns server in language A
- Runs client in language B
- Validates they can communicate

**Current matrix:**
- Rust ↔ Rust (TCP)
- Rust ↔ TypeScript (WebSocket)
- Rust ↔ Swift (TCP) — in progress
- TypeScript ↔ TypeScript — in progress

## Test Binaries

| Binary | Purpose |
|--------|---------|
| `tcp-echo-server` | Rust TCP server for cross-language tests |
| `ws-echo-server` | Rust WebSocket server for browser tests and cross-language |
| `tcp-echo-client` | Rust TCP client for cross-language tests |
| `cross_language_test` | Orchestrates cross-language test matrix |

## Running Tests

```bash
# Build subjects first
cargo build --release -p subject-rust

# Run protocol conformance tests (uses subject-rust by default)
cargo test -p spec-tests

# Run with a different subject
SUBJECT_CMD="./typescript/subject/subject-ts.sh" cargo test -p spec-tests

# Run cross-language tests
cargo run -p spec-tests --bin cross_language_test

# Run browser tests
cd typescript/tests/playwright && pnpm test
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `SUBJECT_CMD` | Command to run the subject | `./target/release/subject-rust` |
| `SUBJECT_MODE` | `server` or `client` | `server` |
| `PEER_ADDR` | Address to connect to (client mode) | — |
| `CLIENT_SCENARIO` | Which test scenario to run as client | `echo` |
| `ROAM_WIRE_SPY` | Enable wire-level message logging | — |

## Design Considerations

### Why Subjects?

The test harness treats implementations as black boxes. This ensures:
1. Tests validate wire protocol, not internal implementation
2. Same tests work for any language implementation
3. Clear separation between "test harness" and "implementation under test"

### Issue: Harness Using roam Internals

Currently, `client_mode.rs` uses roam runtime types (`Rx`, `Tx`,
`roam_stream::establish_acceptor`) to act as a server. This is problematic
because:

1. We're testing roam-against-roam rather than testing the wire protocol
2. Bugs in the harness could mask bugs in the subject
3. The harness should be a minimal wire-level implementation

**Proposed fix:** The harness should implement a wire-level server that:
- Does hello exchange manually
- Dispatches requests based on method ID
- Handles streaming via raw Data/Close messages

This would make `client_mode.rs` consistent with `protocol.rs` and
`streaming.rs`, which already operate at the wire level.

### Echo Servers vs Subjects

The `tcp-echo-server` and `ws-echo-server` binaries implement the same
`Testbed` service as `subject-rust`. The difference:

- **Subjects** are implementations being tested
- **Echo servers** are reference servers for cross-language testing

In practice, `subject-rust` could potentially replace the echo servers,
but having separate binaries allows different transport configurations.
