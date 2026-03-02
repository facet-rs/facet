#![deny(unsafe_code)]

//! Code generation for roam RPC bindings across multiple languages.
//!
//! # This Is Where Code Generation Actually Happens
//!
//! While [`roam-macros`] parses your service traits and emits metadata, this crate
//! consumes that metadata and generates actual protocol implementations for:
//!
//! - **TypeScript** — Browser and Node.js clients
//! - **Swift** — iOS/macOS clients
//! - **Go** — Server and client implementations
//! - **Java** — Android and server implementations
//! - **Python** — Client bindings
//! - **Rust** — Extended codegen beyond what the proc macro provides
//!
//! # Usage: In Your build.rs
//!
//! ```ignore
//! // In your service crate's build.rs
//! use my_service::calculator_service_detail;
//!
//! fn main() {
//!     let detail = calculator_service_detail();
//!
//!     // Generate TypeScript client
//!     let ts_code = roam_codegen::targets::typescript::generate(&detail);
//!     std::fs::write("generated/calculator.ts", ts_code).unwrap();
//!
//!     // Generate Go server
//!     let go_code = roam_codegen::targets::go::generate(&detail);
//!     std::fs::write("generated/calculator.go", go_code).unwrap();
//! }
//! ```
//!
//! # The Pipeline
//!
//! ```text
//! #[service] trait     →    ServiceDescriptor    →    roam-codegen    →    .ts, .go, .swift, ...
//!   (your code)            (runtime metadata)     (build script)       (generated code)
//! ```
//!
//! # Why Build Scripts? (The Technical Reason)
//!
//! Code generation happens in build scripts (not proc macros) because **proc macros
//! cannot see into the type system**.
//!
//! When a proc macro sees `Tx<String>` in a method signature, it sees tokens — it has
//! no idea if `Tx` refers to `roam::channel::Tx` or some user-defined type. It cannot
//! resolve type aliases, follow generic parameters, or inspect nested types.
//!
//! But here, with `facet::Shape`, we have **full type introspection**:
//!
//! ```ignore
//! // We can identify roam's Tx vs user-defined types
//! let shape = <Tx<String> as facet::Facet>::SHAPE;
//!
//! // We can traverse nested types like Result<Vec<Tx<T>>, Error>
//! // and find the Tx buried inside
//! ```
//!
//! This is why validation happens here:
//! - Is this actually roam's `Tx`/`Rx` channel type?
//! - Are channel types incorrectly used in error positions?
//! - What serialization does this nested type require?
//!
//! Additional benefits of build scripts:
//!
//! 1. **File I/O** — Build scripts can write files; proc macros cannot
//! 2. **Configuration** — Build scripts can read config files, env vars, etc.
//! 3. **Flexibility** — Different projects can generate different subsets of bindings
//!
//! [`roam-macros`]: https://docs.rs/roam-service-macros

pub mod code_writer;
mod render;
pub mod targets;

use roam_types::MethodDescriptor;

pub fn method_id(detail: &MethodDescriptor) -> u64 {
    detail.id.0
}
