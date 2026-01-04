+++
title = "Specification"
description = "Formal Rapace RPC protocol specification"
weight = 1
+++

This section contains the formal specification for the Rapace RPC protocol. It defines the wire format, semantics, and requirements for conforming implementations in any language.

## Overview

### Type System & Encoding

- [Data Model](@/spec/data-model.md) – supported types and primitives
- [Payload Encoding](@/spec/payload-encoding.md) – postcard binary format
- [Frame Format](@/spec/frame-format.md) – MsgDescHot descriptor and payload abstraction
- [Transport Bindings](@/spec/transport-bindings.md) – TCP, WebSocket, and shared memory framing
- [Schema Evolution](@/spec/schema-evolution.md) – compatibility, hashing, and versioning

### RPC Protocol

- [Core Protocol](@/spec/core.md) – frames, channels, and control messages
- [Handshake & Capabilities](@/spec/handshake.md) – connection establishment and feature negotiation
- [Cancellation & Deadlines](@/spec/cancellation.md) – request cancellation and deadline semantics
- [Error Handling & Retries](@/spec/errors.md) – error codes, status, and retry semantics
- [Metadata Conventions](@/spec/metadata.md) – standard metadata keys for auth, tracing, and priority

### Annexes

- [Annex A: Requirements Guidelines](@/spec/requirements-guidelines.md) – principles for writing traceable, testable requirements

## Status

This specification is under active development. The [Core Protocol](@/spec/core.md) reflects the current Rust implementation. Other sections describe planned features and conventions that implementations should follow.

For usage and examples, see the [Guide](@/guide/_index.md) and [crate documentation](https://docs.rs/rapace).
