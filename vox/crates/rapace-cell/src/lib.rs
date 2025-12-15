//! High-level cell runtime for rapace
//!
//! This crate provides boilerplate-free APIs for building rapace cells (plugins) that
//! communicate with a host via shared memory (SHM) transport. It handles all the common
//! setup that every cell needs:
//!
//! - CLI argument parsing (`--shm-path` or positional args)
//! - Waiting for the host to create the SHM file
//! - SHM session setup and transport initialization
//! - RPC session creation with correct channel ID conventions (cells use even IDs)
//! - Service dispatcher setup for handling incoming requests
//!
//! # Architecture Note
//!
//! Currently supports **two-peer SHM sessions** only (one host ↔ one cell). For details on
//! the SHM architecture, see [`rapace-transport-shm/ARCHITECTURE.md`](../rapace_transport_shm/ARCHITECTURE.md).
//!
//! **Future**: Hub architecture support for multi-cell scenarios is planned (one host ↔ many cells).
//!
//! # Single-service cells
//!
//! For simple cells that expose a single service:
//!
//! ```rust,ignore
//! use rapace_cell::{run, ServiceDispatch};
//! use rapace::{Frame, RpcError};
//! use std::future::Future;
//! use std::pin::Pin;
//!
//! # struct MyServiceServer;
//! # impl MyServiceServer {
//! #     fn new(impl_: ()) -> Self { Self }
//! #     async fn dispatch_impl(&self, method_id: u32, payload: &[u8]) -> Result<Frame, RpcError> {
//! #         unimplemented!()
//! #     }
//! # }
//! # impl ServiceDispatch for MyServiceServer {
//! #     fn dispatch(&self, method_id: u32, payload: &[u8]) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
//! #         Box::pin(Self::dispatch_impl(self, method_id, payload))
//! #     }
//! # }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let server = MyServiceServer::new(());
//!     run(server).await?;
//!     Ok(())
//! }
//! ```
//!
//! # Multi-service cells
//!
//! For cells that expose multiple services:
//!
//! ```rust,ignore
//! use rapace_cell::{run_multi, DispatcherBuilder, ServiceDispatch};
//! use rapace::{Frame, RpcError};
//! use std::future::Future;
//! use std::pin::Pin;
//!
//! # struct MyServiceServer;
//! # struct AnotherServiceServer;
//! # impl MyServiceServer {
//! #     fn new(impl_: ()) -> Self { Self }
//! #     async fn dispatch_impl(&self, method_id: u32, payload: &[u8]) -> Result<Frame, RpcError> {
//! #         unimplemented!()
//! #     }
//! # }
//! # impl AnotherServiceServer {
//! #     fn new(impl_: ()) -> Self { Self }
//! #     async fn dispatch_impl(&self, method_id: u32, payload: &[u8]) -> Result<Frame, RpcError> {
//! #         unimplemented!()
//! #     }
//! # }
//! # impl ServiceDispatch for MyServiceServer {
//! #     fn dispatch(&self, method_id: u32, payload: &[u8]) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
//! #         Box::pin(Self::dispatch_impl(self, method_id, payload))
//! #     }
//! # }
//! # impl ServiceDispatch for AnotherServiceServer {
//! #     fn dispatch(&self, method_id: u32, payload: &[u8]) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
//! #         Box::pin(Self::dispatch_impl(self, method_id, payload))
//! #     }
//! # }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     run_multi(|builder| {
//!         builder
//!             .add_service(MyServiceServer::new(()))
//!             .add_service(AnotherServiceServer::new(()))
//!     }).await?;
//!     Ok(())
//! }
//! ```
//!
//! # Configuration
//!
//! ## Default SHM Configuration
//!
//! The default configuration is designed for typical host-cell communication with moderate
//! payload sizes:
//!
//! ```text
//! ring_capacity: 256 descriptors  (256 in-flight requests per direction)
//! slot_size: 64KB                 (maximum payload size without fragmentation)
//! slot_count: 128 slots           (8MB total data segment)
//! ```
//!
//! This allocates approximately **~8.5MB per SHM session** (2 rings + data segment).
//!
//! ## When to Customize
//!
//! Use `run_with_config()` or `run_multi_with_config()` to override defaults:
//!
//! - **Large payloads** (images, video): Increase `slot_size` to 256KB or more
//! - **High throughput**: Increase `slot_count` for more concurrent transfers
//! - **Low latency**: Increase `ring_capacity` to reduce descriptor exhaustion
//! - **Memory constrained**: Decrease `slot_count` or `slot_size`
//!
//! ## Channel ID Convention
//!
//! Cells always use **even channel IDs** starting from 2 (following rapace convention where
//! cells use even IDs and hosts use odd IDs). This prevents collisions in bidirectional RPC.

