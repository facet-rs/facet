# TypeScript Streaming Implementation Handoff

This document describes how to add streaming support to the TypeScript implementation
of roam. The Rust implementation is the reference - this describes how to port it.

## Current State

The TypeScript implementation has:
- `Tx<T>` and `Rx<T>` types (in `tx.ts` and `rx.ts`)
- `StreamRegistry` for tracking registered streams
- `StreamIdAllocator` for allocating channel IDs
- Generated client code that serializes stream IDs to the wire
- Generated handler code with `/* TODO: create real Rx handle */` placeholders

What's missing:
- Actual send/recv implementations that work
- Stream binding during server dispatch
- Drain tasks that pump Data messages to/from the wire
- Proper ordering guarantees (Data/Close before Response)

## Rust Architecture Overview

### The Three Streaming Patterns

```rust
// Pattern 1: Client→Server streaming (sum)
// Client sends numbers, server receives and aggregates
async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
    let mut total = 0;
    while let Some(n) = numbers.recv().await {
        total += n;
    }
    total
}

// Pattern 2: Server→Client streaming (generate)
// Server sends numbers, client receives
async fn generate(&self, count: u32, output: Tx<i32>) {
    for i in 0..count {
        output.send(&i).await;
    }
    // Tx dropped here → sends Close automatically
}

// Pattern 3: Bidirectional (transform)
// Server receives via Rx, sends via Tx
async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
    while let Some(s) = input.recv().await {
        output.send(&s).await;
    }
}
```

### Channel Flow (Client Calling `sum`)

```
Client                                           Server
------                                           ------
1. let (tx, rx) = channel::<i32>()
   // tx: sender, rx: receiver
   // channel_id not yet assigned

2. client.sum(rx)
   // bind_streams() walks args:
   //   - allocates channel_id (e.g., 1)
   //   - takes tx, spawns drain task
   //   - drain task reads tx → sends Data messages
   
3. Serialize rx.channel_id → payload
   Send: Request { method_id, payload: [channel_id] }
                                                  4. Receive Request
                                                     Deserialize channel_id
                                                     
                                                  5. bind_streams() on args:
                                                     - create mpsc channel
                                                     - register incoming(channel_id, tx)
                                                     - give rx to handler
                                                     
                                                  6. Handler calls numbers.recv()
                                                     (blocks waiting for data)

7. User code: tx.send(&42)
   → drain task: Data { channel_id=1, payload }
   → wire: MSG_DATA
                                                  8. Receive MSG_DATA
                                                     route_data(channel_id, payload)
                                                     → sent to handler's rx
                                                     → recv() returns Some(42)

9. drop(tx) or tx.close()
   → drain task: Close { channel_id=1 }
   → wire: MSG_CLOSE
                                                  10. Receive MSG_CLOSE
                                                      close(channel_id)
                                                      → rx.recv() returns None
                                                      → handler loop exits

                                                  11. Handler returns result
                                                      Response sent AFTER all Data/Close
```

### Key Invariant: Message Ordering

All `Data`, `Close`, and `Response` messages go through a single `TaskMessage` channel:

```rust
enum TaskMessage {
    Data { channel_id: ChannelId, payload: Vec<u8> },
    Close { channel_id: ChannelId },
    Response { request_id: RequestId, payload: Vec<u8> },
}
```

The driver polls this channel with highest priority, ensuring:
- All Data messages are sent before Response
- Close is sent before Response (when Tx is dropped)
- Correct ordering for streaming RPC semantics

## TypeScript Implementation Plan

### Phase 1: Core Channel Infrastructure

**1.1 Implement `Channel<T>` (async queue with close)**

```typescript
// A simple async channel with close semantics
class Channel<T> {
  private queue: T[] = [];
  private closed = false;
  private waiters: Array<(value: T | null) => void> = [];
  
  send(value: T): boolean {
    if (this.closed) return false;
    if (this.waiters.length > 0) {
      this.waiters.shift()!(value);
    } else {
      this.queue.push(value);
    }
    return true;
  }
  
  async recv(): Promise<T | null> {
    if (this.queue.length > 0) {
      return this.queue.shift()!;
    }
    if (this.closed) return null;
    return new Promise(resolve => this.waiters.push(resolve));
  }
  
  close(): void {
    this.closed = true;
    for (const waiter of this.waiters) {
      waiter(null);
    }
    this.waiters = [];
  }
}
```

**1.2 Implement real `Tx<T>` and `Rx<T>`**

```typescript
// Tx<T> - for sending values
class Tx<T> {
  readonly streamId: bigint;
  private channel: Channel<Uint8Array>;
  private serialize: (value: T) => Uint8Array;
  private closed = false;
  
  constructor(streamId: bigint, channel: Channel<Uint8Array>, serialize: (T) => Uint8Array) {
    this.streamId = streamId;
    this.channel = channel;
    this.serialize = serialize;
  }
  
  send(value: T): boolean {
    if (this.closed) return false;
    const payload = this.serialize(value);
    return this.channel.send(payload);
  }
  
  close(): void {
    this.closed = true;
    this.channel.close();
  }
}

// Rx<T> - for receiving values  
class Rx<T> implements AsyncIterable<T> {
  readonly streamId: bigint;
  private channel: Channel<Uint8Array>;
  private deserialize: (bytes: Uint8Array) => T;
  
  constructor(streamId: bigint, channel: Channel<Uint8Array>, deserialize: (Uint8Array) => T) {
    this.streamId = streamId;
    this.channel = channel;
    this.deserialize = deserialize;
  }
  
  async recv(): Promise<T | null> {
    const bytes = await this.channel.recv();
    if (bytes === null) return null;
    return this.deserialize(bytes);
  }
  
  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    while (true) {
      const value = await this.recv();
      if (value === null) break;
      yield value;
    }
  }
}
```

