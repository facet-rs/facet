+++
title = "Specification"
description = "Formal Rapace RPC protocol specification"
+++

This section contains the formal specification for the Rapace RPC protocol. It defines the wire format, semantics, and requirements for conforming implementations in any language.

## Overview

### Type System & Encoding

- [Data Model](@/spec/data-model.md) – supported types and primitives
- [Wire Format](@/spec/wire-format.md) – postcard encoding and Rapace frames
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

### Observability & Implementation

- [Observability](@/spec/observability.md) – tracing, metrics, and instrumentation
- [Transport Considerations](@/spec/transports.md) – transport-specific behaviors and optimizations

## Status

This specification is under active development. The [Core Protocol](@/spec/core.md) reflects the current Rust implementation. Other sections describe planned features and conventions that implementations should follow.

For usage and examples, see the [Guide](/guide/) and [crate documentation](https://docs.rs/rapace).