use std::error::Error as StdError;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{Frame, RpcError, RpcSession, TransportError};

/// Default SHM configuration for two-peer sessions.
///
/// Designed for typical host-cell communication with moderate payloads.
/// Total memory per session: ~8.5MB (2 × 17KB rings + 8MB data segment).
///
/// See module documentation for customization guidelines.
pub const DEFAULT_SHM_CONFIG: ShmSessionConfig = ShmSessionConfig {
    ring_capacity: 256, // 256 in-flight descriptors per direction
    slot_size: 65536,   // 64KB max payload per slot
    slot_count: 128,    // 128 slots = 8MB total data segment
};

/// Channel ID start for cells (even IDs: 2, 4, 6, ...)
/// Hosts use odd IDs (1, 3, 5, ...)
const CELL_CHANNEL_START: u32 = 2;

/// Error type for cell runtime operations
#[derive(Debug)]
pub enum CellError {
    /// Failed to parse command line arguments
    Args(String),
    /// SHM file was not created by host within timeout
    ShmTimeout(PathBuf),
    /// Failed to open SHM session
    ShmOpen(String),
    /// RPC session error
    Rpc(RpcError),
    /// Transport error
    Transport(TransportError),
}

impl std::fmt::Display for CellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Args(msg) => write!(f, "Argument error: {}", msg),
            Self::ShmTimeout(path) => write!(f, "SHM file not created by host: {}", path.display()),
            Self::ShmOpen(msg) => write!(f, "Failed to open SHM: {}", msg),
            Self::Rpc(e) => write!(f, "RPC error: {:?}", e),
            Self::Transport(e) => write!(f, "Transport error: {:?}", e),
        }
    }
}

impl StdError for CellError {}

impl From<RpcError> for CellError {
    fn from(e: RpcError) -> Self {
        Self::Rpc(e)
    }
}

impl From<TransportError> for CellError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e)
    }
}

/// Trait for service servers that can be dispatched
pub trait ServiceDispatch: Send + Sync + 'static {
    /// Dispatch a method call to this service
    fn dispatch(
        &self,
        method_id: u32,
        payload: &[u8],
    ) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>>;
}

/// Builder for creating multi-service dispatchers
pub struct DispatcherBuilder {
    services: Vec<Box<dyn ServiceDispatch>>,
}

impl DispatcherBuilder {
    /// Create a new dispatcher builder
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    /// Add a service to the dispatcher
    pub fn add_service<S>(mut self, service: S) -> Self
    where
        S: ServiceDispatch,
    {
        self.services.push(Box::new(service));
        self
    }

    /// Add service introspection to this cell.
    ///
    /// This exposes the `ServiceIntrospection` service, allowing callers to
    /// query what services and methods this cell provides at runtime.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rapace_cell::run_multi;
    ///
    /// run_multi(|builder| {
    ///     builder
    ///         .add_service(MyServiceServer::new(my_impl))
    ///         .with_introspection() // ← Add introspection!
    /// }).await?;
    /// ```
    #[cfg(feature = "introspection")]
    pub fn with_introspection(self) -> Self {
        use rapace_introspection::{DefaultServiceIntrospection, ServiceIntrospectionServer};

        let introspection = DefaultServiceIntrospection::new();
        let server = Arc::new(ServiceIntrospectionServer::new(introspection));

        // Wrap the generated server to implement ServiceDispatch
        struct IntrospectionDispatcher(
            Arc<ServiceIntrospectionServer<DefaultServiceIntrospection>>,
        );

        impl ServiceDispatch for IntrospectionDispatcher {
            fn dispatch(
                &self,
                method_id: u32,
                payload: &[u8],
            ) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>>
            {
                // Clone payload and capture server Arc for the future
                let payload_owned = payload.to_vec();
                let server = self.0.clone();
                Box::pin(async move { server.dispatch(method_id, &payload_owned).await })
            }
        }

        self.add_service(IntrospectionDispatcher(server))
    }

