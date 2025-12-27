+++
title = "Specification"
description = "Formal Rapace RPC protocol specification"
+++

This section contains the formal specification for the Rapace RPC protocol. It defines the wire format, semantics, and requirements for conforming implementations in any language.

## Overview

### Type System & Encoding

- [Data Model](@/spec/data-model.md) – supported types and primitives
- [Payload Encoding](@/spec/payload-encoding.md) – postcard binary format
- [Frame Format](@/spec/frame-format.md) – MsgDescHot descriptor and payload abstraction
- [Transport Bindings](@/spec/transport-bindings.md) – TCP, WebSocket, and shared memory framing
- [Schema Evolution](@/spec/schema-evolution.md) – compatibility, hashing, and versioning
- [Language Mappings](@/spec/language-mappings.md) – Rust, Swift, TypeScript, Go, Java
- [Code Generation](@/spec/codegen.md) – code generation architecture and IR

### RPC Protocol

- [Core Protocol](@/spec/core.md) – frames, channels, and control messages
- [Handshake & Capabilities](@/spec/handshake.md) – connection establishment and feature negotiation
- [Cancellation & Deadlines](@/spec/cancellation.md) – request cancellation and deadline semantics
- [Error Handling & Retries](@/spec/errors.md) – error codes, status, and retry semantics
- [Metadata Conventions](@/spec/metadata.md) – standard metadata keys for auth, tracing, and priority

### Quality of Service

- [Prioritization & QoS](@/spec/prioritization.md) – scheduling and quality of service
- [Overload & Draining](@/spec/overload.md) – graceful degradation and server shutdown

### Security

- [Security Profiles](@/spec/security.md) – security requirements and deployment profiles

### Observability & Implementation

- [Observability](@/spec/observability.md) – tracing, metrics, and instrumentation
- [Transport Requirements](@/spec/transports.md) – transport abstraction and new transport guidance
- [Compliance & Testing](@/spec/compliance.md) – conformance test suite and certification

## Status

This specification is under active development. The [Core Protocol](@/spec/core.md) reflects the current Rust implementation. Other sections describe planned features and conventions that implementations should follow.

For usage and examples, see the [Guide](/guide/) and [crate documentation](https://docs.rs/rapace).
