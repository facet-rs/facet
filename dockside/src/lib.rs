//! Minimal Docker CLI wrapper for testing.
//!
//! No serde, no bollard - just shells out to the `docker` CLI.
//!
//! Features:
//! - Automatic cleanup via reaper container (survives crashes)
//! - Ephemeral host ports (no conflicts in parallel tests)
//! - Wait for log patterns or TCP port ready
//!
//! # Example
//!
//! ```no_run
//! use dockside::{Container, Image};
//!
//! let container = Container::run(
//!     Image::new("postgres", "16-alpine")
//!         .env("POSTGRES_PASSWORD", "test")
//!         .port(5432)
//! ).unwrap();
//!
//! let host_port = container.host_port(5432).unwrap();
//! println!("Postgres available at localhost:{}", host_port);
//!
//! // Container is automatically removed on drop.
//! // If process crashes, reaper container cleans up.
//! ```

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Session state - reaper process and session ID.
struct Session {
    id: String,
    #[allow(dead_code)]
    reaper: Child,
}

static SESSION: OnceLock<Session> = OnceLock::new();

/// Get or initialize the session (starts reaper on first call).
fn session() -> &'static Session {
    SESSION.get_or_init(|| {
        let id = format!("{}-{}", std::process::id(), timestamp_ms());

        // Start reaper container that monitors our stdin
        // When stdin closes (we die), it cleans up all our containers
        let reaper_script = format!(
            r#"cat >/dev/null; docker rm -f $(docker ps -q --filter label=dockside.session={}) 2>/dev/null; true"#,
            id
        );

        let reaper = Command::new("docker")
            .args([
                "run",
                "--rm",
                "-i", // interactive - monitors stdin
                "-v", "/var/run/docker.sock:/var/run/docker.sock",
                "docker:cli",
                "sh", "-c", &reaper_script,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start reaper container");

        Session { id, reaper }
    })
}

fn timestamp_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

/// Error type for docker operations.
#[derive(Debug)]
pub enum Error {
    /// Docker command failed.
    Command { cmd: String, stderr: String },
    /// Docker CLI not found or not executable.
    DockerNotFound,
    /// Timeout waiting for container.
    Timeout { message: String },
    /// I/O error.
    Io(std::io::Error),
    /// Failed to parse docker output.
    Parse(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Command { cmd, stderr } => write!(f, "docker command failed: {}\n{}", cmd, stderr),
            Error::DockerNotFound => write!(f, "docker CLI not found"),
            Error::Timeout { message } => write!(f, "timeout: {}", message),
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::Parse(msg) => write!(f, "failed to parse docker output: {}", msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Result type for docker operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Docker image specification.
#[derive(Debug, Clone)]
pub struct Image {
    name: String,
    tag: String,
    env: HashMap<String, String>,
    ports: Vec<u16>,
    cmd: Option<Vec<String>>,
}

impl Image {
    /// Create a new image specification.
    pub fn new(name: impl Into<String>, tag: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tag: tag.into(),
            env: HashMap::new(),
            ports: Vec::new(),
            cmd: None,
        }
    }

    /// Add an environment variable.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Expose a port.
    pub fn port(mut self, port: u16) -> Self {
        self.ports.push(port);
        self
    }

    /// Set the command to run.
    pub fn cmd(mut self, cmd: Vec<String>) -> Self {
        self.cmd = Some(cmd);
        self
    }

    /// Get the full image reference (name:tag).
    pub fn reference(&self) -> String {
        format!("{}:{}", self.name, self.tag)
    }
}

/// A running Docker container.
///
/// The container is automatically removed when this struct is dropped.
/// If the process crashes, the reaper container will clean it up.
pub struct Container {
    id: String,
    port_mappings: HashMap<u16, u16>,
}

impl Container {
    /// Run a container from the given image.
    pub fn run(image: Image) -> Result<Self> {
        // Ensure reaper is running
        let session = session();

        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("-d") // detached
            .arg("--rm") // remove on stop
            .arg("--label").arg(format!("dockside.session={}", session.id));

        // Add environment variables
        for (key, value) in &image.env {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        // Add port mappings (let Docker assign random host ports)
        for port in &image.ports {
            cmd.arg("-p").arg(port.to_string());
        }

        // Image reference
        cmd.arg(image.reference());

        // Optional command
        if let Some(args) = &image.cmd {
            cmd.args(args);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            return Err(Error::Command {
                cmd: format!("{:?}", cmd),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        let id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Get port mappings
        let mut port_mappings = HashMap::new();
        for port in &image.ports {
            if let Ok(host_port) = Self::get_host_port(&id, *port) {
                port_mappings.insert(*port, host_port);
            }
        }

        Ok(Self { id, port_mappings })
    }

    /// Get the container ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the host port mapped to a container port.
    pub fn host_port(&self, container_port: u16) -> Option<u16> {
        self.port_mappings.get(&container_port).copied()
    }

    /// Get the container logs.
    pub fn logs(&self) -> Result<String> {
        let output = Command::new("docker")
            .arg("logs")
            .arg(&self.id)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(format!("{}{}", stdout, stderr))
    }

    /// Wait for a string to appear in the logs.
    pub fn wait_for_log(&self, needle: &str, timeout: Duration) -> Result<()> {
        let start = Instant::now();

        while start.elapsed() < timeout {
            let logs = self.logs()?;
            if logs.contains(needle) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        Err(Error::Timeout {
            message: format!("waiting for '{}' in logs", needle),
        })
    }

    /// Wait for a TCP port to accept connections.
    pub fn wait_for_port(&self, container_port: u16, timeout: Duration) -> Result<u16> {
        let host_port = self.host_port(container_port).ok_or_else(|| Error::Parse(
            format!("port {} not mapped", container_port),
        ))?;

        let start = Instant::now();
        let addr = format!("127.0.0.1:{}", host_port);

        while start.elapsed() < timeout {
            if std::net::TcpStream::connect(&addr).is_ok() {
                return Ok(host_port);
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        Err(Error::Timeout {
            message: format!("waiting for port {} to accept connections", host_port),
        })
    }

    /// Stream logs line by line, calling the callback for each line.
    /// Returns when the callback returns `false` or timeout is reached.
    pub fn stream_logs<F>(&self, mut callback: F, timeout: Duration) -> Result<()>
    where
        F: FnMut(&str) -> bool,
    {
        let mut child = Command::new("docker")
            .arg("logs")
            .arg("-f") // follow
            .arg(&self.id)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        let start = Instant::now();

        for line in reader.lines() {
            if start.elapsed() > timeout {
                let _ = child.kill();
                return Err(Error::Timeout {
                    message: "streaming logs".to_string(),
                });
            }

            let line = line?;
            if !callback(&line) {
                let _ = child.kill();
                return Ok(());
            }
        }

        Ok(())
    }

    fn get_host_port(container_id: &str, container_port: u16) -> Result<u16> {
        let output = Command::new("docker")
            .arg("port")
            .arg(container_id)
            .arg(container_port.to_string())
            .output()?;

        if !output.status.success() {
            return Err(Error::Command {
                cmd: format!("docker port {} {}", container_id, container_port),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        // Output format: "0.0.0.0:32768" or "[::]:32768"
        let output_str = String::from_utf8_lossy(&output.stdout);
        let port_str = output_str
            .trim()
            .split(':')
            .last()
            .ok_or_else(|| Error::Parse(format!("unexpected port output: {}", output_str)))?;

        port_str
            .parse()
            .map_err(|_| Error::Parse(format!("invalid port number: {}", port_str)))
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        // Force remove the container
        let _ = Command::new("docker")
            .arg("rm")
            .arg("-f")
            .arg(&self.id)
            .output();
    }
}

/// Convenience module for common database containers.
pub mod containers {
    use super::*;

    /// Create a PostgreSQL container image.
    pub fn postgres(tag: &str, password: &str) -> Image {
        Image::new("postgres", tag)
            .env("POSTGRES_PASSWORD", password)
            .env("POSTGRES_HOST_AUTH_METHOD", "trust")
            .port(5432)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // requires docker
    fn test_postgres_container() {
        let container = Container::run(containers::postgres("16-alpine", "test")).unwrap();

        // Wait for postgres to be ready
        container
            .wait_for_log("database system is ready to accept connections", Duration::from_secs(30))
            .unwrap();

        let port = container.host_port(5432).unwrap();
        println!("Postgres available on port {}", port);

        // Verify we can connect
        container.wait_for_port(5432, Duration::from_secs(5)).unwrap();
    }
}
