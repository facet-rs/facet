+++
title = "Zero-copy deserialization"
description = "Avoid copying data by borrowing directly from the frame"
+++

> **Note:** This page covers Rust-specific optimizations using lifetimes and borrowing. Other language implementations achieve similar goals through different mechanisms (e.g., `Uint8Array` views in TypeScript, slice types in Swift).

When rapace deserializes a response, it normally copies data out of the frame's payload into owned types like `String` and `Vec<u8>`. For large payloads, this copying can be significant overhead.

Zero-copy deserialization lets your types borrow directly from the frame's payload buffer, eliminating the copy entirely.

## How to opt in

Change your type definitions to use borrowing types with a lifetime parameter:

```rust
// Before: copies data
#[derive(facet::Facet)]
struct Response {
    name: String,
    data: Vec<u8>,
}

// After: borrows from frame
#[derive(facet::Facet)]
struct Response<'a> {
    name: Cow<'a, str>,
    data: &'a [u8],
}
```

That's it. The macro detects the lifetime parameter and automatically uses zero-copy deserialization.

## What changes

When your response type has a lifetime, the client method returns `OwnedMessage<Response<'static>>` instead of `Response`:

```rust
// Owned type (no lifetime)
async fn get_user(&self, id: u64) -> Result<User, RpcError>;

// Borrowing type (has lifetime)
async fn get_document(&self, id: u64) -> Result<OwnedMessage<Document<'static>>, RpcError>;
```

## Using the response

`OwnedMessage` implements `Deref`, so you can use it like a normal reference:

```rust
let doc = client.get_document(123).await?;

// These all work via Deref:
println!("{}", doc.title);
process_data(doc.content);
let name: &str = &doc.author;
```

The `'static` in the type is a "lie" for ergonomics—the actual lifetime is tied to the `OwnedMessage`. When the `OwnedMessage` is dropped, the borrowed data becomes invalid. The borrow checker enforces this through `Deref`.

## How it works

`OwnedMessage<T>` is a self-referential struct that holds both:
- The frame (boxed for a stable address)
- The deserialized value (which borrows from the frame)

```
┌─────────────────────────────────────┐
│ OwnedMessage                        │
│ ┌─────────────────────────────────┐ │
│ │ frame: Box<Frame>               │ │
│ │ ┌─────────────────────────────┐ │ │
│ │ │ payload: [u8]               │ │ │
│ │ └──────────▲──────────────────┘ │ │
│ └────────────│──────────────────┘ │
│              │ borrows from        │
│ ┌────────────┴──────────────────┐ │
│ │ value: T                      │ │
│ │   name: Cow ───────────────┐  │ │
│ │   data: &[u8] ─────────────┤  │ │
│ └────────────────────────────│──┘ │
└──────────────────────────────│────┘
                               │
                    points into payload
```

When dropped, the value is dropped first (releasing the borrows), then the frame.

## Covariance requirement

Your type must be **covariant** in its lifetime parameter. This means the lifetime only appears in "read-only" positions.

Covariant (allowed):
- `&'a T`
- `Cow<'a, T>`
- `Box<T>`, `Vec<T>` where `T` is covariant

Not covariant (will panic at runtime):
- `&'a mut T`
- `fn(&'a T)`
- `Cell<&'a T>`

If you accidentally use a non-covariant type, you'll get a panic with an explanation when the macro-generated code tries to deserialize.

## When to use it

Zero-copy is most beneficial when:
- Responses contain large strings or byte arrays
- You only need to read the data, not modify or store it long-term
- You're processing high volumes of messages

For small responses or when you need to store the data beyond the RPC call, owned types (`String`, `Vec<u8>`) are simpler and may be just as fast.

## Extracting owned data

If you need to convert borrowed data to owned, use the standard library methods:

```rust
let doc = client.get_document(123).await?;

// Convert Cow to owned String
let title: String = doc.title.into_owned();

// Copy slice to Vec
let content: Vec<u8> = doc.content.to_vec();
```

## Recovering the frame

If you need the underlying frame back (e.g., to forward it), use `into_frame()`:

```rust
let doc = client.get_document(123).await?;
// ... inspect doc ...
let frame = doc.into_frame();  // value is dropped, frame returned
```
