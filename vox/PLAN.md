# Plan: Unbound channels with schema-driven binding for TypeScript client

## Goal
Match Rust's channel semantics:
1. `channel<T>()` standalone function returns unbound `[Tx<T>, Rx<T>]` pair
2. Tx/Rx are bound (allocated channel IDs) at call time via schema-driven runtime walker
3. Consume semantics: once bound, Tx/Rx cannot be reused
4. Rename "stream" → "channel" throughout

## Current State
- 14/17 tests pass (all server-mode tests)
- 3 client mode tests timeout because client-side streaming isn't implemented
- Current `createTx()`/`createRx()` on Connection allocate IDs upfront (wrong model)
- Terminology: code says "stream" but spec says "channel"

## Architecture

### Unbound Channels
```typescript
// Standalone function, not on Connection
function channel<T>(): [Tx<T>, Rx<T>] {
  // Returns paired Tx/Rx with no channelId yet
  // They share internal state so binding one binds both
}

const [tx, rx] = channel<number>();
// tx.channelId === undefined
// rx.channelId === undefined
// tx._pair === rx, rx._pair === tx (internal linkage)
```

### Async Send with Zero Buffer
`tx.send()` is async and blocks until receiver is ready (backpressure):

```typescript
const [tx, rx] = channel<number>();

// WRONG - blocks forever, no receiver yet:
await tx.send(1);
client.sum(rx);

// CORRECT - start call first, then send:
const resultPromise = client.sum(rx);  // Binds channels, sets up receiver
await tx.send(1);  // Now there's a receiver
await tx.send(2);
tx.close();
const result = await resultPromise;
```

### Schema-Driven Binding
Codegen emits a schema describing argument structure:
```typescript
// Generated schema for sum(numbers: Rx<i32>) -> i64
const sumSchema = {
  args: [{ kind: 'rx', element: 'i32' }],
  returns: { kind: 'i64' }
};

// Generated schema for complex nested types
const complexSchema = {
  args: [{
    kind: 'vec',
    element: {
      kind: 'enum',
      variants: {
        'Text': [{ kind: 'string' }],
        'Data': [{ kind: 'tx', element: 'bytes' }],
      }
    }
  }]
};
```

Runtime binder walks args using schema, finds Tx/Rx, allocates IDs, binds them:
```typescript
// In generated client method:
async sum(numbers: Rx<number>): Promise<bigint> {
  // Runtime binding via schema
  this.conn.bindChannels(sumSchema.args, [numbers]);
  // Now numbers.channelId is set
  
  const payload = encodeU64(numbers.channelId);
  const response = await this.conn.call(METHOD_ID.sum, payload);
  // ...
}
```

### Consume Semantics
Once bound, Tx/Rx are "consumed" - passing to another method throws:
```typescript
class Tx<T> {
  private _channelId: bigint | undefined;
  private _consumed = false;
  
  bind(channelId: bigint, registry: ChannelRegistry): void {
    if (this._consumed) throw new Error("Tx already consumed");
    this._channelId = channelId;
    this._consumed = true;
    // Register with registry for outgoing data
    this._sender = registry.registerOutgoing(channelId);
    // Also bind the paired Rx
    this._pair?.bind(channelId, registry);
  }
  
  async send(value: T): Promise<void> {
    if (!this._channelId) throw new Error("Tx not bound - call the method first");
    // Blocks until receiver ready (zero buffer, backpressure)
  }
}
```

## Phases

Each phase is a discrete, testable unit of work.

---

### Phase 1: Rename stream → channel

**Goal:** Terminology matches the spec.

**Files:**
- `typescript/packages/roam-core/src/streaming/*.ts`
- `typescript/packages/roam-core/src/connection.ts`
- `rust/roam-codegen/src/targets/typescript.rs`

**Changes:**
- `StreamId` → `ChannelId`
- `streamId` → `channelId`
- `StreamRegistry` → `ChannelRegistry`
- `StreamIdAllocator` → `ChannelIdAllocator`
- Update all references in codegen

