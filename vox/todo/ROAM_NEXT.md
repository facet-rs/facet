# roam-next: Minimize Monomorphization in Dispatch

## Goal

Reduce code bloat by making dispatch helpers non-generic. Currently, `dispatch_call<A, R, E, F, Fut>()`
gets monomorphized for every RPC method. With 50 methods, that's 50 copies of the same deserialization,
middleware, and serialization logic.

**New approach:** Generated code knows types and calls non-generic helpers that work via Shape + pointer.

## Current State

```
rust/roam-next/           # Prototype with prepare() API
rust/roam-session/        # Production code with generic dispatch_call()
```

The prototype proves the concept works. Now we integrate it into roam-session.

## Design

### Non-Generic Helpers

```rust
// Deserialize into pointer (SYNC, non-generic)
pub unsafe fn deserialize_into(
    ptr: *mut (),
    shape: &'static Shape,
    payload: &[u8],
) -> Result<(), PrepareError>;

// Patch channel IDs (SYNC, non-generic)
pub unsafe fn patch_channel_ids_by_shape(
    args_ptr: *mut (),
    args_shape: &'static Shape,
    channels: &[u64],
);

// Run middleware (ASYNC, takes SendPeek which is Send-safe)
pub async fn run_middleware(
    send_peek: SendPeek<'_>,
    ctx: &mut Context,
    middleware: &[Arc<dyn Middleware>],
) -> Result<(), Rejection>;

// Serialize and send OK response (ASYNC, takes SendPeek)
pub async fn send_ok_response(
    result: SendPeek<'_>,
    driver_tx: &Sender<DriverMessage>,
    conn_id: ConnectionId,
    request_id: u64,
);

// Serialize and send error response (ASYNC, takes SendPeek)
pub async fn send_error_response(
    error: SendPeek<'_>,
    driver_tx: &Sender<DriverMessage>,
    conn_id: ConnectionId,
    request_id: u64,
);
```

**Key insight:** Async functions take `SendPeek` (which is `Send+Sync`) instead of raw
pointers (which are not `Send`). This allows the Future's state to be `Send`.

### Middleware Trait (Peek-based)

```rust
pub trait Middleware: Send + Sync {
    fn intercept<'a>(
        &'a self,
        ctx: &'a mut Context,
        args: Peek<'_, 'static>,  // Can inspect args via reflection!
    ) -> Pin<Box<dyn Future<Output = Result<(), Rejection>> + Send + 'a>>;
}
```

### Generated Dispatcher

```rust
pub struct TestbedDispatcher<H> {
    handler: H,
    middleware: Vec<Arc<dyn Middleware>>,
}

impl<H> TestbedDispatcher<H> {
    pub fn new(handler: H) -> Self {
        Self { handler, middleware: Vec::new() }
    }

    pub fn with_middleware<M: Middleware + 'static>(mut self, mw: M) -> Self {
        self.middleware.push(Arc::new(mw));
        self
    }
}
```

### Generated dispatch_* Methods

```rust
fn dispatch_echo(&self, cx: Context, payload: Vec<u8>, registry: &mut ChannelRegistry)
    -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
{
    let handler = self.handler.clone();
    let middleware = self.middleware.clone();
    let driver_tx = registry.driver_tx();
    let dispatch_ctx = registry.dispatch_context();
    let channels = cx.channels.clone();
    let conn_id = cx.conn_id;
    let request_id = cx.request_id.raw();

    // === SYNC PHASE (before async block) ===
    let mut args_slot = MaybeUninit::<(String,)>::uninit();

    // Deserialize (non-generic via Shape)
    if let Err(e) = unsafe {
        deserialize_into(args_slot.as_mut_ptr().cast(), <(String,)>::SHAPE, &payload)
    } {
        return Box::pin(async move { send_prepare_error(e, &driver_tx, conn_id, request_id).await });
    }

    // Patch channel IDs (non-generic via Shape)
    unsafe { patch_channel_ids_by_shape(args_slot.as_mut_ptr().cast(), <(String,)>::SHAPE, &channels) };

    // Bind streams (non-generic via Shape) - MUST be sync, needs registry
    unsafe { registry.bind_streams_by_shape(args_slot.as_mut_ptr().cast(), <(String,)>::SHAPE) };

    // Read args - moves ownership to async block
    let args: (String,) = unsafe { args_slot.assume_init_read() };

    // === ASYNC PHASE ===
    Box::pin(DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
        let mut cx = cx;

        // Run middleware (takes SendPeek, which is Send-safe)
        if !middleware.is_empty() {
            let send_peek = unsafe { SendPeek::new(Peek::unchecked_new(...)) };
            if let Err(rejection) = run_middleware(send_peek, &mut cx, &middleware).await {
                send_prepare_error(PrepareError::Rejected(rejection), &driver_tx, conn_id, request_id).await;
                return;
            }
        }

        // Destructure and call handler (monomorphized - unavoidable)
        let (message,) = args;
        let result = handler.echo(&cx, message).await;

        // Send response (takes SendPeek, non-generic)
        match &result {
            Ok(value) => {
                let send_peek = unsafe { SendPeek::new(Peek::unchecked_new(...)) };
                send_ok_response(send_peek, &driver_tx, conn_id, request_id).await;
            }
            Err(error) => {
                let send_peek = unsafe { SendPeek::new(Peek::unchecked_new(...)) };
                send_error_response(send_peek, &driver_tx, conn_id, request_id).await;
            }
        }
    }))
}
```

