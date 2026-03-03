+++
title = "Swift Guide"
description = "How Swift codegen and the Swift runtime integrate with Rust-defined Roam services."
weight = 22
+++

Swift support in Roam is generated from Rust service definitions.

## Layer mapping

- Schema source: Rust `#[roam::service]` traits
- Code generation: `roam-codegen` (Swift target)
- Runtime package: `swift/roam-runtime`

## Typical flow

1. Define/update service traits in Rust.
2. Run your codegen step to emit Swift client/server bindings.
3. Use `roam-runtime` transport/runtime primitives in Swift code.
4. Validate behavior through spec and cross-language tests.

## Runtime boundaries

- Protocol compatibility stays anchored on Rust descriptors.
- Swift code should treat generated bindings as API surface.
- Shared-memory integrations go through runtime/FFI layers exposed by the workspace.

## Practical recommendation

Keep generated Swift code checked in or deterministically regenerable in CI so schema drift is easy to detect.
