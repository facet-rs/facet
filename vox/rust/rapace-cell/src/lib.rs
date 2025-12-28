#![doc = include_str!("../README.md")]

use std::error::Error as StdError;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use rapace::TransportError;
use rapace::transport::shm::ShmTransport;

// Re-export common rapace types so macro-expanded code can refer to `$crate::...`
// without requiring every cell crate to depend on `rapace` directly.
pub use rapace::{BufferPool, Frame, RpcError};

/// Type alias for an SHM-specific RPC session.
///
/// rapace-cell is explicitly designed for shared-memory communication only,
/// so we use the concrete `ShmTransport` type to enable full monomorphization
/// and dead code elimination. This means the compiler generates SHM-specific
/// code with zero abstraction overhead.
pub type CellSession = rapace::RpcSession<ShmTransport>;

pub mod lifecycle;
pub use lifecycle::{CellLifecycle, CellLifecycleClient, CellLifecycleServer, ReadyAck, ReadyMsg};

pub mod tracing_setup;
pub use tracing_setup::TracingConfigService;

#[cfg(unix)]
use rapace::transport::shm::{Doorbell, HubPeer};
#[cfg(unix)]
use std::os::unix::io::RawFd;

fn quiet_mode_enabled() -> bool {
    fn env_truthy(key: &str) -> bool {
        match std::env::var_os(key) {
            None => false,
            Some(v) => {
                let s = v.to_string_lossy();
                !(s.is_empty() || s == "0" || s.eq_ignore_ascii_case("false"))
            }
        }
    }

    // Support both generic and dodeca-specific toggles.
    env_truthy("RAPACE_QUIET") || env_truthy("DODECA_QUIET")
}

/// Channel ID start for cells (even IDs: 2, 4, 6, ...)
/// Hosts use odd IDs (1, 3, 5, ...)
const CELL_CHANNEL_START: u32 = 2;

/// Error type for cell runtime operations
#[derive(Debug)]
pub enum CellError {
    /// Failed to parse command line arguments
    Args(String),
    /// Hub file was not created by host within timeout
    HubTimeout(PathBuf),
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
            Self::HubTimeout(path) => write!(f, "Hub file not created by host: {}", path.display()),
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
    /// Returns the method IDs that this service handles.
    ///
    /// This is used by the dispatcher to build an O(1) lookup table at registration time,
    /// avoiding the need to try each service in sequence at dispatch time.
    fn method_ids(&self) -> &'static [u32];

    /// Dispatch a method call to this service
    fn dispatch(
        &self,
        method_id: u32,
        frame: Frame,
        buffer_pool: &BufferPool,
    ) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>>;
}

/// Trait for types that can handle RPC dispatch.
///
/// This trait is implemented by generated server types (e.g., `FooServer<S>`).
/// Unlike [`ServiceDispatch`], which takes an owned `Frame`, this trait takes
/// a borrowed `&Frame`, making it more ergonomic for implementors.
///
/// Use the blanket impl `impl<T: Dispatchable> ServiceDispatch for Arc<T>` to
/// get automatic `ServiceDispatch` support for any `Arc`-wrapped dispatchable type.
///
/// # Example
///
/// ```ignore
/// // The macro generates this:
/// impl<S: FooService + Send + Sync + 'static> Dispatchable for FooServer<S> {
///     const METHOD_IDS: &'static [u32] = &[METHOD_ID_FOO, METHOD_ID_BAR];
///
///     async fn dispatch(&self, method_id: u32, frame: &Frame, pool: &BufferPool)
///         -> Result<Frame, RpcError>
///     {
///         // dispatch logic...
///     }
/// }
///
/// // And you get ServiceDispatch for Arc<FooServer<S>> for free!
/// let server = Arc::new(FooServer::new(my_impl));
/// dispatcher.add_service(server);
/// ```
pub trait Dispatchable: Send + Sync + 'static {
    /// Method IDs that this dispatchable type handles.
    const METHOD_IDS: &'static [u32];

    /// Dispatch a method call.
    ///
    /// Takes `&Frame` (borrow) rather than owned `Frame` for ergonomics.
    /// The blanket impl for `Arc<T>` handles the ownership conversion.
    fn dispatch(
        &self,
        method_id: u32,
        frame: &Frame,
        buffer_pool: &BufferPool,
    ) -> impl Future<Output = Result<Frame, RpcError>> + Send + '_;
}

