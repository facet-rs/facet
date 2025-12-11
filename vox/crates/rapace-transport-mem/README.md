# rapace-transport-mem

[![crates.io](https://img.shields.io/crates/v/rapace-transport-mem.svg)](https://crates.io/crates/rapace-transport-mem)
[![documentation](https://docs.rs/rapace-transport-mem/badge.svg)](https://docs.rs/rapace-transport-mem)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-transport-mem.svg)](./LICENSE)

In-process memory transport for rapace RPC.

The semantic reference implementation for rapace transports. All messages are passed via memory without any I/O overhead. Useful for:

- **Testing**: Unit tests and integration tests
- **Single-process RPC**: When you need RPC semantics without cross-process boundaries
- **Reference implementation**: Understanding how rapace transports work

## Feature

Enabled by default in rapace. Use `default-features = false` to disable.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
