+++
title = "Comparisons"
description = "How rapace differs from other approaches"
+++

This page compares rapace to other RPC systems and approaches.

## gRPC / Protocol Buffers

[gRPC](https://grpc.io) uses Protocol Buffers as its IDL and wire format. You write `.proto` files, run a code generator, and get client/server stubs for your language.

**IDL-first vs implementation-first:**
- Protobuf is IDL-first: you define types in `.proto`, then generate code
- Rapace is implementation-first: you define types in Rust, then generate bindings for other languages

**Type model:**
- Protobuf has its own type system (enums as i32, `oneof` for unions, field numbers as identity)
- Rapace uses Rust's type system directly (rich enums, `Option<T>`, algebraic data types)

**Schema compatibility:**
- Protobuf uses field numbers for forward/backward compatibility
- Rapace uses structural hashing; incompatible schemas are rejected at handshake

**Cross-language story:**
- gRPC/protobuf has mature generators for many languages
- Rapace has Rust (reference), TypeScript, and Swift; Go and Java are planned

Both support streaming RPCs. gRPC runs over HTTP/2; rapace has its own framing protocol that works over shared memory, WebSocket, and TCP.

## Cap'n Proto

[Cap'n Proto](https://capnproto.org/) is another IDL-based system, designed for zero-copy access to serialized data.

**Similarities:**
- Both support zero-copy access to message data
- Both have their own binary wire format

**Differences:**
- Cap'n Proto is IDL-first (`.capnp` schema files)
- Rapace derives schema from Rust code via Facet
- Cap'n Proto's wire format is self-describing; rapace's (Postcard) is not

## FlatBuffers

[FlatBuffers](https://flatbuffers.dev/) is Google's zero-copy serialization library.

**Similarities:**
- Both support zero-copy reads
- Both are compact binary formats

**Differences:**
- FlatBuffers is IDL-first (`.fbs` schema files)
- FlatBuffers requires field access through generated accessors; rapace deserializes into native types
- Rapace has an integrated RPC protocol; FlatBuffers is serialization-only

## Dynamic libraries (dlopen)

For in-process plugins, you can use dynamic libraries loaded via `dlopen`/`libloading`.

**Advantages of dynamic libraries:**
- Function calls, not RPC — no serialization overhead
- Shared address space for direct memory access

**Advantages of rapace's process-per-plugin model:**
- Crash isolation: a plugin crash doesn't take down the host
- No ABI concerns: the contract is "bytes on the wire"
- Hot reload potential: stop old process, start new one
- Same service definition works locally (SHM) or remotely (TCP/WebSocket)

The trade-off is serialization overhead vs. isolation and flexibility.

## tarpc

[`tarpc`](https://docs.rs/tarpc) is a Rust-only RPC framework that also uses traits to define services.

**Similarities:**
- Both use Rust traits as service definitions
- Both generate client/server code from traits

**Differences:**
- tarpc uses serde; rapace uses Facet (with explicit type shapes for introspection and codegen)
- rapace has a shared-memory transport with zero-copy support
- rapace generates cross-language bindings; tarpc is Rust-only
- rapace has a formal protocol spec; tarpc is a Rust library

If you want Rust-to-Rust RPC with serde, tarpc is simpler. If you want cross-language support, zero-copy SHM, or the Facet introspection story, rapace provides those.

## JSON-RPC

[JSON-RPC](https://www.jsonrpc.org/) is a simple RPC protocol using JSON.

**Advantages of JSON-RPC:**
- Human-readable wire format
- No code generation required
- Broad language support

**Advantages of rapace:**
- Compact binary format (Postcard)
- Type-safe generated clients with compile-time checking
- Zero-copy shared memory transport
- Streaming support

JSON-RPC is good for quick integration and debugging. Rapace is better for performance-sensitive applications with strong typing requirements.