/// Blanket impl: Any `Arc<T>` where `T: Dispatchable` automatically implements `ServiceDispatch`.
///
/// This eliminates the need for wrapper types like `FooDispatch<S>`. Instead of:
///
/// ```ignore
/// dispatcher.add_service(FooServer::new(impl).into_dispatch())
/// ```
///
/// You can simply write:
///
/// ```ignore
/// dispatcher.add_service(Arc::new(FooServer::new(impl)))
/// ```
impl<T: Dispatchable> ServiceDispatch for Arc<T> {
    fn method_ids(&self) -> &'static [u32] {
        T::METHOD_IDS
    }

    fn dispatch(
        &self,
        method_id: u32,
        frame: Frame,
        buffer_pool: &BufferPool,
    ) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>> {
        let server = Arc::clone(self);
        let buffer_pool = buffer_pool.clone();
        Box::pin(async move {
            // ServiceDispatch receives owned Frame, pass &Frame to Dispatchable::dispatch
            Dispatchable::dispatch(&*server, method_id, &frame, &buffer_pool).await
        })
    }
}

/// Builder for creating multi-service dispatchers
pub struct DispatcherBuilder {
    services: std::collections::HashMap<u32, Arc<dyn ServiceDispatch>>,
}

impl DispatcherBuilder {
    /// Create a new dispatcher builder
    pub fn new() -> Self {
        Self {
            services: std::collections::HashMap::new(),
        }
    }

    /// Add a service to the dispatcher
    ///
    /// The service's method IDs are registered for O(1) dispatch lookup.
    ///
    /// # Panics
    ///
    /// Panics if any method_id from this service is already registered by another service.
    /// This ensures that method ID collisions are caught at cell startup rather than
    /// silently routing calls to the wrong service.
    pub fn add_service<S>(mut self, service: S) -> Self
    where
        S: ServiceDispatch,
    {
        let service = Arc::new(service);
        for &method_id in service.method_ids() {
            if let Some(_existing) = self.services.insert(method_id, service.clone()) {
                // Method ID collision detected - this is a fatal configuration error
                // Look up method name from registry for better diagnostics
                let method_info = rapace_registry::ServiceRegistry::with_global(|reg| {
                    reg.method_by_id(rapace_registry::MethodId(method_id))
                        .map(|m| format!("'{}' (id={})", m.full_name, method_id))
                });

                let method_desc = method_info.as_deref().unwrap_or("unknown method");

                panic!(
                    "DispatcherBuilder: duplicate registration for method {}. \
                     Each method_id must be unique across all services in a cell. \
                     This likely indicates a hash collision or the same service being added twice. \
                     Hint: Set RUST_BACKTRACE=1 to see which services are conflicting.",
                    method_desc
                );
            }
        }
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
            fn method_ids(&self) -> &'static [u32] {
                use rapace_introspection::{
                    SERVICE_INTROSPECTION_METHOD_ID_DESCRIBE_SERVICE,
                    SERVICE_INTROSPECTION_METHOD_ID_HAS_METHOD,
                    SERVICE_INTROSPECTION_METHOD_ID_LIST_SERVICES,
                };
                &[
                    SERVICE_INTROSPECTION_METHOD_ID_LIST_SERVICES,
                    SERVICE_INTROSPECTION_METHOD_ID_DESCRIBE_SERVICE,
                    SERVICE_INTROSPECTION_METHOD_ID_HAS_METHOD,
                ]
            }

