+++
title = "Comparisons"
description = "How rapace differs from other approaches"
+++

This page is an info dump about how rapace compares to a few things people might reasonably reach for instead.

## gRPC / protobuf

In Rust, “[gRPC](https://grpc.io)” usually means [`tonic`](https://docs.rs/tonic) plus [protobuf](https://protobuf.dev/). The important part for this comparison is not HTTP/2, it’s protobuf’s type model: enums as `i32` with names, `oneof` as a tagged‑union substitute, default values and presence rules, `repeated`/`map` fields, and field numbers as the real identity of everything.

That model does not line up very well with how people usually write Rust. Rich enums with payloads get flattened into `oneof` plus extra messages. Presence and defaults fight with `Option<T>` and “zero means zero”. Lists and maps don’t carry the invariants you’d normally encode in the type system. Even if you generate `.proto` from Rust instead of the other way around, you are still targeting “whatever protobuf can express” as the contract.

rapace treats the Rust type system as the contract. Service definitions are Rust traits with [`#[rapace::service]`](https://docs.rs/rapace-macros/latest/rapace_macros/attr.service.html). Request and response types are ordinary Rust structs/enums that derive `Facet`. [facet](https://facets.rs) gives you shapes for those types; [`facet-postcard`](https://docs.rs/facet-postcard/latest/facet_postcard/) turns those shapes into bytes and back again. There is no separate `.proto` file; the Rust types (and their shapes) are the schema, and the same trait is used over SHM, stream/TCP, WebSocket, or in‑memory transports without rewriting it for each case.

## Dynamic libraries (plugins via dlopen / libloading)

Another way to structure plugins is as dynamic libraries loaded into the host process. You compile a `.so`/`.dylib`/`.dll`, load it at runtime (e.g. with [`libloading`](https://docs.rs/libloading)), and call exported functions directly. Host and plugin share one address space, one allocator, one set of global variables.

That has some nice properties: calls are just function calls, and you don’t need to think about framing or transports. But it also ties the plugin very tightly to the host binary:

- they must agree on ABI and symbol layout;
- unloading or replacing a plugin safely is hard, because every bit of shared state that points into plugin code or data has to be cleaned up first;
- crash isolation is weak; a bad plugin can corrupt host memory directly.

With rapace, plugins are just separate executables. They talk to the host over IPC/RPC instead of direct calls. The boundary is “send a request, get a response (or stream)”, not “jump into a function pointer in the same address space”. That means:

- ABI is "bytes on the wire" defined by facet shapes and postcard, not C‑style calling conventions;
- crashes and leaks are process‑local; the host can in principle kill and restart a plugin without corrupting its own heap;
- the transport can change (SHM on one machine, stream/TCP to another, WebSocket to a browser) without changing the service trait.

You pay for that with serialization/deserialization and some machinery around frames and channels. In exchange you avoid the class of problems that come from mixing plugin and host memory, and you get one RPC surface that can be reused in more than one place.

## tarpc

[`tarpc`](https://docs.rs/tarpc) is another Rust RPC framework that uses Rust traits to describe services. You annotate a trait with <code>#[tarpc::service]</code>, it generates client and server code, and you can plug it into transports like TCP, Unix sockets, or in‑process channels. Data is typically serialized with [`serde`](https://docs.rs/serde) over those transports.

Compared to tarpc, rapace is doing a few things differently:

- It leans on facet + `facet-postcard` instead of [`serde`](https://docs.rs/serde) alone. That gives you explicit type shapes that are reused by registry and tooling, not just an encoder/decoder pair.
- It has a shared‑memory transport with an explicit layout (descriptor rings + slab allocator) for same‑machine cases, in addition to stream and WebSocket transports.
– It has a fairly opinionated frame and session layer ([`Frame`](https://docs.rs/rapace-core/latest/rapace_core/struct.Frame.html), [`FrameView`](https://docs.rs/rapace-core/latest/rapace_core/struct.FrameView.html), [`RpcSession`](https://docs.rs/rapace/latest/rapace/struct.RpcSession.html), channels, control frames) that is intended to be reused across multiple kinds of tooling (plugins, devtools, tracing), not just classic request/response RPC.

Conceptually they are in the same broad space (“Rust traits as RPC services”), but they make different trade‑offs about transports, encoding, and introspection. If you just want “[serde over TCP](https://serde.rs/)” to call a service, tarpc is a straightforward option. If you want the facet/postcard schema story and the SHM transport behaviour, rapace is the thing that has those.
