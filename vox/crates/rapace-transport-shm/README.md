# rapace-transport-shm

[![crates.io](https://img.shields.io/crates/v/rapace-transport-shm.svg)](https://crates.io/crates/rapace-transport-shm)
[![documentation](https://docs.rs/rapace-transport-shm/badge.svg)](https://docs.rs/rapace-transport-shm)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-transport-shm.svg)](./LICENSE)

Shared memory transport for rapace RPC.

This crate implements a transport on top of POSIX shared memory. It follows the layout described in the crate documentation (segment header, descriptor rings, and a data segment managed by a slab-style allocator).

## Characteristics

- single-writer/single-reader rings in each direction;
- descriptors that point into a shared data segment;
- optional SHM-backed allocation for callers that want to avoid extra copies;
- Linux/Unix only (requires a POSIX-style shared memory API).

See the crate docs for details about the layout and configuration options.

## Configuration

Enable the `allocator` feature for zero-copy SHM allocation:

```toml
rapace = { version = "0.1", features = ["shm"] }
rapace-transport-shm = { version = "0.1", features = ["allocator"] }
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
