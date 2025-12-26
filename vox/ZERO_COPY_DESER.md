# Zero-Copy Deserialization with `ManuallyDrop` and Facet Variance Checking

> Design exploration for [#47](https://github.com/bearcove/rapace/issues/47)

## Background

When rapace receives a frame and deserializes it with `facet_format_postcard::from_slice()`,
we copy data even for large payloads. The deserialized value owns its data:

```rust
let frame = transport.recv_frame().await?;
let request: MyRequest = facet_format_postcard::from_slice(frame.payload_bytes())?;
// request owns copies of all strings/bytes from the frame
```

## The Opportunity

`facet_format_postcard` already supports zero-copy deserialization via the `'de` lifetime:

```rust
pub fn from_slice<'de, T>(input: &'de [u8]) -> Result<T, DeserializeError>
where
    T: facet_core::Facet<'de>,
```

Types can borrow directly from the input buffer:

```rust
#[derive(Facet)]
pub struct MyRequest<'a> {
    pub name: Cow<'a, str>,      // borrows from frame payload
    pub data: &'a [u8],          // borrows from frame payload
    pub count: u32,              // copied (small, no benefit)
}
```

## The Problem

If the deserialized value borrows from the frame, we can't move it independently:

```rust
let frame = transport.recv_frame().await?;
let request: MyRequest<'_> = facet_format_postcard::from_slice(frame.payload_bytes())?;
// request borrows from frame - can't return it, can't outlive frame!
```

## Solution: `OwnedMessage<T>` with Facet Variance Checking

We implement a generic `OwnedMessage<T>` struct that:

1. Boxes the frame for stable memory address
2. Creates a `'static` slice pointing to the frame's payload (the "lifetime lie")
3. Uses `ManuallyDrop` to ensure value is dropped before frame
4. **Validates covariance at runtime using facet's variance tracking**

```rust
pub struct OwnedMessage<T: 'static> {
    value: ManuallyDrop<T>,
    frame: ManuallyDrop<Box<Frame>>,
}

impl<T: 'static + Facet<'static>> OwnedMessage<T> {
    pub fn try_new<E>(
        frame: Frame,
        builder: impl FnOnce(&'static [u8]) -> Result<T, E>,
    ) -> Result<Self, E> {
        // Runtime covariance check via facet
        let variance = (T::SHAPE.variance)(T::SHAPE);
        assert!(variance.can_shrink(), "T must be covariant");
        // ... construct with fake 'static slice
    }
}
```

### Why this approach?

| Aspect | yoke | self_cell | Our approach |
|--------|------|-----------|--------------|
| Macro type | proc-macro | macro_rules! | **None** |
| User type changes | `#[derive(Yokeable)]` | None | **None** |
| Variance check | Compile-time (trait) | User asserts | **Runtime (facet)** |
| Generic over T | Yes (with trait) | No | **Yes** |
| Dependencies | yoke crate | self_cell crate | **Just facet** |

**Key advantage**: No macros, no special derives. Just use `Cow<'a, str>` or `&'a [u8]`
in your facet types and it works. Facet already tracks variance, so we leverage that.

## Design

### Core Type

```rust
// In rapace-core/src/owned_message.rs

use std::mem::ManuallyDrop;
use crate::Frame;

/// A deserialized value co-located with its backing Frame.
pub struct OwnedMessage<T: 'static> {
    // SAFETY: value MUST be dropped before frame!
    value: ManuallyDrop<T>,
    frame: ManuallyDrop<Box<Frame>>,
}

impl<T: 'static> Drop for OwnedMessage<T> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.value);  // value first
            ManuallyDrop::drop(&mut self.frame);  // frame second
        }
    }
}

impl<T: 'static + facet::Facet<'static>> OwnedMessage<T> {
    pub fn try_new<E>(
        frame: Frame,
        builder: impl FnOnce(&'static [u8]) -> Result<T, E>,
    ) -> Result<Self, E> {
        // Runtime covariance check
        let variance = (T::SHAPE.variance)(T::SHAPE);
        assert!(variance.can_shrink(), "T must be covariant");

        let frame = Box::new(frame);
        let payload: &'static [u8] = unsafe {
            std::slice::from_raw_parts(
                frame.payload_bytes().as_ptr(),
                frame.payload_bytes().len(),
            )
        };
        let value = builder(payload)?;
        Ok(Self {
            value: ManuallyDrop::new(value),
            frame: ManuallyDrop::new(frame),
        })
    }
}

impl<T: 'static> std::ops::Deref for OwnedMessage<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.value }
}
```

### Request Deserialization (Server Side)

**Current generated code:**
```rust
let request: MyRequest = facet_format_postcard::from_slice(frame.payload_bytes())?;
let result = self.service.my_method(request).await;
```

**With zero-copy (for types with lifetime):**
```rust
let owned = OwnedMessage::<MyRequest<'static>>::try_new(frame, |payload| {
    facet_format_postcard::from_slice(payload)
})?;
let result = self.service.my_method(&*owned).await;
// Frame + deserialized value dropped together after call
```

### Response Deserialization (Client Side)

**Current generated code:**
```rust
let response: MyResponse = facet_format_postcard::from_slice(frame.payload_bytes())?;
Ok(response)
```

**With zero-copy:**
```rust
let owned = OwnedMessage::<MyResponse<'static>>::try_new(frame, |payload| {
    facet_format_postcard::from_slice(payload)
})?;
Ok(owned)
```

**Return type changes:**
```rust
// Before
async fn get_user(&self, id: u64) -> Result<User, RpcError>;

// After (with zero-copy response type)
async fn get_user(&self, id: u64) -> Result<OwnedMessage<User<'static>>, RpcError>;
```

Callers access data via `Deref`:
```rust
let response = client.get_user(123).await?;
println!("Name: {}", response.name);  // Deref to &User
```

### Streaming Responses

Each chunk in a stream is independently wrapped:

```rust
// Before
impl Stream<Item = Result<Chunk, RpcError>>

// After
impl Stream<Item = Result<OwnedMessage<Chunk<'static>>, RpcError>>
```

### Opt-in Mechanism

Zero-copy is **automatic based on type signature**:

```rust
// Owned type - no lifetime, uses current path
#[derive(Facet)]
pub struct SmallRequest {
    pub name: String,
}

// Borrowing type - has lifetime, uses zero-copy path
#[derive(Facet)]
pub struct LargeRequest<'a> {
    pub name: Cow<'a, str>,
    pub payload: &'a [u8],
}
```

The macro detects `<'a>` / `<'_>` / `<'static>` in the type and generates
the appropriate deserialization code.

### Service Trait Signatures

Service methods receive borrowed references (not the wrapper):

```rust
#[rapace::service]
trait MyService {
    // Request type has lifetime - receives &LargeRequest
    async fn process_large(&self, request: &LargeRequest<'_>) -> SmallResponse;

    // Request type is owned - receives owned SmallRequest
    async fn process_small(&self, request: SmallRequest) -> SmallResponse;
}
```

The macro handles wrapping/unwrapping transparently.

## Data Flow

```
SERVER RECEIVING REQUEST
========================

Transport
    │
    ▼
Frame { desc, payload: Bytes/Pooled/Shm }
    │
    ▼
OwnedMessage::try_new(frame, |payload| from_slice(payload))
    │
    ▼
OwnedMessage<Request<'static>>
    │
    ▼
service.method(&*owned)  ← borrowed reference to service
    │
    ▼
Drop OwnedMessage (frame + request together)


CLIENT RECEIVING RESPONSE
=========================

RPC call
    │
    ▼
Wait for response Frame
    │
    ▼
OwnedMessage::try_new(frame, |payload| from_slice(payload))
    │
    ▼
Return OwnedMessage<Response<'static>> to caller
    │
    ▼
Caller uses *response (Deref) or response.borrow_dependent()
    │
    ▼
Caller drops OwnedMessage when done
```

## Types That Benefit

| Current Type | Zero-Copy Alternative | Location |
|--------------|----------------------|----------|
| `String` | `Cow<'a, str>` | Anywhere |
| `Vec<u8>` | `&'a [u8]` or `Cow<'a, [u8]>` | Binary payloads |
| `Vec<(String, Vec<u8>)>` | `Vec<(Cow<'a, str>, &'a [u8])>` | Metadata |

### In rapace-core

```rust
// control.rs - ControlPayload::OpenChannel
pub struct OpenChannel<'a> {
    pub channel_id: u32,
    pub service_name: Cow<'a, str>,
    pub method_name: Cow<'a, str>,
    pub metadata: Vec<(Cow<'a, str>, &'a [u8])>,
}

// error.rs - CloseReason
pub enum CloseReason<'a> {
    Normal,
    Error(Cow<'a, str>),
}
```

### User-defined types

```rust
// Before
#[derive(Facet)]
pub struct DocumentRequest {
    pub path: String,
    pub content: Vec<u8>,
}

// After (zero-copy)
#[derive(Facet)]
pub struct DocumentRequest<'a> {
    pub path: Cow<'a, str>,
    pub content: &'a [u8],
}
```

## Macro Changes Required

### Detection

The macro needs to detect if a type has a lifetime parameter:

```rust
// In rapace-macros
fn has_lifetime_param(ty: &syn::Type) -> bool {
    // Check for <'a>, <'_>, <'static>, etc.
}
```

### Generated Code Branches

```rust
// For owned types (no lifetime)
quote! {
    let arg: #ty = facet_format_postcard::from_slice(frame.payload_bytes())?;
    self.service.#method_name(arg).await
}

// For borrowing types (has lifetime)
quote! {
    let owned = OwnedMessage::<#ty>::try_new(frame, |payload| {
        facet_format_postcard::from_slice(payload)
    })?;
    self.service.#method_name(&*owned).await
}
```

### Client Return Types

```rust
// For owned response types
quote! {
    let result: #return_type = facet_format_postcard::from_slice(response.payload_bytes())?;
    Ok(result)
}

// For borrowing response types
quote! {
    let owned = OwnedMessage::<#return_type>::try_new(response, |payload| {
        facet_format_postcard::from_slice(payload)
    })?;
    Ok(owned)
}
```

## Related Issues

- **#44 (Buffer pools)**: Complementary. Pools reduce allocation but still copy into
  the deserialized struct. Zero-copy eliminates that copy.

- **#45 (SHM)**: Complementary. SHM avoids kernel copy between processes. Zero-copy
  avoids copying from the SHM buffer into owned structs.

The optimizations stack:
```
SHM transport     → no kernel copy between processes
Buffer pools      → reuse allocations, reduce malloc pressure
Zero-copy deser   → no copy from buffer to struct fields
Inline payload    → 16 bytes in cache-line descriptor (smallest messages)
```

## Implementation Plan

1. ~~**Add `self_cell` dependency**~~ → Not needed! We use facet's variance tracking.

2. **Create `OwnedMessage<T>` type** in `rapace-core/src/owned_message.rs` ✅
   - Generic over `T: 'static + Facet<'static>`
   - Runtime covariance check via `T::SHAPE.variance`
   - Proper drop ordering with `ManuallyDrop`

3. **Update `rapace-macros`** to detect lifetime parameters and generate
   appropriate deserialization code (using `OwnedMessage`)

4. **Add integration tests** with borrowing types (`Cow<'a, str>`, `&'a [u8]`)

5. **Optionally migrate internal types** (`ControlPayload`, `CloseReason`) to
   use borrowing - this is lower priority since control frames are infrequent

## Current Status: Working

Zero-copy deserialization is fully functional with facet-format-postcard (as of
[facet-rs/facet#1475](https://github.com/facet-rs/facet/pull/1475)).

### What's Implemented
- `OwnedMessage<T>` type with proper drop ordering
- Runtime covariance check via `(T::SHAPE.variance)(T::SHAPE).can_shrink()`
- Macro detection of lifetime parameters in types
- Client-side response deserialization with zero-copy
- Integration tests for `Cow<'a, str>`, `&'a [u8]`, and combined types

### Still TODO
- Server-side request borrowing (requires trait signature changes)

## Open Questions

1. **Should `OwnedMessage` implement `Clone`?** Only if `Frame` is cheaply cloneable
   (it is for `Payload::Bytes` via `Bytes::clone()`).

2. **Streaming with large items**: Each chunk gets its own `OwnedMessage`. For very
   high-throughput streams, this is fine. For memory-constrained scenarios, we might
   want a "parse in place" API that processes chunks without accumulating.

3. **Interaction with buffer pools**: When using `Payload::Pooled`, the pooled buffer
   returns to the pool when `OwnedMessage` is dropped. This is correct behavior but
   means the buffer isn't reusable until the caller is done with the response.
