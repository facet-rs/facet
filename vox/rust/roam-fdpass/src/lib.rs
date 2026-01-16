//! Cross-platform file descriptor / handle passing for roam.
//!
//! This crate provides a unified API for passing file descriptors (Unix) or
//! socket handles (Windows) between processes over IPC channels.
//!
//! # Platform implementations
//!
//! - **Unix**: Uses SCM_RIGHTS to pass raw file descriptors over Unix domain sockets
//! - **Windows**: Uses `WSADuplicateSocket` to serialize socket state, sends it over
//!   a named pipe, and recreates the socket with `WSASocket` on the receiving end
//!
//! # Usage
//!
//! ## Unix
//!
//! ```ignore
//! use roam_fdpass::{send_fd, recv_fd};
//! use tokio::net::UnixStream;
//!
//! // Sender
//! let tcp_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
//! let fd = tcp_listener.into_raw_fd();
//! send_fd(&unix_stream, fd).await?;
//!
//! // Receiver
//! let fd = recv_fd(&unix_stream).await?;
//! let listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
//! ```
//!
//! ## Windows
//!
//! ```ignore
//! use roam_fdpass::{send_socket, recv_socket};
//! use tokio::net::windows::named_pipe::{NamedPipeServer, NamedPipeClient};
//!
//! // Sender (needs receiver's process ID)
//! let tcp_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
//! send_socket(&pipe, &tcp_listener, receiver_pid).await?;
//!
//! // Receiver
//! let listener: std::net::TcpListener = recv_socket(&pipe).await?;
//! ```

#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
pub use windows::*;
