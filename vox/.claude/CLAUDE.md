# Rapace

Rapace is a high-performance RPC protocol designed for shared-memory and networked communication.

## Tracey - Spec Coverage Tracking

Tracey is a CLI tool for tracking specification coverage. It extracts rules from markdown spec documents and traces them to implementation and test code.

### Basic Commands

```bash
# Check coverage status
tracey check

# Verbose output showing all references
tracey check -v

# Different output formats
tracey check --format json
tracey check --format markdown
tracey check --format html

# Set minimum coverage threshold (fails if below)
tracey check --threshold 50

# Extract rules to JSON
tracey rules

# Show rules at a specific file/location
tracey at rust/rapace-core/src/session.rs

# Impact analysis - what code references a rule
tracey impact core.call.request.flags

# Generate traceability matrix
tracey matrix

# Interactive dashboard
tracey serve

# Show code that lacks spec references
tracey rev
```

### Configuration

Tracey is configured in `.config/tracey/config.kdl`:

```kdl
spec {
   name "rapace"
   rules_glob "docs/content/spec/**/*.md"
}
```

### Rule Markers in Spec Documents

Rules are defined in markdown using the `r[rule.id]` syntax:

```markdown
r[core.call.request.flags] The REQUEST frame MUST have the DATA flag set.
```

### Reference Markers in Code

Code references rules using these comment patterns:

```rust
// Spec: `[impl rule.id]` - Implementation of the requirement
// Spec: `[verify rule.id]` - Test that validates the requirement
// Spec: `[depends rule.id]` - This code depends on the rule being satisfied
```

Example:
```rust
/// Spec: `[impl frame.desc.size]` - exactly 64 bytes.
pub struct MsgDescHot { ... }
```

## Conformance Testing Architecture

The Rust implementation uses a two-binary conformance testing approach:

### spec-peer (Reference Peer)

The `spec-peer` binary is the **reference implementation** that validates other implementations. It:

- Acts as a protocol peer (can be initiator or acceptor depending on test)
- Sends and receives raw Rapace frames over TCP
- Validates that the implementation under test follows the specification
- Uses the `#[conformance(rules = "...")]` macro to declare which rules each test covers

```bash
# List all test cases
rapace-spec-peer --list

# List with rules covered
rapace-spec-peer --list --show-rules

# Run a specific test
PEER_ADDR=127.0.0.1:9000 rapace-spec-peer --case handshake.valid_hello_exchange

# Filter by category
rapace-spec-peer --list --category handshake
```

Exit codes:
- 0: Test passed
- 1: Test failed (protocol violation)
- 2: Internal error

### spec-subject (Implementation Under Test)

The `spec-subject` binary wraps the real `rapace-core` implementation:

- Uses actual `RpcSession` and `StreamTransport` from rapace-core
- Connects to spec-peer via TCP
- Exercises the implementation as it would be used in production

```bash
PEER_ADDR=127.0.0.1:9000 rapace-spec-subject --case handshake.valid_hello_exchange
```

### Test Categories

Tests are organized by protocol area:

- `handshake.*` - Hello exchange, version negotiation, feature negotiation
- `frame.*` - Frame structure, descriptor encoding, payload handling
- `channel.*` - Channel lifecycle, ID allocation, control messages
- `call.*` - Unary RPC semantics, request/response flags
- `control.*` - Ping/pong, control channel operations
- `flow.*` - Flow control, credit semantics

### Writing New Conformance Tests

Tests live in `spec-peer/src/tests/` and use the conformance macro:

```rust
#[conformance(
    name = "handshake.valid_hello_exchange",
    rules = "handshake.required, handshake.ordering"
)]
pub async fn valid_hello_exchange(peer: &mut Peer) -> TestResult {
    // Receive frame from implementation
    let frame = peer.recv().await?;
    
    // Validate it follows the spec
    if frame.desc.channel_id != 0 {
        return TestResult::fail("expected channel 0");
    }
    
    // Send response if needed
    peer.send(&response_frame).await?;
    
    TestResult::pass()
}
```

The `Peer` struct provides:
- `peer.recv()` / `peer.recv_timeout(Duration)` - Receive frames
- `peer.recv_raw()` - Receive with raw wire bytes preserved
- `peer.send(&frame)` - Send frames
- `peer.try_recv()` - Non-blocking receive (returns None on timeout/EOF)

### Running Conformance Tests

The test harness spawns both binaries and connects them:

```bash
# Run all conformance tests
cargo nextest run -p rapace-spec-peer

# Run specific test
cargo nextest run -p rapace-spec-peer valid_hello_exchange
```