### User Ergonomics

```rust
// Define handler
struct MyHandler;
impl Testbed for MyHandler {
    async fn echo(&self, cx: &Context, message: String) -> String {
        message
    }
}

// Create dispatcher with middleware
let dispatcher = TestbedDispatcher::new(MyHandler)
    .with_middleware(AuthMiddleware::new())
    .with_middleware(RateLimiter::new());

// Use as before
let client = connect(connector, config, dispatcher);
```

## Implementation Plan

### Phase 1: Update roam-session dispatch infrastructure

- [x] **1.1** Update `Middleware` trait to take `args: SendPeek` (Peek wrapper with unsafe Send/Sync)
- [x] **1.2** Add `prepare()` function (non-generic deserialize + middleware)
- [x] **1.3** Add `send_ok_response()` function (non-generic serialize + send)
- [x] **1.4** Add `send_error_response()` function (non-generic serialize + send)
- [x] **1.5** Keep old `dispatch_call` temporarily for compatibility (already exists)

### Phase 2: Update macro codegen (roam-macros)

- [x] **2.1** Add `middleware: Vec<Arc<dyn Middleware>>` field to generated dispatcher
- [x] **2.2** Add `with_middleware()` builder method
- [x] **2.3** Update generated `dispatch_*` methods to use new pattern:
  - Allocate `MaybeUninit` for args
  - Deserialize via `deserialize_into()` with Shape
  - Patch channel IDs via `patch_channel_ids_by_shape()`
  - Bind streams via `bind_streams_by_shape()`
  - Read args and call handler
  - Create `SendPeek` and call `send_*_response()` (SendPeek is Send-safe)
- [x] **2.4** Handle channel ID patching via Poke (non-generic) - `patch_channel_ids_by_shape()`
- [x] **2.5** Handle stream binding via Poke (non-generic) - `bind_streams_by_shape()`

### Phase 3: Cleanup

- [ ] **3.1** Remove old generic `dispatch_call` / `dispatch_call_infallible` - KEPT: still used by test dispatchers
- [x] **3.2** Remove `WithMiddleware` wrapper (superseded) - was never implemented, only TODO comment removed
- [x] **3.3** Delete `roam-next` crate (concepts moved to roam-session)
- [x] **3.4** Update any tests - all 329 tests pass

### Phase 4: Future work (not this PR)

- [ ] Client-side middleware (intercept outgoing calls)
- [ ] Middleware that can modify args (Poke, not just Peek)

## Open Questions (Resolved)

1. **Channel ID patching** - ✅ Solved via `patch_channel_ids_by_shape()` using Poke (non-generic).

2. **Stream binding** - ✅ Solved via `bind_streams_by_shape()` in ChannelRegistry (non-generic).

3. **Response channel collection** - ✅ Already non-generic via `collect_channel_ids_from_peek()`.

4. **Send safety for async functions** - ✅ Solved by using `SendPeek` (Send+Sync wrapper around Peek)
   instead of raw pointers. Async functions take SendPeek, generated code creates SendPeek before
   calling async functions.

## Files to Modify

```
rust/roam-session/src/
  dispatch.rs      # Add prepare(), send_*_response(), update Middleware trait
  middleware.rs    # Update Middleware trait, remove WithMiddleware
  lib.rs           # Re-exports

rust/roam-macros/src/
  lib.rs           # Update codegen for new dispatch pattern

rust/roam-next/    # Eventually delete or merge
```

## Success Criteria

1. All existing tests pass
2. Middleware can peek at deserialized args
3. `cargo llvm-lines` shows reduced monomorphization
4. No regression in runtime performance