            fn dispatch(
                &self,
                method_id: u32,
                frame: Frame,
                buffer_pool: &BufferPool,
            ) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send + 'static>>
            {
                let buffer_pool = buffer_pool.clone();
                let server = self.0.clone();
                Box::pin(async move { server.dispatch(method_id, &frame, &buffer_pool).await })
            }
        }

        self.add_service(IntrospectionDispatcher(server))
    }

    /// Build the dispatcher function
    ///
    /// Dispatch is O(1) via HashMap lookup - no iteration or frame copying needed.
    #[allow(clippy::type_complexity)]
    pub fn build(
        self,
        buffer_pool: BufferPool,
    ) -> impl Fn(Frame) -> Pin<Box<dyn Future<Output = Result<Frame, RpcError>> + Send>>
    + Send
    + Sync
    + 'static {
        let services = Arc::new(self.services);
        move |request: Frame| {
            let services = services.clone();
            let buffer_pool = buffer_pool.clone();
            Box::pin(async move {
                let method_id = request.desc.method_id;
                let channel_id = request.desc.channel_id;
                let msg_id = request.desc.msg_id;

                // O(1) lookup - no iteration, no frame copying
                if let Some(service) = services.get(&method_id) {
                    let mut response = service.dispatch(method_id, request, &buffer_pool).await?;
                    response.desc.channel_id = channel_id;
                    response.desc.msg_id = msg_id;
                    return Ok(response);
                }

                // No service handles this method - use registry for better error message
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

/// Parsed CLI arguments for hub mode.
#[cfg(unix)]
struct ParsedArgs {
    hub_path: PathBuf,
    peer_id: u16,
    doorbell_fd: RawFd,
}

/// Parse CLI arguments for hub mode.
#[cfg(unix)]
fn parse_args() -> Result<ParsedArgs, CellError> {
    let mut hub_path: Option<PathBuf> = None;
    let mut peer_id: Option<u16> = None;
    let mut doorbell_fd: Option<RawFd> = None;

    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--hub-path=") {
            hub_path = Some(PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--peer-id=") {
            peer_id = value.parse::<u16>().ok();
        } else if let Some(value) = arg.strip_prefix("--doorbell-fd=") {
            doorbell_fd = value.parse::<i32>().ok();
        }
    }

    let hub_path = hub_path.ok_or_else(|| CellError::HubArgs("Missing --hub-path".to_string()))?;
    let peer_id = peer_id.ok_or_else(|| CellError::HubArgs("Missing --peer-id".to_string()))?;
    let doorbell_fd =
        doorbell_fd.ok_or_else(|| CellError::HubArgs("Missing --doorbell-fd".to_string()))?;

    Ok(ParsedArgs {
        hub_path,
        peer_id,
        doorbell_fd,
    })
}

#[cfg(not(unix))]
fn parse_args() -> Result<(), CellError> {
    Err(CellError::Args(
        "rapace-cell requires Unix (hub SHM transport is Unix-only)".to_string(),
    ))
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

fn cell_name_guess() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "cell".to_string())
}

/// Cell setup result
struct CellSetup {
    session: Arc<CellSession>,
    #[allow(dead_code)]
    path: PathBuf,
    peer_id: u16,
}

/// Setup common cell infrastructure
#[cfg(unix)]
async fn setup_cell() -> Result<CellSetup, CellError> {
    // Install death-watch: cell will exit if parent dies
    // Required on macOS for ur-taking-me-with-you to work
    ur_taking_me_with_you::die_with_parent();

    let args = parse_args()?;

    wait_for_hub(&args.hub_path, 5000).await?;
    validate_doorbell_fd(args.doorbell_fd)?;

    let peer = HubPeer::open(&args.hub_path, args.peer_id)
        .map_err(|e| CellError::HubOpen(format!("{:?}", e)))?;
    peer.register();

    let doorbell = Doorbell::from_raw_fd(args.doorbell_fd)
        .map_err(|e| CellError::DoorbellFd(format!("{:?}", e)))?;

    let transport = ShmTransport::hub_peer(Arc::new(peer), doorbell, cell_name_guess());

    let session = Arc::new(CellSession::with_channel_start(
        transport,
        CELL_CHANNEL_START,
    ));
    Ok(CellSetup {
        session,
        path: args.hub_path,
        peer_id: args.peer_id,
    })
}

#[cfg(not(unix))]
async fn setup_cell() -> Result<CellSetup, CellError> {
    Err(CellError::Args(
        "rapace-cell requires Unix (hub SHM transport is Unix-only)".to_string(),
    ))
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
    let setup = setup_cell().await?;

    let session = setup.session;
    let peer_id = setup.peer_id;
    let cell_name = cell_name_guess();

    // Expose TracingConfig early (so the host can push filters), but don't install
    // the forwarding tracing layer until after the ready handshake.
    let (tracing_filter, tracing_service) = tracing_setup::create_tracing_config_service();

    // IMPORTANT: Set up dispatcher before starting demux.
    // We intentionally delay installing the tracing layer until after ready, to avoid
    // startup floods on contended transports.
    session.set_dispatcher(
        DispatcherBuilder::new()
            .add_service(tracing_service)
            .add_service(service)
            .build(session.buffer_pool().clone()),
    );

    // Start demux loop in background so we can send ready signal
    let run_task = {
        let session = session.clone();
        tokio::spawn(async move { session.run().await })
    };

    // Yield to ensure demux task gets scheduled
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    // Send ready signal FIRST, before tracing is initialized
    let client = CellLifecycleClient::new(session.clone());
    let msg = ReadyMsg {
        peer_id,
        cell_name: cell_name.clone(),
        pid: Some(std::process::id()),
        version: None,
        features: vec![],
    };
    if !quiet_mode_enabled() {
        eprintln!(
            "[rapace-cell] {} (peer_id={}) sending ready signal...",
            cell_name, peer_id
        );
    }
    // Retry the handshake; hub slot allocation can be temporarily contended during parallel startup.
    match ready_handshake_with_backoff(&client, msg).await {
        Ok(ack) => {
            if !quiet_mode_enabled() {
                eprintln!(
                    "[rapace-cell] {} (peer_id={}) ready acknowledged: ok={}",
                    cell_name, peer_id, ack.ok
                );
            }
        }
        Err(e) => {
            if !quiet_mode_enabled() {
                eprintln!(
                    "[rapace-cell] {} (peer_id={}) ready FAILED: {:?}",
                    cell_name, peer_id, e
                );
            }
        }
    }

    // NOW initialize tracing (after ready signal is confirmed)
    tracing_setup::install_tracing_layer(session.clone(), tracing_filter);
    tracing::debug!(target: "cell", cell = %cell_name, "Connected to host via SHM: {}", setup.path.display());

    // Wait for session to complete
    match run_task.await {
        Ok(result) => result?,
        Err(join_err) => {
            return Err(CellError::Transport(TransportError::Io(
                std::io::Error::other(format!("demux task join error: {join_err}")).into(),
            )));
        }
    }

    Ok(())
}

/// Run a single-service cell, but let the service factory access the `CellSession`.
///
/// This variant is useful for cells that need to make outgoing RPC calls during setup.
/// It starts the demux loop in a background task before invoking `factory`.
pub async fn run_with_session<F, S>(factory: F) -> Result<(), CellError>
where
    F: FnOnce(Arc<CellSession>) -> S,
    S: ServiceDispatch,
{
    let setup = setup_cell().await?;

    let session = setup.session;
    let peer_id = setup.peer_id;
    let cell_name = cell_name_guess();

    // Create service from factory (needs session)
    let service = factory(session.clone());

    // Expose TracingConfig early (so the host can push filters), but don't install
    // the forwarding tracing layer until after the ready handshake.
    let (tracing_filter, tracing_service) = tracing_setup::create_tracing_config_service();

    // IMPORTANT: Set up dispatcher before starting demux.
    session.set_dispatcher(
        DispatcherBuilder::new()
            .add_service(tracing_service)
            .add_service(service)
            .build(session.buffer_pool().clone()),
    );

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

    // Send ready signal FIRST, before tracing is initialized
    let client = CellLifecycleClient::new(session.clone());
    let msg = ReadyMsg {
        peer_id,
        cell_name: cell_name.clone(),
        pid: Some(std::process::id()),
        version: None,
        features: vec![],
    };
    if !quiet_mode_enabled() {
        eprintln!(
            "[rapace-cell] {} (peer_id={}) sending ready signal...",
            cell_name, peer_id
        );
    }
    // Retry the handshake; hub slot allocation can be temporarily contended during parallel startup.
    match ready_handshake_with_backoff(&client, msg).await {
        Ok(ack) => {
            if !quiet_mode_enabled() {
                eprintln!(
                    "[rapace-cell] {} (peer_id={}) ready acknowledged: ok={}",
                    cell_name, peer_id, ack.ok
                );
            }
        }
        Err(e) => {
            if !quiet_mode_enabled() {
                eprintln!(
                    "[rapace-cell] {} (peer_id={}) ready FAILED: {:?}",
                    cell_name, peer_id, e
                );
            }
        }
    }

    // NOW initialize tracing (after ready signal is confirmed)
    tracing_setup::install_tracing_layer(session.clone(), tracing_filter);
    tracing::debug!(target: "cell", cell = %cell_name, "Connected to host via SHM: {}", setup.path.display());

    match run_task.await {
        Ok(result) => result?,
        Err(join_err) => {
            return Err(CellError::Transport(TransportError::Io(
                std::io::Error::other(format!("demux task join error: {join_err}")).into(),
            )));
        }
    }

    Ok(())
}

fn ready_total_timeout() -> std::time::Duration {
    // Keep compatibility with dodeca's historical knob while providing a generic name too.
    let timeout_ms = std::env::var("RAPACE_CELL_READY_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| {
            std::env::var("DODECA_CELL_READY_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
        })
        .unwrap_or(10_000);

    std::time::Duration::from_millis(timeout_ms)
}

async fn ready_handshake_with_backoff(
    client: &CellLifecycleClient<ShmTransport>,
    msg: ReadyMsg,
) -> Result<ReadyAck, RpcError> {
    let timeout = ready_total_timeout();

    let start = std::time::Instant::now();
    let mut delay_ms = 10u64;

    loop {
        match client.ready(msg.clone()).await {
            Ok(ack) => return Ok(ack),
            Err(e) => {
                if start.elapsed() >= timeout {
                    return Err(e);
                }
                tracing::debug!(
                    cell = %msg.cell_name,
                    peer_id = msg.peer_id,
                    error = ?e,
                    delay_ms,
                    "Ready handshake failed; retrying"
                );
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        delay_ms = (delay_ms * 2).min(200);
    }
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
    let setup = setup_cell().await?;

    let session = setup.session;
    let peer_id = setup.peer_id;
    let cell_name = cell_name_guess();

    // Expose TracingConfig early (so the host can push filters), but don't install
    // the forwarding tracing layer until after the ready handshake.
    let (tracing_filter, tracing_service) = tracing_setup::create_tracing_config_service();

    // Build the dispatcher with user services + tracing config
    let builder = DispatcherBuilder::new();
    let builder = builder_fn(builder);
    let builder = builder.add_service(tracing_service);
    let dispatcher = builder.build(session.buffer_pool().clone());

    session.set_dispatcher(dispatcher);

    // Start demux loop in background so we can send ready signal
    let run_task = {
        let session = session.clone();
        tokio::spawn(async move { session.run().await })
    };

    // Yield a few times to ensure the demux task gets scheduled.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    // Send ready signal
    let client = CellLifecycleClient::new(session.clone());
    let msg = ReadyMsg {
        peer_id,
        cell_name: cell_name.clone(),
        pid: Some(std::process::id()),
        version: None,
        features: vec![],
    };
    if !quiet_mode_enabled() {
        eprintln!(
            "[rapace-cell] {} (peer_id={}) sending ready signal...",
            cell_name, peer_id
        );
    }
    match ready_handshake_with_backoff(&client, msg).await {
        Ok(ack) => {
            if !quiet_mode_enabled() {
                eprintln!(
                    "[rapace-cell] {} (peer_id={}) ready acknowledged: ok={}",
                    cell_name, peer_id, ack.ok
                );
            }
        }
        Err(e) => {
            if !quiet_mode_enabled() {
                eprintln!(
                    "[rapace-cell] {} (peer_id={}) ready FAILED: {:?}",
                    cell_name, peer_id, e
                );
            }
        }
    }

    // Install tracing forwarding now that the cell is ready.
    tracing_setup::install_tracing_layer(session.clone(), tracing_filter);
    tracing::debug!(target: "cell", cell = %cell_name, "Connected to host via SHM: {}", setup.path.display());

    // Wait for session to complete
    match run_task.await {
        Ok(result) => result?,
        Err(join_err) => {
            return Err(CellError::Transport(TransportError::Io(
                std::io::Error::other(format!("demux task join error: {join_err}")).into(),
            )));
        }
    }

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

impl RpcSessionExt for CellSession {
    fn set_service<S>(&self, service: S)
    where
        S: ServiceDispatch,
    {
        let service = Arc::new(service);
        let buffer_pool = self.buffer_pool().clone();
        let dispatcher = move |request: Frame| {
            let service = service.clone();
            let buffer_pool = buffer_pool.clone();
            let method_id = request.desc.method_id;
            let channel_id = request.desc.channel_id;
            let msg_id = request.desc.msg_id;
            Box::pin(async move {
                let mut response = service.dispatch(method_id, request, &buffer_pool).await?;
                response.desc.channel_id = channel_id;
                response.desc.msg_id = msg_id;
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
        fn main() -> Result<(), Box<dyn std::error::Error>> {
            let rt = ::tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                $crate::run($service).await?;
                Ok(())
            })
        }
    };
}

/// Macro to run a cell whose setup needs access to the RPC session.
///
/// Generates a `current_thread` tokio main that calls `rapace_cell::run_with_session(...)`.
#[macro_export]
macro_rules! run_cell_with_session {
    ($factory:expr) => {
        fn main() -> Result<(), Box<dyn std::error::Error>> {
            let rt = ::tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                $crate::run_with_session($factory).await?;
                Ok(())
            })
        }
    };
}

/// Macro to wrap a generated server type as a `ServiceDispatch` cell service.
///
/// This is convenient when a proc-macro generates `FooServer<T>` where `FooServer::new(T)`
/// constructs the server and `FooServer::dispatch(method_id, bytes)` routes calls.
///
/// The `#[rapace::service]` macro automatically generates a `METHOD_IDS` constant
/// on the server type, which this macro uses for registration with the dispatcher.
///
/// # Arguments
///
/// - `$server_type`: The generated server type (e.g., `CalculatorServer<MyImpl>`)
/// - `$impl_type`: The implementation type (e.g., `MyImpl`)
///
/// # Example
///
/// ```ignore
/// cell_service!(
///     CalculatorServer<MyCalculatorImpl>,
///     MyCalculatorImpl
/// );
/// ```
#[macro_export]
macro_rules! cell_service {
    ($server_type:ty, $impl_type:ty) => {
        struct CellService(std::sync::Arc<$server_type>);

        impl $crate::ServiceDispatch for CellService {
            fn method_ids(&self) -> &'static [u32] {
                <$server_type>::METHOD_IDS
            }

            fn dispatch(
                &self,
                method_id: u32,
                frame: $crate::Frame,
                buffer_pool: &$crate::BufferPool,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = std::result::Result<$crate::Frame, $crate::RpcError>,
                        > + Send
                        + 'static,
                >,
            > {
                let server = self.0.clone();
                let buffer_pool = buffer_pool.clone();
                // Frame is now owned - no copying needed!
                // Use fully-qualified syntax to call the inherent dispatch method.
                Box::pin(async move {
                    <$server_type>::dispatch(&*server, method_id, &frame, &buffer_pool).await
                })
            }
        }

        impl From<$impl_type> for CellService {
            fn from(impl_val: $impl_type) -> Self {
                Self(std::sync::Arc::new(<$server_type>::new(impl_val)))
            }
        }
    };
}