    /// Build the dispatcher function
    #[allow(clippy::type_complexity)]
    pub fn build(
        self,
    ) -> impl Fn(u32, u32, Vec<u8>) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>>
    + Send
    + Sync
    + 'static {
        let services = Arc::new(self.services);
        move |_channel_id, method_id, payload| {
            let services = services.clone();
            Box::pin(async move {
                // Try each service in order until one doesn't return Unimplemented
                for service in services.iter() {
                    let result = service.dispatch(method_id, &payload).await;

                    // If not "unknown method_id", return the result
                    if !matches!(
                        &result,
                        Err(RpcError::Status {
                            code: rapace::ErrorCode::Unimplemented,
                            ..
                        })
                    ) {
                        return result;
                    }
                }

                // No service handled this method - use registry for better error message
                let error_msg = rapace_registry::ServiceRegistry::with_global(|reg| {
                    if let Some(method) = reg.method_by_id(rapace_registry::MethodId(method_id)) {
                        format!(
                            "Method '{}' (id={}) exists in registry but is not implemented by any service in this cell",
                            method.full_name, method_id
                        )
                    } else {
                        format!(
                            "Unknown method_id: {} (not registered in global registry)",
                            method_id
                        )
                    }
                });

                Err(RpcError::Status {
                    code: rapace::ErrorCode::Unimplemented,
                    message: error_msg,
                })
            })
        }
    }
}

impl Default for DispatcherBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse CLI arguments to extract SHM path
fn parse_args() -> Result<PathBuf, CellError> {
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--shm-path=") {
            return Ok(PathBuf::from(value));
        } else if !arg.starts_with("--") {
            // First positional argument
            return Ok(PathBuf::from(arg));
        }
    }
    Err(CellError::Args(
        "Missing SHM path (use --shm-path=PATH or provide as first argument)".to_string(),
    ))
}

