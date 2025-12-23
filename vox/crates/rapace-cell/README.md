# rapace-cell

[![crates.io](https://img.shields.io/crates/v/rapace-cell.svg)](https://crates.io/crates/rapace-cell)
[![documentation](https://docs.rs/rapace-cell/badge.svg)](https://docs.rs/rapace-cell)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/rapace-cell.svg)](./LICENSE)

High-level cell runtime for rapace that eliminates boilerplate.

This crate provides simple APIs for building rapace cells that communicate via SHM transport. It handles all the common setup that every cell needs:

- CLI argument parsing (`--shm-path` or positional args)
- Waiting for the host to create the SHM file
- SHM session setup with standard configuration
- RPC session creation with correct channel ID conventions (cells use even IDs)
- Service dispatcher setup

## Before & After

### Before (95+ lines of boilerplate)

```rust,ignore
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{Frame, RpcError, RpcSession};

const SHM_CONFIG: ShmSessionConfig = ShmSessionConfig {
    ring_capacity: 256,
    slot_size: 65536,
    slot_count: 128,
};

struct Args {
    shm_path: PathBuf,
}

fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    args.next();
    let shm_path = match args.next() {
        Some(path) => PathBuf::from(path),
        None => return Err("Usage: cell <shm_path>".into()),
    };
    Ok(Args { shm_path })
}

fn create_dispatcher(
    impl_: MyServiceImpl,
) -> impl Fn(Frame)
    -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>>
    + Send + Sync + 'static
{
    move |frame| {
        let impl_ = impl_.clone();
        Box::pin(async move {
            let server = MyServiceServer::new(impl_);
            server.dispatch(frame.desc.method_id, &frame).await
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;

    // Wait for SHM file
    while !args.shm_path.exists() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    let shm_session = ShmSession::open_file(&args.shm_path, SHM_CONFIG)?;
    let transport = rapace::Transport::from(Arc::new(ShmTransport::new(shm_session)));
    let session = Arc::new(RpcSession::with_channel_start(transport, 2));

    let dispatcher = create_dispatcher(MyServiceImpl);
    session.set_dispatcher(dispatcher);

    session.run().await?;
    Ok(())
}
```

### After (3 lines!)

```rust,ignore
use rapace_cell::run;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(MyServiceServer::new(MyServiceImpl)).await?;
    Ok(())
}
```

## Usage

### Single-service cells

For simple cells that expose a single service:

```rust,ignore
use rapace_cell::{run, ServiceDispatch};
use rapace::{Frame, RpcError};
use std::future::Future;
use std::pin::Pin;

// Your service implementation
#[derive(Clone)]
struct MyServiceImpl;

// Your generated service server
struct MyServiceServer {
    impl_: MyServiceImpl,
}

impl MyServiceServer {
    fn new(impl_: MyServiceImpl) -> Self {
        Self { impl_ }
    }

    async fn dispatch(&self, method_id: u32, frame: &Frame) -> Result<Frame, RpcError> {
        // Your dispatch logic
        todo!()
    }
}

// Implement ServiceDispatch so the cell runtime can use it
impl ServiceDispatch for MyServiceServer {
    fn dispatch(
        &self,
        method_id: u32,
        frame: &Frame,
    ) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
        Box::pin(Self::dispatch(self, method_id, frame))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server = MyServiceServer::new(MyServiceImpl);
    run(server).await?;

    Ok(())
}
```

### Multi-service cells

For cells that expose multiple services:

```rust,ignore
use rapace_cell::{run_multi, DispatcherBuilder, ServiceDispatch};

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

### Using RpcSessionExt for custom setups

If you need more control but still want simplified service setup:

```rust,ignore
use rapace_cell::{RpcSessionExt, DEFAULT_SHM_CONFIG};
use rapace::transport::shm::{ShmSession, ShmTransport};
use rapace::RpcSession;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Your custom setup logic...
    let shm_session = ShmSession::open_file("/tmp/my-app.shm", DEFAULT_SHM_CONFIG)?;
    let transport = rapace::Transport::from(Arc::new(ShmTransport::new(shm_session)));
    let session = Arc::new(RpcSession::with_channel_start(transport, 2));

    // Simple service setup with extension trait
    session.set_service(MyServiceServer::new(MyServiceImpl));

    session.run().await?;
    Ok(())
}
```

## Configuration

The default SHM configuration is:

```rust,ignore
pub const DEFAULT_SHM_CONFIG: ShmSessionConfig = ShmSessionConfig {
    ring_capacity: 256,  // 256 descriptors in flight
    slot_size: 65536,    // 64KB per slot
    slot_count: 128,     // 128 slots = 8MB total
};
```

You can customize this with `run_with_config()` or `run_multi_with_config()`:

```rust,ignore
use rapace_cell::{run_with_config, DEFAULT_SHM_CONFIG};
use rapace::transport::shm::ShmSessionConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let custom_config = ShmSessionConfig {
        ring_capacity: 512,
        slot_size: 131072,  // 128KB
        slot_count: 256,
    };

    run_with_config(MyServiceServer::new(MyServiceImpl), custom_config).await?;
    Ok(())
}
```

## CLI Arguments

The cell runtime accepts the SHM path in two formats:

```bash
# Flag format (recommended)
./my-cell --shm-path=/tmp/my-app.shm

# Positional format
./my-cell /tmp/my-app.shm
```

## Channel ID Conventions

The cell runtime automatically uses the correct channel ID convention:
- **Cells**: Even channel IDs starting from 2 (2, 4, 6, ...)
- **Hosts**: Odd channel IDs starting from 1 (1, 3, 5, ...)

You don't need to worry about this - it's handled automatically.

## Error Handling

The cell runtime provides a `CellError` type that covers common failure modes:

- `CellError::Args` - Invalid command-line arguments
- `CellError::ShmTimeout` - SHM file not created by host within 5 seconds
- `CellError::ShmOpen` - Failed to open SHM session
- `CellError::Rpc` - RPC session error

## Tracing

The cell runtime doesn't configure tracing by default - you should set it up yourself in `main()`:

```rust,ignore
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simple console logging
    tracing_subscriber::fmt::init();

    // Or use rapace-tracing to forward logs to the host
    // (see rapace-tracing documentation)

    run(MyServiceServer::new(MyServiceImpl)).await?;
    Ok(())
}
```

## License

MIT OR Apache-2.0
