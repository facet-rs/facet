//! Connection handling for an application-owned Dibs tooling endpoint.

use crate::DbConfig;
use dibs_proto::DibsServiceClient;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tracing::{info, warn};

/// A connection to the dibs service.
pub struct ServiceConnection {
    /// The typed vox client for making calls.
    client: DibsServiceClient,
    /// The migrations directory path
    pub migrations_dir: Option<std::path::PathBuf>,
}

impl ServiceConnection {
    /// Get a typed client for calling service methods.
    pub fn client(&self) -> DibsServiceClient {
        self.client.clone()
    }
}

/// Connect to the application-owned Dibs endpoint specified in the config.
pub async fn connect_to_service(db_config: &DbConfig) -> Result<ServiceConnection, ServiceError> {
    let endpoint = endpoint(db_config)?;
    info!(%endpoint, "Connecting to Dibs tooling endpoint");
    let stream = TcpStream::connect(endpoint).await.map_err(|error| {
        ServiceError::Connection(format!("Failed to connect to {endpoint}: {error}"))
    })?;
    let link = vox_stream::StreamLink::tcp(stream);
    let client = vox::initiator_on(link)
        .establish::<DibsServiceClient>()
        .await
        .map_err(|e| ServiceError::Connection(format!("Vox handshake failed: {}", e)))?;
    observe_service_version(&client).await;

    Ok(ServiceConnection {
        client,
        migrations_dir: migrations_dir(db_config),
    })
}

fn endpoint(db_config: &DbConfig) -> Result<SocketAddr, ServiceError> {
    let endpoint = db_config.endpoint.as_deref().ok_or_else(|| {
        ServiceError::Config(
            "db.endpoint is required; start the application in its Dibs tooling mode and configure its address"
                .to_string(),
        )
    })?;
    endpoint
        .parse()
        .map_err(|error| ServiceError::Config(format!("Invalid db.endpoint {endpoint:?}: {error}")))
}

fn migrations_dir(db_config: &DbConfig) -> Option<std::path::PathBuf> {
    db_config.crate_name.as_ref().and_then(|crate_name| {
        crate::config::find_crate_path(crate_name).map(|path| path.join("src/migrations"))
    })
}

/// Errors that can occur when connecting to the service.
#[derive(Debug)]
pub enum ServiceError {
    /// Configuration error
    Config(String),
    /// Connection error
    Connection(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::Config(e) => write!(f, "Configuration error: {}", e),
            ServiceError::Connection(e) => write!(f, "Connection error: {}", e),
        }
    }
}

impl std::error::Error for ServiceError {}

/// A line of output from the build process.
#[derive(Debug, Clone)]
pub struct BuildOutput {
    /// The text content
    pub text: String,
    /// Whether this came from stderr (vs stdout)
    pub is_stderr: bool,
}

/// A build process that captures output and eventually yields a connection.
pub struct BuildProcess {
    endpoint: SocketAddr,
    migrations_dir: Option<std::path::PathBuf>,
}

impl BuildProcess {
    /// Poll for the next line of output (non-blocking).
    pub fn try_read_line(&mut self) -> Option<BuildOutput> {
        None
    }

    /// Check if the child process has exited (async version).
    pub async fn check_exit(&mut self) -> Option<std::process::ExitStatus> {
        None
    }

    /// Try to connect to the application-owned service.
    ///
    /// Returns `None` if no connection is ready yet.
    pub async fn try_accept(&mut self) -> Result<Option<ServiceConnection>, ServiceError> {
        use tokio::time::{Duration, timeout};

        // Try to accept with a very short timeout (non-blocking feel)
        match timeout(
            Duration::from_millis(100),
            TcpStream::connect(self.endpoint),
        )
        .await
        {
            Ok(Ok(stream)) => {
                let link = vox_stream::StreamLink::tcp(stream);
                let client = vox::initiator_on(link)
                    .establish::<DibsServiceClient>()
                    .await
                    .map_err(|e| {
                        ServiceError::Connection(format!("Vox handshake failed: {}", e))
                    })?;
                observe_service_version(&client).await;

                Ok(Some(ServiceConnection {
                    client,
                    migrations_dir: self.migrations_dir.clone(),
                }))
            }
            Ok(Err(_)) | Err(_) => Ok(None),
        }
    }
}

/// Start connecting to the tooling endpoint, returning a process the TUI can poll.
pub async fn start_service(db_config: &DbConfig) -> Result<BuildProcess, ServiceError> {
    Ok(BuildProcess {
        endpoint: endpoint(db_config)?,
        migrations_dir: migrations_dir(db_config),
    })
}

async fn observe_service_version(client: &DibsServiceClient) {
    match client.dibs_version().await {
        Ok(service_version) => info!(
            cli_version = env!("CARGO_PKG_VERSION"),
            service_version, "Connected to Dibs tooling endpoint"
        ),
        Err(error) => warn!(
            %error,
            "Dibs endpoint did not report package metadata; Vox schema negotiation succeeded"
        ),
    }
}