**1.3 Channel pair creation**

```typescript
function createChannel<T>(
  serialize: (value: T) => Uint8Array,
  deserialize: (bytes: Uint8Array) => T,
): [Tx<T>, Rx<T>] {
  const channel = new Channel<Uint8Array>();
  // streamId=0 initially, set during bind
  const tx = new Tx(0n, channel, serialize);
  const rx = new Rx(0n, channel, deserialize);
  return [tx, rx];
}
```

### Phase 2: TaskMessage Channel

**2.1 Add TaskMessage type**

```typescript
type TaskMessage =
  | { kind: 'data'; channelId: bigint; payload: Uint8Array }
  | { kind: 'close'; channelId: bigint }
  | { kind: 'response'; requestId: bigint; payload: Uint8Array };
```

**2.2 Add to Connection**

```typescript
class Connection {
  private taskChannel: Channel<TaskMessage> = new Channel();
  
  // Called by drain tasks and handlers
  sendTask(msg: TaskMessage): void {
    this.taskChannel.send(msg);
  }
  
  // In the message loop, poll taskChannel with priority
  async run(dispatcher: ServiceDispatcher): Promise<void> {
    while (true) {
      // Check for task messages first (priority)
      const task = await this.pollTaskChannel();
      if (task) {
        await this.handleTaskMessage(task);
        continue;
      }
      
      // Then check for incoming wire messages
      const msg = await this.io.recv();
      // ... handle msg
    }
  }
  
  private async handleTaskMessage(msg: TaskMessage): Promise<void> {
    switch (msg.kind) {
      case 'data':
        await this.io.send(encodeData(msg.channelId, msg.payload));
        break;
      case 'close':
        await this.io.send(encodeClose(msg.channelId));
        break;
      case 'response':
        await this.io.send(encodeResponse(msg.requestId, msg.payload));
        break;
    }
  }
}
```

### Phase 3: Stream Binding in Dispatch

**3.1 For handler dispatch (server side)**

The generated code currently has:
```typescript
const _numbers_r = decodeU64(buf, offset);
const numbers = { streamId: _numbers_r.value } as Rx<number>;
// TODO: create real Rx handle
```

Replace with:
```typescript
const _numbers_r = decodeU64(buf, offset);
const channelId = _numbers_r.value;
// Create a real channel for incoming data
const channel = new Channel<Uint8Array>();
// Register so Data messages get routed here
registry.registerIncoming(channelId, channel);
// Create Rx with working recv()
const numbers = new Rx(channelId, channel, decodeI32);
```

**3.2 For Tx arguments (server sends to client)**

```typescript
const _output_r = decodeU64(buf, offset);
const channelId = _output_r.value;
// Create channel for outgoing data
const channel = new Channel<Uint8Array>();
// Spawn drain task
spawnDrainTask(channelId, channel, connection.sendTask.bind(connection));
// Create Tx with working send()
const output = new Tx(channelId, channel, encodeI32);
```

**3.3 Drain task implementation**

```typescript
async function spawnDrainTask(
  channelId: bigint,
  channel: Channel<Uint8Array>,
  sendTask: (msg: TaskMessage) => void,
): Promise<void> {
  // Run in background (don't await)
  (async () => {
    while (true) {
      const payload = await channel.recv();
      if (payload === null) {
        // Channel closed, send Close message
        sendTask({ kind: 'close', channelId });
        break;
      }
      sendTask({ kind: 'data', channelId, payload });
    }
  })();
}
```

### Phase 4: Client-Side Binding

When the client calls a streaming method:

```typescript
async sum(numbers: Rx<number>): Promise<bigint> {
  // Before: just encoded the streamId
  // After: need to bind the channel
  
  // Allocate stream ID
  const channelId = this.conn.allocateStreamId();
  
  // Get the paired Tx from the Rx (they share a channel)
  // and spawn drain task for it
  const tx = numbers._pairedTx; // internal field
  tx._streamId = channelId;
  spawnDrainTask(channelId, tx._channel, this.conn.sendTask.bind(this.conn));
  
  // Register for Close messages from server
  this.conn.registerIncoming(channelId, ...);
  
  // Now serialize and call
  const payload = encodeU64(channelId);
  const response = await this.conn.call(METHOD_ID.sum, payload);
  // ...
}
```

### Phase 5: Update Generated Code

The codegen needs to be updated to generate proper binding code instead of TODOs.
This affects:
- `rust/roam-codegen/src/targets/typescript.rs`
- The `generate_decode_stmt_server` and related functions

## Testing

Once implemented, these tests should pass:
- `streaming_sum_client_to_server` - client sends numbers, server sums
- `streaming_generate_server_to_client` - server sends numbers, client receives  
- `streaming_transform_bidirectional` - echo through server

Run with:
```bash
just ts
```

## Files to Modify

1. `typescript/packages/roam-core/src/streaming/tx.ts` - Real Tx implementation
2. `typescript/packages/roam-core/src/streaming/rx.ts` - Real Rx implementation
3. `typescript/packages/roam-core/src/streaming/channel.ts` - Add Channel class
4. `typescript/packages/roam-core/src/connection.ts` - Add TaskMessage handling
5. `rust/roam-codegen/src/targets/typescript.rs` - Generate binding code
6. `typescript/subject/subject.ts` - Implement streaming handlers

## Reference Files

Look at these Rust files for reference:
- `rust/roam-session/src/lib.rs` - Tx, Rx, ChannelRegistry, dispatch_call
- `rust/subject-rust/src/main.rs` - Example streaming handler implementations
- `rust/roam-stream/src/driver.rs` - Driver loop, TaskMessage handling
