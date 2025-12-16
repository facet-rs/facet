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
//! rapace supports both:
//! - **two-peer SHM sessions** (one host ↔ one cell) via `--shm-path=...`
//! - **hub SHM sessions** (one host ↔ many cells) via `--hub-path=... --peer-id=... --doorbell-fd=...`
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
use rapace::{Frame, RpcError, RpcSession, Transport, TransportError};

pub mod lifecycle;
pub use lifecycle::{CellLifecycle, CellLifecycleClient, CellLifecycleServer, ReadyAck, ReadyMsg};

#[cfg(unix)]
use rapace::transport::shm::{Doorbell, HubPeer};
#[cfg(unix)]
use std::os::unix::io::RawFd;

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
    /// Hub file was not created by host within timeout
    HubTimeout(PathBuf),
    /// Failed to open SHM session
    ShmOpen(String),
    /// Failed to open hub session
    HubOpen(String),
    /// Missing or invalid hub arguments
    HubArgs(String),
    /// Doorbell fd invalid
    DoorbellFd(String),
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
            Self::HubTimeout(path) => write!(f, "Hub file not created by host: {}", path.display()),
            Self::ShmOpen(msg) => write!(f, "Failed to open SHM: {}", msg),
            Self::HubOpen(msg) => write!(f, "Failed to open hub: {}", msg),
            Self::HubArgs(msg) => write!(f, "Hub argument error: {}", msg),
            Self::DoorbellFd(msg) => write!(f, "Doorbell fd error: {}", msg),
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
    ) -> impl Fn(Frame) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>>
    + Send
    + Sync
    + 'static {
        let services = Arc::new(self.services);
        move |request: Frame| {
            let services = services.clone();
            Box::pin(async move {
                let method_id = request.desc.method_id;
                let payload = request.payload_bytes();

                // Try each service in order until one doesn't return Unimplemented
                for service in services.iter() {
                    let result = service.dispatch(method_id, payload).await;

                    // If not "unknown method_id", return the result
                    if !matches!(
                        &result,
                        Err(RpcError::Status {
                            code: rapace::ErrorCode::Unimplemented,
                            ..
                        })
                    ) {
                        let mut response = result?;
                        response.desc.channel_id = request.desc.channel_id;
                        response.desc.msg_id = request.desc.msg_id;
                        return Ok(response);
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

enum ParsedArgs {
    Pair {
        shm_path: PathBuf,
    },
    #[cfg(unix)]
    Hub {
        hub_path: PathBuf,
        peer_id: u16,
        doorbell_fd: RawFd,
    },
}

/// Parse CLI arguments to extract either SHM pair args or hub args.
fn parse_args() -> Result<ParsedArgs, CellError> {
    let mut shm_path: Option<PathBuf> = None;
    let mut hub_path: Option<PathBuf> = None;
    let mut peer_id: Option<u16> = None;
    #[cfg(unix)]
    let mut doorbell_fd: Option<RawFd> = None;

    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--shm-path=") {
            shm_path = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--hub-path=") {
            hub_path = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--peer-id=") {
            peer_id = value.parse::<u16>().ok();
        } else if let Some(value) = arg.strip_prefix("--doorbell-fd=") {
            #[cfg(unix)]
            {
                doorbell_fd = value.parse::<i32>().ok();
            }
        } else if !arg.starts_with("--") && shm_path.is_none() && hub_path.is_none() {
            // First positional argument defaults to shm-path for backwards compat.
            shm_path = Some(PathBuf::from(arg));
        }
    }

    if let Some(hub_path) = hub_path {
        #[cfg(not(unix))]
        {
            return Err(CellError::HubArgs(
                "hub mode is only supported on unix platforms".to_string(),
            ));
        }

        #[cfg(unix)]
        {
            let peer_id = peer_id
                .ok_or_else(|| CellError::HubArgs("Missing --peer-id for hub mode".to_string()))?;
            let doorbell_fd = doorbell_fd.ok_or_else(|| {
                CellError::HubArgs("Missing --doorbell-fd for hub mode".to_string())
            })?;
            return Ok(ParsedArgs::Hub {
                hub_path,
                peer_id,
                doorbell_fd,
            });
        }
    }

    if let Some(shm_path) = shm_path {
        return Ok(ParsedArgs::Pair { shm_path });
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

async fn wait_for_hub(path: &std::path::Path, timeout_ms: u64) -> Result<(), CellError> {
    let attempts = timeout_ms / 100;
    for i in 0..attempts {
        if path.exists() {
            return Ok(());
        }
        if i < attempts - 1 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    Err(CellError::HubTimeout(path.to_path_buf()))
}

#[cfg(unix)]
fn validate_doorbell_fd(fd: RawFd) -> Result<(), CellError> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(CellError::DoorbellFd(format!(
            "doorbell fd {fd} is invalid: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn cell_name_guess() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "cell".to_string())
}

/// Cell setup result with optional peer_id for hub mode
struct CellSetup {
    session: Arc<RpcSession>,
    #[allow(dead_code)]
    path: PathBuf,
    /// peer_id is Some when in hub mode, indicating ready signal should be sent
    peer_id: Option<u16>,
}

/// Setup common cell infrastructure
async fn setup_cell(config: ShmSessionConfig) -> Result<CellSetup, CellError> {
    match parse_args()? {
        ParsedArgs::Pair { shm_path } => {
            wait_for_shm(&shm_path, 5000).await?;

            let shm_session = ShmSession::open_file(&shm_path, config)
                .map_err(|e| CellError::ShmOpen(format!("{:?}", e)))?;

            let transport = Transport::Shm(ShmTransport::new(shm_session));
            let session = Arc::new(RpcSession::with_channel_start(
                transport,
                CELL_CHANNEL_START,
            ));
            Ok(CellSetup {
                session,
                path: shm_path,
                peer_id: None,
            })
        }
        #[cfg(unix)]
        ParsedArgs::Hub {
            hub_path,
            peer_id,
            doorbell_fd,
        } => {
            wait_for_hub(&hub_path, 5000).await?;
            validate_doorbell_fd(doorbell_fd)?;

            let peer = HubPeer::open(&hub_path, peer_id)
                .map_err(|e| CellError::HubOpen(format!("{:?}", e)))?;
            peer.register();

            let doorbell = Doorbell::from_raw_fd(doorbell_fd)
                .map_err(|e| CellError::DoorbellFd(format!("{:?}", e)))?;

            let transport = Transport::Shm(ShmTransport::hub_peer(
                Arc::new(peer),
                doorbell,
                cell_name_guess(),
            ));

            let session = Arc::new(RpcSession::with_channel_start(
                transport,
                CELL_CHANNEL_START,
            ));
            Ok(CellSetup {
                session,
                path: hub_path,
                peer_id: Some(peer_id),
            })
        }
    }
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
///
/// When running in hub mode (with `--peer-id`), this automatically sends a
/// `CellLifecycle.ready()` signal to the host after the session is established.
pub async fn run_with_config<S>(service: S, config: ShmSessionConfig) -> Result<(), CellError>
where
    S: ServiceDispatch,
{
    let setup = setup_cell(config).await?;

    tracing::info!("Connected to host via SHM: {}", setup.path.display());

    let session = setup.session;
    let peer_id = setup.peer_id;

    // Set up single-service dispatcher
    session.set_service(service);

    // Start demux loop in background so we can send ready signal
    let run_task = {
        let session = session.clone();
        tokio::spawn(async move { session.run().await })
    };

    // Yield to ensure demux task gets scheduled
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    // Send ready signal if in hub mode
    if let Some(peer_id) = peer_id {
        let client = CellLifecycleClient::new(session.clone());
        let msg = ReadyMsg {
            peer_id,
            cell_name: cell_name_guess(),
            pid: Some(std::process::id()),
            version: None,
            features: vec![],
        };
        tokio::spawn(async move {
            let _ = client.ready(msg).await;
        });
    }

    // Wait for session to complete
    match run_task.await {
        Ok(result) => result?,
        Err(join_err) => {
            return Err(CellError::Transport(TransportError::Io(
                std::io::Error::other(format!("demux task join error: {join_err}")),
            )));
        }
    }

    Ok(())
}

/// Run a single-service cell, but let the service factory access the `RpcSession`.
///
/// This variant is useful for cells that need to make outgoing RPC calls during setup.
/// It starts the demux loop in a background task before invoking `factory`.
pub async fn run_with_session<F, S>(factory: F) -> Result<(), CellError>
where
    F: FnOnce(Arc<RpcSession>) -> S,
    S: ServiceDispatch,
{
    run_with_session_and_config(factory, DEFAULT_SHM_CONFIG).await
}

/// Run a single-service cell with session access and custom SHM configuration.
///
/// When running in hub mode (with `--peer-id`), this automatically sends a
/// `CellLifecycle.ready()` signal to the host after the session is established.
pub async fn run_with_session_and_config<F, S>(
    factory: F,
    config: ShmSessionConfig,
) -> Result<(), CellError>
where
    F: FnOnce(Arc<RpcSession>) -> S,
    S: ServiceDispatch,
{
    let setup = setup_cell(config).await?;

    tracing::info!("Connected to host via SHM: {}", setup.path.display());

    let session = setup.session;
    let peer_id = setup.peer_id;

    // Start demux loop in background, so outgoing RPC calls won't deadlock on
    // current_thread runtimes.
    let run_task = {
        let session = session.clone();
        tokio::spawn(async move { session.run().await })
    };

    // Yield a few times to ensure the demux task gets scheduled.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    // Send ready signal if in hub mode
    if let Some(peer_id) = peer_id {
        let client = CellLifecycleClient::new(session.clone());
        let msg = ReadyMsg {
            peer_id,
            cell_name: cell_name_guess(),
            pid: Some(std::process::id()),
            version: None,
            features: vec![],
        };
        // Fire and forget - spawn so we don't block
        tokio::spawn(async move {
            let _ = client.ready(msg).await;
        });
    }

    let service = factory(session.clone());

    session.set_service(service);

    match run_task.await {
        Ok(result) => result?,
        Err(join_err) => {
            return Err(CellError::Transport(TransportError::Io(
                std::io::Error::other(format!("demux task join error: {join_err}")),
            )));
        }
    }

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
    let setup = setup_cell(config).await?;

    tracing::info!("Connected to host via SHM: {}", setup.path.display());

    let session = setup.session;

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
pub trait RpcSessionExt {
    /// Set a single service as the dispatcher for this session
    ///
    /// This is a convenience method for cells that only expose one service.
    /// For multi-service cells, use `set_dispatcher` with a `DispatcherBuilder`.
    fn set_service<S>(&self, service: S)
    where
        S: ServiceDispatch;
}

impl RpcSessionExt for RpcSession {
    fn set_service<S>(&self, service: S)
    where
        S: ServiceDispatch,
    {
        let service = Arc::new(service);
        let dispatcher = move |request: Frame| {
            let service = service.clone();
            Box::pin(async move {
                let mut response = service
                    .dispatch(request.desc.method_id, request.payload_bytes())
                    .await?;
                response.desc.channel_id = request.desc.channel_id;
                response.desc.msg_id = request.desc.msg_id;
                Ok(response)
            })
        };
        self.set_dispatcher(dispatcher);
    }
}

/// Macro to run a cell with minimal boilerplate.
///
/// Generates a `current_thread` tokio main that calls `rapace_cell::run(...)`.
#[macro_export]
macro_rules! run_cell {
    ($service:expr) => {
        #[tokio::main(flavor = "current_thread")]
        async fn main() -> Result<(), Box<dyn std::error::Error>> {
            $crate::run($service).await?;
            Ok(())
        }
    };
}

/// Macro to run a cell whose setup needs access to the RPC session.
///
/// Generates a `current_thread` tokio main that calls `rapace_cell::run_with_session(...)`.
#[macro_export]
macro_rules! run_cell_with_session {
    ($factory:expr) => {
        #[tokio::main(flavor = "current_thread")]
        async fn main() -> Result<(), Box<dyn std::error::Error>> {
            $crate::run_with_session($factory).await?;
            Ok(())
        }
    };
}

/// Macro to wrap a generated server type as a `ServiceDispatch` cell service.
///
/// This is convenient when a proc-macro generates `FooServer<T>` where `FooServer::new(T)`
/// constructs the server and `FooServer::dispatch(method_id, bytes)` routes calls.
#[macro_export]
macro_rules! cell_service {
    ($server_type:ty, $impl_type:ty) => {
        struct CellService(std::sync::Arc<$server_type>);

        impl $crate::ServiceDispatch for CellService {
            fn dispatch(
                &self,
                method_id: u32,
                payload: &[u8],
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = std::result::Result<::rapace::Frame, ::rapace::RpcError>,
                        > + Send
                        + 'static,
                >,
            > {
                let server = self.0.clone();
                let bytes = payload.to_vec();
                Box::pin(async move { server.dispatch(method_id, &bytes).await })
            }
        }

        impl From<$impl_type> for CellService {
            fn from(impl_val: $impl_type) -> Self {
                Self(std::sync::Arc::new(<$server_type>::new(impl_val)))
            }
        }
    };
}