**Test:** All 14 server-mode tests still pass.

---

### Phase 2: Refactor Tx/Rx for unbound state

**Goal:** Tx/Rx can exist without a channel ID, support binding later.

**Files:**
- `typescript/packages/roam-core/src/streaming/tx.ts`
- `typescript/packages/roam-core/src/streaming/rx.ts`

**Changes:**
- `_channelId` starts as `undefined`
- Add `bind(channelId, registry)` method
- Add `_pair` reference linking Tx↔Rx
- `send()` becomes async, throws if not bound
- `recv()` throws if not bound
- Add `_consumed` flag, throw if already bound

**Test:** Existing server-side usage still works (server-side Tx/Rx created via `createServerTx`/`createServerRx` still work).

---

### Phase 3: Add `channel<T>()` function

**Goal:** Standalone function to create paired unbound Tx/Rx.

**Files:**
- `typescript/packages/roam-core/src/streaming/index.ts` (or new file)

**Changes:**
```typescript
export function channel<T>(): [Tx<T>, Rx<T>] {
  const tx = new Tx<T>();
  const rx = new Rx<T>();
  tx._pair = rx;
  rx._pair = tx;
  return [tx, rx];
}
```

**Test:** Can create channel, both sides are unbound, linked together.

---

### Phase 4: Add runtime channel binder

**Goal:** Walk argument structure via schema, find and bind Tx/Rx.

**Files:**
- `typescript/packages/roam-core/src/binding.ts` (new)

**Changes:**
```typescript
export function bindChannels(
  schema: ArgSchema[],
  args: unknown[],
  allocator: ChannelIdAllocator,
  registry: ChannelRegistry,
): void

function bindValue(schema: Schema, value: unknown, ...): void
  // Handles: tx, rx, vec, struct, enum, primitives
```

**Test:** Unit test that binding works for simple and nested types.

---

### Phase 5: Codegen schemas

**Goal:** Generate type schemas for each method's arguments.

**Files:**
- `rust/roam-codegen/src/targets/typescript.rs`

**Changes:**
```typescript
// Generated
export const testbed_schemas = {
  echo: { args: [{ kind: 'string' }] },
  sum: { args: [{ kind: 'rx', element: 'i32' }] },
  generate: { args: [{ kind: 'u32' }, { kind: 'tx', element: 'i32' }] },
  // ...
};
```

**Test:** Codegen produces valid schemas, TypeScript compiles.

---

### Phase 6: Update generated client methods to use binding

**Goal:** Client methods bind channels before encoding/calling.

**Files:**
- `rust/roam-codegen/src/targets/typescript.rs`

**Changes:**
```typescript
async sum(numbers: Rx<number>): Promise<bigint> {
  bindChannels(testbed_schemas.sum.args, [numbers], this.conn.allocator, this.conn.registry);
  const payload = encodeU64(numbers.channelId);
  // ...
}
```

**Test:** Generated client code compiles.

---

### Phase 7: Implement subject.ts client mode

**Goal:** Client mode tests pass.

**Files:**
- `typescript/subject/subject.ts`

**Changes:**
```typescript
case "sum": {
  const [tx, rx] = channel<number>();
  const resultPromise = client.sum(rx);
  
  for (let i = 1; i <= 5; i++) {
    await tx.send(i);
  }
  tx.close();
  
  const result = await resultPromise;
  break;
}

case "generate": {
  const [tx, rx] = channel<number>();
  
  const recvTask = (async () => {
    const received = [];
    for await (const n of rx) received.push(n);
    return received;
  })();
  
  await client.generate(5, tx);
  const received = await recvTask;
  break;
}
```

**Test:** All 17 tests pass.

---

## Test Expectations

After all phases:
- All 17 tests pass
- Client mode works with proper channel semantics
- Reusing a bound Tx/Rx throws clear error
- `tx.send()` before binding throws clear error
- Terminology matches spec (channels, not streams)
