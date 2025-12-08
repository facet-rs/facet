# Plugin Host Example

## Overview

`plugin_host.rs` demonstrates how to use rapace as a plugin architecture host, managing multiple plugin processes via shared memory IPC.

## Architecture

### Host Responsibilities
- **Session Management**: Creates separate SHM segments for each plugin
- **Lifecycle Control**: Tracks plugin states (Connected, Initializing, Running, ShuttingDown, Crashed)
- **Service Exposure**: Provides host services that plugins can call
- **Liveness Monitoring**: Uses heartbeat mechanism to detect crashed plugins
- **Graceful Shutdown**: Sends shutdown requests and waits for acknowledgment

### Plugin Sessions

Each plugin gets its own dedicated:
- Shared memory segment (4MB default)
- Bidirectional ring buffers (64 descriptors each)
- Slot-based data segment (16 slots × 4KB)
- Session with PeerA role (host is always PeerA, plugin is PeerB)

### Communication Patterns

1. **Control Channel (channel 0)**: Session-level control messages
   - Plugin initialization
   - Shutdown coordination
   - Flow control

2. **Data Channels (channel 1+)**: Application-level RPC
   - Plugin processing requests
   - Host service calls
   - Event streams

## Host Services

### HostLogger
```rust
// Method ID: 1000
HostLogger::log(LogRequest { level, message })
```
Plugins can log messages through the host for centralized logging.

### HostStorage
```rust
// Method ID: 1001
HostStorage::get(GetRequest { key }) -> GetResponse { value: Option<Vec<u8>> }

// Method ID: 1002
HostStorage::set(SetRequest { key, value })
```
Key-value storage shared across all plugins, managed by the host.

### HostEvents (Future)
```rust
// Method ID: 1003
HostEvents::subscribe() -> stream<HostEvent>
```
Bidirectional event stream for host-wide notifications.

## Plugin Interface

### Plugin::initialize
```rust
// Method ID: 2000
Plugin::initialize(PluginConfig { plugin_id, config_data })
```
Called when plugin first connects. Plugin should set up its state and respond when ready.

### Plugin::process
```rust
// Method ID: 2001
Plugin::process(ProcessRequest { request_id, operation, data })
    -> ProcessResponse { request_id, success, result }
```
Main work method. Plugins receive requests and return responses.

**Nested Calls**: During processing, plugins can call back into host services (e.g., HostStorage::get).

### Plugin::shutdown
```rust
// Method ID: 2002
Plugin::shutdown()
```
Graceful shutdown request. Plugin should clean up and acknowledge.

## Example Flow

1. **Host Startup**
   ```
   Host creates PluginHost
   Builds service registry (for introspection)
   ```

2. **Plugin Loading**
   ```
   Host creates SHM segment for "analytics" plugin
   Host initializes Session<PeerA>
   Host sends Plugin::initialize(config)
   Plugin responds (state -> Running)
   ```

3. **Processing with Nested Calls**
   ```
   Host sends Plugin::process(request)
   Plugin receives request
   Plugin calls HostLogger::log("Processing started")
   Plugin calls HostStorage::get("cache_key")
   Host responds with stored value
   Plugin completes processing
   Plugin sends ProcessResponse back to host
   ```

4. **Graceful Shutdown**
   ```
   Host sends Plugin::shutdown()
   Host polls for response
   Plugin cleans up and acknowledges
   Host unloads plugin session
   ```

## Liveness & Crash Detection

- Host calls `session.heartbeat()` every poll
- Host checks `session.is_peer_alive()` to detect crashes
- If plugin hasn't updated its timestamp within 1 second -> marked as Crashed
- Host can restart crashed plugins or alert operators

## Message Encoding

- Uses **postcard** for compact binary serialization
- Inline payloads (≤24 bytes): embedded directly in descriptor
- Larger payloads: allocated in slot-based data segment
- Automatic slot lifecycle management (alloc → commit → free)

## Error Handling

- **Ring Full**: Back-pressure if plugin can't keep up with messages
- **Slot Exhausted**: No available memory slots (need to wait for freed slots)
- **Validation Errors**: Malformed frames are logged and dropped
- **Peer Died**: Detected via heartbeat timeout → plugin marked as Crashed

## Service Registry

The host builds a registry describing all available services and methods:

```rust
Service: rapace.HostServices v1.0
  - Logger.log (1000): Unary
  - Storage.get (1001): Unary
  - Storage.set (1002): Unary
  - Events.subscribe (1003): ServerStreaming

Service: rapace.Plugin v1.0
  - initialize (2000): Unary
  - process (2001): Unary
  - shutdown (2002): Unary
```

This registry can be serialized and shared with plugins for introspection.

## Demonstration

The example demonstrates:

1. **Multiple Sessions**: Two plugins ("analytics" and "transform") running simultaneously
2. **Configuration**: Each plugin receives custom config during initialization
3. **Bidirectional Communication**: Host → Plugin (process requests) and Plugin → Host (service calls)
4. **State Tracking**: Plugin lifecycle states are monitored and logged
5. **Graceful Teardown**: Plugins are shut down cleanly, not just killed

## Running the Example

```bash
# Note: This example currently requires the rapace library to compile cleanly
# There is work-in-progress code (rpc module) that needs to be completed first

cargo build --example plugin_host
cargo run --example plugin_host
```

Expected output:
```
=== Rapace Plugin Host Example ===

Service registry: 432 bytes

=== Loading plugin: analytics ===
[Host] Creating session for plugin: analytics
[Host] Initializing plugin: analytics
...
[Host/Logger] [analytics] [Info] Processing started
[Host/Storage] Plugin analytics GET cache_key: None
[Host] Plugin analytics process response: req_id=1, success=true, 8 bytes
...
=== Shutting down ===
[Host] Shutting down plugin: analytics
```

## Architecture Notes

### Why Separate SHM Segments Per Plugin?

- **Isolation**: Plugin crash doesn't corrupt other plugins' memory
- **Security**: Plugins can't read/write each other's data
- **Resource Management**: Per-plugin quotas and limits
- **Simplicity**: No need for complex multiplexing at the segment level

### Why PeerA for Host?

- PeerA creates the segment → host controls initialization
- Consistent role: host is always PeerA, plugins always PeerB
- Clear ownership: host owns segment lifecycle

### Future Extensions

- **Dynamic plugin discovery**: Scan directory for plugin binaries
- **Facet integration**: Use facet for zero-copy serialization
- **Channel multiplexing**: Multiple concurrent RPC calls per plugin
- **Event subscriptions**: Plugins subscribe to host events via bidirectional streams
- **Hot reload**: Restart crashed plugins automatically
- **Metrics collection**: Track per-plugin CPU, memory, message rates

## Related Files

- `/Users/amos/bearcove/rapace/examples/echo_server.rs`: Simpler server example
- `/Users/amos/bearcove/rapace/examples/echo_client.rs`: Simpler client example
- `/Users/amos/bearcove/rapace/src/session.rs`: Session API with role-based types
- `/Users/amos/bearcove/rapace/src/channel.rs`: Channel abstraction with typestate
- `/Users/amos/bearcove/rapace/src/registry.rs`: Service registry for introspection
