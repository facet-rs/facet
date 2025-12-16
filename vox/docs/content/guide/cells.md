+++
title = "Cells"
description = "Building cells with rapace-cell"
+++

This page describes `rapace-cell`, a helper crate that eliminates boilerplate when building cells that communicate with a host process via shared memory.

## Overview

When building a cell process that talks to a host over SHM, there is a fair amount of common setup:

- Parse command-line arguments to find the SHM path
- Wait for the host to create the SHM file
- Open the SHM session with the right configuration
- Create an RPC session with the correct channel ID convention
- Set up a service dispatcher
- Run the session loop

The `rapace-cell` crate wraps all of this into a few simple functions.

### Before (95+ lines)

```rust,noexec
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use rapace::transport::shm::{ShmSession, ShmSessionConfig};
use rapace::{Frame, RpcError, RpcSession, Transport};

const SHM_CONFIG: ShmSessionConfig = ShmSessionConfig {
    ring_capacity: 256,
    slot_size: 65536,
    slot_count: 128,
};

fn parse_args() -> Result<PathBuf, Error> {
    // argument parsing logic...
}

fn create_dispatcher(impl_: MyServiceImpl) -> impl Fn(...) -> ... {
    // dispatcher setup...
}

#[tokio::main]
async fn main() -> Result<()> {
    let shm_path = parse_args()?;

    // Wait for SHM file
    while !shm_path.exists() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let shm_session = ShmSession::open_file(&shm_path, SHM_CONFIG)?;
    let transport = Transport::shm(shm_session);
    let session = Arc::new(RpcSession::with_channel_start(transport, 2));

    let dispatcher = create_dispatcher(MyServiceImpl);
    session.set_dispatcher(dispatcher);

    session.run().await?;
    Ok(())
}
```

### After (3 lines)

```rust,noexec
use rapace_cell::run;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(MyServiceServer::new(MyServiceImpl)).await?;
    Ok(())
}
```

## Single-service cells

Most cells expose a single service. Use the `run()` function:

```rust,noexec
use rapace_cell::run;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server = MyServiceServer::new(MyServiceImpl);
    run(server).await?;

    Ok(())
}
```

The `run()` function:

1. Parses CLI arguments to find `--shm-path=PATH` or the first positional argument
2. Waits up to 5 seconds for the host to create the SHM file
3. Opens the SHM session with [default configuration](#default-configuration)
4. Creates an RPC session using even channel IDs (cell convention)
5. Sets up the service dispatcher
6. Runs the session loop until the connection closes

## Multi-service cells

For cells that expose multiple services, use `run_multi()`:

```rust,noexec
use rapace_cell::run_multi;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    run_multi(|builder| {
        builder
            .add_service(MyServiceServer::new(MyServiceImpl))
            .add_service(AnotherServiceServer::new(AnotherServiceImpl))
    }).await?;

    Ok(())
}
```

When a method is called, the dispatcher tries each service in order until one handles it. If no service recognizes the method ID, an `Unimplemented` error is returned.

## CLI arguments

The cell runtime accepts the SHM path in two formats:

```bash
# Flag format (recommended)
./my-cell --shm-path=/tmp/my-app.shm

# Positional format
./my-cell /tmp/my-app.shm
```

## Default configuration

The default SHM configuration is:

```rust,noexec
pub const DEFAULT_SHM_CONFIG: ShmSessionConfig = ShmSessionConfig {
    ring_capacity: 256,  // 256 descriptors in flight
    slot_size: 65536,    // 64KB per slot
    slot_count: 128,     // 128 slots = 8MB total
};
```

This should match most hosts. If you need different settings, use `run_with_config()` or `run_multi_with_config()`:

```rust,noexec
use rapace_cell::run_with_config;
use rapace::transport::shm::ShmSessionConfig;

let custom_config = ShmSessionConfig {
    ring_capacity: 512,
    slot_size: 131072,  // 128KB
    slot_count: 256,
};

run_with_config(server, custom_config).await?;
```

## Channel ID conventions

rapace uses a convention to avoid channel ID collisions:

- **Hosts** use odd channel IDs starting from 1 (1, 3, 5, ...)
- **Cells** use even channel IDs starting from 2 (2, 4, 6, ...)

The cell runtime handles this automatically. You do not need to configure it.

## Error handling

The cell runtime returns `CellError` for common failure modes:

| Variant | Meaning |
|---------|---------|
| `CellError::Args` | Invalid command-line arguments (missing SHM path) |
| `CellError::ShmTimeout` | SHM file was not created by host within 5 seconds |
| `CellError::ShmOpen` | Failed to open SHM session |
| `CellError::Rpc` | RPC session error |
| `CellError::Transport` | Transport-level error |

## Custom setup with RpcSessionExt

If you need more control but still want simplified service setup, use the `RpcSessionExt` trait:

```rust,noexec
use rapace_cell::{RpcSessionExt, DEFAULT_SHM_CONFIG};
use rapace::transport::shm::{ShmSession, ShmTransport};
use rapace::RpcSession;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Your custom setup logic...
    let shm_session = ShmSession::open_file(&shm_path, DEFAULT_SHM_CONFIG)?;
    let transport = Arc::new(ShmTransport::new(shm_session));
    let session = Arc::new(RpcSession::with_channel_start(transport, 2));

    // Simple service setup with extension trait
    session.set_service(MyServiceServer::new(MyServiceImpl));

    session.run().await?;
    Ok(())
}
```

## Tracing

The cell runtime does not configure tracing. Set it up yourself:

```rust,noexec
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simple console logging
    tracing_subscriber::fmt::init();

    // Or forward logs to the host with rapace-tracing
    // (see rapace-tracing documentation)

    run(server).await?;
    Ok(())
}
```

## Where this fits in the stack

The cell runtime sits above the layers described in [Architecture](architecture.md):

```text
┌─────────────────────────────────────────┐
│  Cell runtime (rapace-cell)             │  ← This crate
├─────────────────────────────────────────┤
│  Service layer (#[rapace::service])     │
├─────────────────────────────────────────┤
│  Session layer (RpcSession)             │
├─────────────────────────────────────────┤
│  Transport layer (ShmTransport)         │
└─────────────────────────────────────────┘
```

It is purely a convenience layer. Everything it does can be done manually using the lower-level APIs if you need more control.
