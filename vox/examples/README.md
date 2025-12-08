# Rapace Examples

This directory contains example programs demonstrating how to use the rapace shared-memory RPC library.

## Echo Server/Client

The echo server and client demonstrate the basic usage of rapace for bidirectional communication using shared memory.

### Running the Echo Examples

For a simple single-process demo, just run the client (which creates its own segment):

```bash
cargo run --example echo_client
```

For a multi-process setup (which demonstrates the actual shared-memory IPC):

1. In one terminal, start the server:
```bash
cargo run --example echo_server
```

Note the file descriptor (FD) that the server prints.

2. In another terminal, run the client with that FD:
```bash
cargo run --example echo_client -- <FD>
```

Replace `<FD>` with the actual file descriptor number from the server output.

**Note:** The multi-process setup works on macOS and Linux. The FD passing mechanism shown here is simplified for demonstration - in a production application, you would pass the FD via Unix domain sockets using `sendmsg`/`recvmsg` with `SCM_RIGHTS`.

### What They Demonstrate

**echo_server.rs** shows:
- Creating a shared memory segment (PeerA role)
- Initializing the segment layout (header, rings, slots)
- Setting up a session with outbound/inbound rings
- Receiving messages from the client
- Validating message descriptors
- Handling both inline and slot-based payloads
- Echoing messages back to the client
- Heartbeat management and peer liveness checking

**echo_client.rs** shows:
- Connecting to an existing shared memory segment (PeerB role)
- Setting up a session with the opposite ring configuration
- Sending messages with different payload sizes
- Using inline payloads for small messages (<= 24 bytes)
- Using slot-based payloads for larger messages
- Receiving and validating echo responses
- Proper slot allocation and deallocation

### Key Concepts Illustrated

1. **Session Roles**: PeerA creates the segment, PeerB connects to it. Roles are compile-time checked.

2. **Ring Buffers**: Bidirectional communication uses two rings:
   - A→B ring: Server sends to client
   - B→A ring: Client sends to server

3. **Payload Strategies**:
   - Inline: Messages ≤ 24 bytes are embedded in the descriptor
   - Slot-based: Larger messages use shared memory slots

4. **Slot Management**:
   - `alloc()` → write data → `commit()` → send descriptor
   - Receive descriptor → read data → `into_inbound_payload()` to free

5. **Liveness**: Both peers update heartbeats and check if the other is alive.

### Architecture

```
┌─────────────┐                         ┌─────────────┐
│   Server    │                         │   Client    │
│   (PeerA)   │                         │   (PeerB)   │
└──────┬──────┘                         └──────┬──────┘
       │                                       │
       │    ┌───────────────────────────┐     │
       ├────┤  Shared Memory Segment    ├─────┤
       │    │                           │     │
       │    │  ┌─────────────────────┐ │     │
       │    │  │  SegmentHeader      │ │     │
       │    │  └─────────────────────┘ │     │
       │    │  ┌─────────────────────┐ │     │
       │    │  │  A→B Ring (server   │ │     │
       │    │  │  writes, client     │ │     │
       │    │  │  reads)             │ │     │
       │    │  └─────────────────────┘ │     │
       │    │  ┌─────────────────────┐ │     │
       │    │  │  B→A Ring (client   │ │     │
       │    │  │  writes, server     │ │     │
       │    │  │  reads)             │ │     │
       │    │  └─────────────────────┘ │     │
       │    │  ┌─────────────────────┐ │     │
       │    │  │  Slot Metadata      │ │     │
       │    │  └─────────────────────┘ │     │
       │    │  ┌─────────────────────┐ │     │
       │    │  │  Slot Data (shared  │ │     │
       │    │  │  payload buffers)   │ │     │
       │    │  └─────────────────────┘ │     │
       │    └───────────────────────────┘     │
       │                                       │
       └───────────────────────────────────────┘
```

### Implementation Notes

These examples use a simplified, synchronous approach for clarity:

- No async runtime (uses polling loops with `std::thread::sleep`)
- Lifetimes are managed with `Box::leak` for simplicity
- Single-threaded message handling

In a production application, you would typically:

- Use an async runtime (tokio, etc.)
- Integrate with event notification (eventfd, kqueue, etc.)
- Handle lifetimes more carefully
- Implement proper shutdown and resource cleanup
- Add error recovery and reconnection logic
