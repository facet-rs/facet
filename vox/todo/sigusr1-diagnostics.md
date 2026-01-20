# SIGUSR1 Diagnostic Dump for In-Flight RPC State

## Problem

When debugging hung connections, stack traces alone don't reveal:
- Which RPC calls are waiting for responses
- Which streaming channels are open
- How long each has been waiting

## Goal

On SIGUSR1, dump a complete picture of all in-flight RPC state:

```
============================================================
[cell-http] RPC DIAGNOSTIC DUMP
============================================================

IN-FLIGHT REQUESTS (client → server):
  [req_id=93] TcpTunnel::open - waiting 45.2s
  [req_id=94] ContentService::find_content("/guide/debugging") - waiting 12.1s

IN-FLIGHT REQUESTS (server → client):
  (none)

PENDING RESPONSES (waiting for remote):
  [req_id=63] - waiting 78.3s

OPEN CHANNELS:
  [ch=370] Tx<Vec<u8>> - open 45.2s (outgoing, 1.2MB sent)
  [ch=372] Rx<Vec<u8>> - open 45.2s (incoming, 0 bytes received)
  [ch=374] Tx<LogRecord> - open 120.5s (outgoing, 847 records sent)

CHANNEL REGISTRIES:
  server_channel_registry: 2 channels [370, 374]
  client_channel_registry: 1 channels [372]

============================================================
```

## Implementation

### 1. Diagnostic State Tracking

Add to `roam-session`:

```rust
use std::time::Instant;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct InFlightRequest {
    pub request_id: u64,
    pub method_id: u64,
    pub method_name: Option<&'static str>,  // If known from service definition
    pub started: Instant,
    pub direction: RequestDirection,
}

#[derive(Debug, Clone, Copy)]
pub enum RequestDirection {
    Outgoing,  // We sent the request, waiting for response
    Incoming,  // We received the request, processing it
}

#[derive(Debug, Clone)]
pub struct OpenChannel {
    pub channel_id: u64,
    pub started: Instant,
    pub direction: ChannelDirection,
    pub bytes_transferred: u64,  // Approximate
}

#[derive(Debug, Clone, Copy)]
pub enum ChannelDirection {
    Tx,  // We're sending
    Rx,  // We're receiving
}

/// Global diagnostic state, one per connection/driver
pub struct DiagnosticState {
    pub connection_name: String,  // e.g., "cell-http", "host→markdown"
    pub in_flight_requests: HashMap<u64, InFlightRequest>,
    pub open_channels: HashMap<u64, OpenChannel>,
}
```

### 2. Recording Events

In `ConnectionHandle::call()`:
```rust
// Before sending request
diagnostic_state.record_outgoing_request(request_id, method_id, method_name);

// After receiving response
diagnostic_state.complete_request(request_id);
```

In `dispatch_call()`:
```rust
// When request arrives
diagnostic_state.record_incoming_request(request_id, method_id, method_name);

// When response sent
diagnostic_state.complete_request(request_id);
```

In channel binding:
```rust
// When channel opened
diagnostic_state.record_channel_open(channel_id, direction);

// When channel closed
diagnostic_state.record_channel_close(channel_id);
```

### 3. Global Registry

Need a way for the signal handler to find all diagnostic states:

```rust
// In roam-session or a new roam-diagnostic crate
static DIAGNOSTIC_REGISTRY: RwLock<Vec<Weak<RwLock<DiagnosticState>>>> = ...;

pub fn register_diagnostic_state(state: Arc<RwLock<DiagnosticState>>) {
    // Add weak reference to registry
}

pub fn dump_all_diagnostics() -> String {
    // Iterate through registry, dump each state
    // Skip any that have been dropped
}
```

### 4. Integration with dodeca-debug

In the driver setup (ShmDriver, etc.):
```rust
let diagnostic_state = Arc::new(RwLock::new(DiagnosticState::new("cell-http")));
roam_session::register_diagnostic_state(diagnostic_state.clone());

dodeca_debug::register_diagnostic(|| {
    eprintln!("{}", roam_session::dump_all_diagnostics());
});
```

### 5. Method Name Resolution

The `method_id` is a hash. To get human-readable names:

Option A: Store method names in a static registry when services are registered
```rust
// In #[roam::service] macro expansion
roam_session::register_method_name(METHOD_ID_OPEN, "TcpTunnel::open");
```

Option B: Just show the hash, let user correlate with logs
```
[req_id=93] method_id=16216355521356224968 - waiting 45.2s
```

Option C: Include method name in the request tracking (caller knows it)

## Files to Modify

1. `roam-session/src/lib.rs` - Add DiagnosticState, tracking calls
2. `roam-session/src/diagnostic.rs` (new) - Diagnostic types and registry
3. `roam-shm/src/driver.rs` - Register diagnostic state for each connection
4. `roam-macros/src/lib.rs` - Optionally register method names
5. `dodeca/src/cells.rs` or host setup - Register the dump callback

## Testing

1. Unit test: Create diagnostic state, record events, verify dump output
2. Integration test: Start cell, make RPC calls, send SIGUSR1, verify output
3. Manual test: Reproduce hung state, send SIGUSR1, verify useful output

## Future Enhancements

- Add memory usage per channel (for large transfers)
- Add request/response payload size
- Add histogram of request latencies
- Web endpoint to dump state (not just SIGUSR1)
- Structured JSON output for tooling
