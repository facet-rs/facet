# rapace-core

[![crates.io](https://img.shields.io/crates/v/rapace-core.svg)](https://crates.io/crates/rapace-core)
[![documentation](https://docs.rs/rapace-core/badge.svg)](https://docs.rs/rapace-core)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-core.svg)](./LICENSE)

Core types and traits for rapace RPC framework.

This crate provides the fundamental types and protocols used by rapace:

- **Transport traits**: Abstraction for different transport implementations
- **Frame formats**: Protocol buffers for RPC messaging
- **RPC session management**: Core session and session pump types
- **Serialization**: Support for multiple serialization formats via facet

This is a low-level crate primarily used by rapace itself and transport implementations.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
