# Facet Opaque Adapter for Roam Payload

## Purpose

Define the contract behind `#[facet(opaque = thing)]` so Roam can:

1. serialize erased outgoing payloads,
2. parse incoming envelopes before payload concrete type is known,
3. defer payload decoding with low-copy behavior when possible.

## Roam-types Source Cross-References

1. Wire envelope and payload lifetime:
   `Message<'payload>`, `MessagePayload<'payload>`, `Payload<'payload>` in
   `rust/roam-types/src/message.rs`.
2. Transport boundary:
   `Link`, `LinkTx`, `LinkRx`, `LinkRx::recv -> Result<Option<Backing>, _>` in
   `rust/roam-types/src/link.rs`.
3. Conduit boundary:
   `Conduit`, `ConduitRx`, `ConduitRx::recv -> Result<Option<SelfRef<T>>, _>` in
   `rust/roam-types/src/conduit.rs`.
4. Backing ownership and transform:
   `SelfRef<T>`, `Backing`, `SelfRef::try_new`, `SelfRef::map` in
   `rust/roam-types/src/selfref.rs`.
5. Dispatch lookup and type plans:
   `ServiceDescriptor::by_id`, `MethodDescriptor.args_plan` in
   `rust/roam-types/src/services.rs`, and `RpcPlan`/`type_plan` in
   `rust/roam-types/src/rpc_plan.rs`.

## Core Invariant

For incoming data, the runtime object is `SelfRef<Message<'static>>`:
`(Backing, Message<'static>)`.

`Backing` is single-owner and not split into sub-backings. Therefore deferred
payload state inside `Message` stores either:

1. a borrowed byte slice into the message backing, or
2. owned bytes.

It does not store a second `Backing` handle for a payload subrange.

## Lifetime Model

1. Incoming messages are held as `SelfRef<Message<'static>>`.
2. Outgoing messages are created in call scope as `Message<'call>`.
3. `Payload::Borrowed` is for outgoing `Message<'call>`.
4. `Payload::RawBorrowed` is for incoming `Message<'static>` backed by
   `SelfRef` ownership of the full frame backing.

## Annotation

```rust
#[facet(opaque = PayloadAdapter)]
pub enum Payload<'payload> {
    Borrowed { /* outgoing erased value */ },
    RawBorrowed(&'payload [u8]),
    RawOwned(Vec<u8>),
}
```

`PayloadAdapter` is a type that implements the adapter trait below.

## Public Adapter Interface

Public interface is typed. Pointer shims are internal.

```rust
pub struct OpaqueSerialize {
    pub ptr: PtrConst,
    pub shape: &'static Shape,
    pub plan: Option<&'static TypePlanCore>,
}

pub enum OpaqueDeserialize<'de> {
    Borrowed(&'de [u8]),
    Owned(Vec<u8>),
}

pub trait FacetOpaqueAdapter {
    type Error;
    type SendValue<'a>;
    type RecvValue<'de>;

    /// Outgoing path: map typed value to erased serialization inputs.
    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize;

    /// Incoming path: build deferred payload representation.
    fn deserialize_build<'de>(input: OpaqueDeserialize<'de>)
        -> Result<Self::RecvValue<'de>, Self::Error>;
}
```

## Sample Implementation

```rust
pub struct PayloadAdapter;

impl FacetOpaqueAdapter for PayloadAdapter {
    type Error = String;
    type SendValue<'a> = Payload<'a>;
    type RecvValue<'de> = Payload<'de>;

    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
        match value {
            Payload::Borrowed { ptr, shape, plan, .. } => OpaqueSerialize {
                ptr: *ptr,
                shape,
                plan: *plan,
            },
            // RawBorrowed/RawOwned are receive-side only — serializing
            // them means the whole Message is being forwarded, which
            // happens at the Message level, not through the adapter.
            _ => unreachable!("serialize only called on outgoing messages"),
        }
    }

    fn deserialize_build<'de>(input: OpaqueDeserialize<'de>)
        -> Result<Self::RecvValue<'de>, Self::Error>
    {
        match input {
            OpaqueDeserialize::Borrowed(bytes) => Ok(Payload::RawBorrowed(bytes)),
            OpaqueDeserialize::Owned(bytes) => Ok(Payload::RawOwned(bytes)),
        }
    }
}
```

## Directional Semantics

### Send

1. Serializer reaches `#[facet(opaque = ...)]` field.
2. Calls `serialize_map(...)`.
3. Uses returned `(ptr, shape, plan)` to serialize payload bytes.

### Receive

1. Parser decodes payload bytes.
2. If parser input can borrow stably, call `deserialize_build(Borrowed(&[u8]))`.
3. Otherwise call `deserialize_build(Owned(Vec<u8>))`.
4. Store returned deferred payload value inside `Message`.

## Where Slicing Lives

Slicing belongs to parser/input logic, not to payload storage types.

1. Parser decides whether borrowed slice is valid.
2. Adapter boundary receives either borrowed slice or owned bytes.
3. If parser internally tracks ranges, it resolves range -> slice before calling
   `deserialize_build`.

## End-to-End Incoming Flow

Incoming flow is:

1. Link layer receives bytes via `LinkRx::recv` and returns `Backing`.
2. Conduit deserializes envelope via `ConduitRx::recv` using that backing and builds
   `SelfRef<Message<'static>>`.
3. Driver receives `SelfRef<Message<'static>>` and routes by message kind.
4. Dispatch reads `method_id`, resolves concrete args `(Shape, TypePlanCore)`
   through `ServiceDescriptor::by_id(...).args_plan`.
5. Dispatch consumes/maps `SelfRef<Message<'static>>` into
   `SelfRef<ConcreteArgs>` using the same backing (via `SelfRef::map`-style transform).

This is a move/transform, not a backing split.

## Outgoing vs Incoming

1. Incoming driver-side value is `SelfRef<Message<'static>>` and can contain
   `RawBorrowed(&[u8])`.
2. Outgoing driver-side value is `Message<'call>` and uses erased outgoing form
   (`Borrowed { ... }`) plus optional owned-raw forwarding form.

## Non-goals

1. No universal facet-wide backing container.
2. No requirement that every format support borrowed input.
3. No payload-level ownership model that splits or clones transport `Backing`.
