# rapace-testkit

[![crates.io](https://img.shields.io/crates/v/rapace-testkit.svg)](https://crates.io/crates/rapace-testkit)
[![documentation](https://docs.rs/rapace-testkit/badge.svg)](https://docs.rs/rapace-testkit)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-testkit.svg)](./LICENSE)

Conformance test suite for rapace transports.

Provides comprehensive tests for transport implementations including:

- **Round-trip messaging**: Unary and streaming RPC validation
- **Error handling**: Connection failures and protocol errors
- **Concurrent operations**: Multiple clients and concurrent streams
- **Edge cases**: Large payloads, rapid reconnects, etc.

Use this test suite when implementing new transports or verifying existing implementations.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/bearcove/rapace/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/bearcove/rapace/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