/// Wait for the host to create the SHM file
async fn wait_for_shm(path: &std::path::Path, timeout_ms: u64) -> Result<(), CellError> {
    let attempts = timeout_ms / 100;
    for i in 0..attempts {
        if path.exists() {
            return Ok(());
        }
        if i < attempts - 1 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    Err(CellError::ShmTimeout(path.to_path_buf()))
}

/// Setup common cell infrastructure
async fn setup_cell(
    config: ShmSessionConfig,
) -> Result<(Arc<RpcSession<ShmTransport>>, PathBuf), CellError> {
    // Parse CLI args
    let shm_path = parse_args()?;

    // Wait for host to create SHM file (5 second timeout)
    wait_for_shm(&shm_path, 5000).await?;

    // Open the SHM session
    let shm_session = ShmSession::open_file(&shm_path, config)
        .map_err(|e| CellError::ShmOpen(format!("{:?}", e)))?;

    // Create SHM transport
    let transport = Arc::new(ShmTransport::new(shm_session));

    // Create RPC session with cell channel start (even IDs)
    let session = Arc::new(RpcSession::with_channel_start(
        transport,
        CELL_CHANNEL_START,
    ));

    Ok((session, shm_path))
}

/// Run a single-service cell
///
/// This function handles all the boilerplate for a simple cell:
/// - Parses CLI arguments
/// - Waits for SHM file creation
/// - Sets up SHM transport and RPC session
/// - Configures the service dispatcher
/// - Runs the session loop
///
/// # Example
///
/// ```rust,ignore
/// use rapace_cell::{run, ServiceDispatch};
/// use rapace::{Frame, RpcError};
/// use std::future::Future;
/// use std::pin::Pin;
///
/// # struct MyServiceServer;
/// # impl MyServiceServer {
/// #     fn new(impl_: ()) -> Self { Self }
/// #     async fn dispatch_impl(&self, method_id: u32, payload: &[u8]) -> Result<Frame, RpcError> {
/// #         unimplemented!()
/// #     }
/// # }
/// # impl ServiceDispatch for MyServiceServer {
/// #     fn dispatch(&self, method_id: u32, payload: &[u8]) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
/// #         Box::pin(Self::dispatch_impl(self, method_id, payload))
/// #     }
/// # }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let server = MyServiceServer::new(());
///     run(server).await?;
///     Ok(())
/// }
/// ```
pub async fn run<S>(service: S) -> Result<(), CellError>
where
    S: ServiceDispatch,
{
    run_with_config(service, DEFAULT_SHM_CONFIG).await
}

/// Run a single-service cell with custom SHM configuration
pub async fn run_with_config<S>(service: S, config: ShmSessionConfig) -> Result<(), CellError>
where
    S: ServiceDispatch,
{
    let (session, shm_path) = setup_cell(config).await?;

    tracing::info!("Connected to host via SHM: {}", shm_path.display());

    // Set up single-service dispatcher
    let dispatcher = {
        let service = Arc::new(service);
        move |_channel_id: u32, method_id: u32, payload: Vec<u8>| {
            let service = service.clone();
            Box::pin(async move { service.dispatch(method_id, &payload).await })
                as Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>>
        }
    };

    session.set_dispatcher(dispatcher);

    // Run the session loop
    session.run().await?;

    Ok(())
}

/// Run a multi-service cell
///
/// This function handles all the boilerplate for a multi-service cell.
/// The builder function receives a `DispatcherBuilder` to configure which
/// services the cell exposes.
///
/// # Example
///
/// ```rust,ignore
/// use rapace_cell::{run_multi, DispatcherBuilder, ServiceDispatch};
/// use rapace::{Frame, RpcError};
/// use std::future::Future;
/// use std::pin::Pin;
///
/// # struct MyServiceServer;
/// # struct AnotherServiceServer;
/// # impl MyServiceServer {
/// #     fn new(impl_: ()) -> Self { Self }
/// #     async fn dispatch_impl(&self, method_id: u32, payload: &[u8]) -> Result<Frame, RpcError> {
/// #         unimplemented!()
/// #     }
/// # }
/// # impl AnotherServiceServer {
/// #     fn new(impl_: ()) -> Self { Self }
/// #     async fn dispatch_impl(&self, method_id: u32, payload: &[u8]) -> Result<Frame, RpcError> {
/// #         unimplemented!()
/// #     }
/// # }
/// # impl ServiceDispatch for MyServiceServer {
/// #     fn dispatch(&self, method_id: u32, payload: &[u8]) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
/// #         Box::pin(Self::dispatch_impl(self, method_id, payload))
/// #     }
/// # }
/// # impl ServiceDispatch for AnotherServiceServer {
/// #     fn dispatch(&self, method_id: u32, payload: &[u8]) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
/// #         Box::pin(Self::dispatch_impl(self, method_id, payload))
/// #     }
/// # }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     run_multi(|builder| {
///         builder
///             .add_service(MyServiceServer::new(()))
///             .add_service(AnotherServiceServer::new(()))
///     }).await?;
///     Ok(())
/// }
/// ```
pub async fn run_multi<F>(builder_fn: F) -> Result<(), CellError>
where
    F: FnOnce(DispatcherBuilder) -> DispatcherBuilder,
{
    run_multi_with_config(builder_fn, DEFAULT_SHM_CONFIG).await
}

/// Run a multi-service cell with custom SHM configuration
pub async fn run_multi_with_config<F>(
    builder_fn: F,
    config: ShmSessionConfig,
) -> Result<(), CellError>
where
    F: FnOnce(DispatcherBuilder) -> DispatcherBuilder,
{
    let (session, shm_path) = setup_cell(config).await?;

    tracing::info!("Connected to host via SHM: {}", shm_path.display());

    // Build the dispatcher
    let builder = DispatcherBuilder::new();
    let builder = builder_fn(builder);
    let dispatcher = builder.build();

    session.set_dispatcher(dispatcher);

    // Run the session loop
    session.run().await?;

    Ok(())
}

/// Extension trait for RpcSession to support single-service setup
pub trait RpcSessionExt<T> {
    /// Set a single service as the dispatcher for this session
    ///
    /// This is a convenience method for cells that only expose one service.
    /// For multi-service cells, use `set_dispatcher` with a `DispatcherBuilder`.
    fn set_service<S>(&self, service: S)
    where
        S: ServiceDispatch;
}

impl<T> RpcSessionExt<T> for RpcSession<T>
where
    T: rapace::Transport + 'static,
{
    fn set_service<S>(&self, service: S)
    where
        S: ServiceDispatch,
    {
        let service = Arc::new(service);
        let dispatcher = move |_channel_id: u32, method_id: u32, payload: Vec<u8>| {
            let service = service.clone();
            Box::pin(async move { service.dispatch(method_id, &payload).await })
                as Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>>
        };
        self.set_dispatcher(dispatcher);
    }
}
