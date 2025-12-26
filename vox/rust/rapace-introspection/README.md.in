# rapace-introspection

[![crates.io](https://img.shields.io/crates/v/rapace-introspection.svg)](https://crates.io/crates/rapace-introspection)
[![documentation](https://docs.rs/rapace-introspection/badge.svg)](https://docs.rs/rapace-introspection)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-introspection.svg)](./LICENSE)

Service introspection RPC service for rapace.

This crate provides a `ServiceIntrospection` RPC service that allows clients to query what services and methods are available at runtime.

## Features

- List all registered services
- Describe a specific service by name
- Check if a method ID is supported
- Runtime service discovery

## Example

```rust
use rapace_introspection::{ServiceIntrospection, ServiceIntrospectionServer};
use rapace_registry::introspection::DefaultServiceIntrospection;

// Create introspection server
let introspection = DefaultServiceIntrospection::new();
let server = ServiceIntrospectionServer::new(introspection);

// Add to your cell's dispatcher
use rapace_cell::DispatcherBuilder;
let dispatcher = DispatcherBuilder::new()
    .add_service(server)
    .build();
```

## Re-exports

For convenience, this crate re-exports key types from `rapace-registry`:

- `ServiceInfo` - Information about a registered service
- `MethodInfo` - Information about a service method
- `ArgInfo` - Information about method arguments
- `DefaultServiceIntrospection` - Default implementation of the introspection trait

## License

MIT OR Apache-2.0
