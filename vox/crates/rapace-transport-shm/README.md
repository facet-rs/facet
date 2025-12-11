# rapace-transport-shm

[![crates.io](https://img.shields.io/crates/v/rapace-transport-shm.svg)](https://crates.io/crates/rapace-transport-shm)
[![documentation](https://docs.rs/rapace-transport-shm/badge.svg)](https://docs.rs/rapace-transport-shm)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-transport-shm.svg)](./LICENSE)

Shared memory transport for rapace RPC.

The performance reference implementation using POSIX shared memory for ultra-low latency local IPC.

## Features

- **Ultra-low latency**: Microsecond-scale message passing via shared memory
- **Zero-copy**: Optional zero-copy allocation directly into SHM slots
- **Linux/Unix**: Requires a POSIX-compliant OS

## Performance

For latency-sensitive applications requiring local inter-process communication, this transport delivers sub-microsecond message round-trip times.

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
