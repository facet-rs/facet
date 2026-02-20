// ============================================================================
// Tunnel Adapters for AsyncRead/AsyncWrite Streams (native only)
// ============================================================================

use facet::Facet;
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;

use crate::{Rx, Tx, channel};

/// Default chunk size for tunnel pumps (32KB).
///
/// Balances throughput with memory usage and slot consumption.
/// Larger values improve throughput but use more memory per read.
/// Smaller values improve latency but increase syscall overhead.
#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_TUNNEL_CHUNK_SIZE: usize = 32 * 1024;

/// A bidirectional byte tunnel over roam channels.
///
/// From the perspective of whoever holds the tunnel:
/// - `tx`: Send bytes TO the remote end
/// - `rx`: Receive bytes FROM the remote end
///
/// Tunnels are typically used to bridge async byte streams (TCP, Unix sockets, etc.)
/// with roam's streaming channels. One side creates a tunnel pair with [`tunnel_pair()`],
/// passes one half to the remote via an RPC call, and uses the other half locally.
///
/// # Example
///
/// ```ignore
/// // Host side: create tunnel and pump to/from a socket
/// let (local, remote) = roam_session::tunnel_pair();
/// let (read_handle, write_handle) = roam_session::tunnel_stream(socket, local, 32 * 1024);
///
/// // Pass `remote` to cell via RPC
/// cell.handle_connection(remote).await?;
/// ```
#[derive(Facet)]
pub struct Tunnel {
    /// Channel for sending bytes to the remote end.
    pub tx: Tx<Vec<u8>>,
    /// Channel for receiving bytes from the remote end.
    pub rx: Rx<Vec<u8>>,
}

/// Create a pair of connected tunnels.
///
/// Returns `(local, remote)` where:
/// - Data sent on `local.tx` arrives at `remote.rx`
/// - Data sent on `remote.tx` arrives at `local.rx`
///
/// This is useful for creating a bidirectional channel that can be split
/// across an RPC boundary. One side keeps `local` and passes `remote` to
/// the other side via an RPC call.
///
/// # Example
///
/// ```ignore
/// let (local, remote) = tunnel_pair();
///
/// // Spawn tasks to pump data from local stream
/// tunnel_stream(tcp_stream, local, DEFAULT_TUNNEL_CHUNK_SIZE);
///
/// // Send remote to the other side via RPC
/// service.handle_tunnel(remote).await?;
/// ```
pub fn tunnel_pair() -> (Tunnel, Tunnel) {
    let (tx1, rx1) = channel::<Vec<u8>>();
    let (tx2, rx2) = channel::<Vec<u8>>();
    (Tunnel { tx: tx1, rx: rx2 }, Tunnel { tx: tx2, rx: rx1 })
}

/// Pump bytes from an `AsyncRead` into a `Tx<Vec<u8>>`.
///
/// Reads chunks up to `chunk_size` bytes and sends them on the channel.
/// Returns when the reader reaches EOF or the channel closes.
///
/// # Arguments
///
/// * `reader` - Any type implementing `AsyncRead + Unpin`
/// * `tx` - The transmit channel to send bytes to
/// * `chunk_size` - Maximum bytes to read per chunk
///
/// # Returns
///
/// * `Ok(())` - Reader reached EOF, channel closed gracefully
/// * `Err(io::Error)` - Read error occurred
///
/// # Example
///
/// ```ignore
/// let (tx, rx) = roam::channel::<Vec<u8>>();
/// let result = pump_read_to_tx(reader, tx, 32 * 1024).await;
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub async fn pump_read_to_tx<R: AsyncRead + Unpin>(
    mut reader: R,
    tx: Tx<Vec<u8>>,
    chunk_size: usize,
) -> io::Result<()> {
    let mut buf = vec![0u8; chunk_size];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            // EOF - drop tx to close the channel
            break;
        }
        // Send the bytes we read
        if tx.send(&buf[..n].to_vec()).await.is_err() {
            // Channel closed by receiver - treat as graceful shutdown
            break;
        }
    }
    Ok(())
}

/// Pump bytes from an `Rx<Vec<u8>>` into an `AsyncWrite`.
///
/// Receives chunks and writes them to the writer.
/// Returns when the channel closes or a write error occurs.
///
/// # Arguments
///
/// * `rx` - The receive channel to get bytes from
/// * `writer` - Any type implementing `AsyncWrite + Unpin`
///
/// # Returns
///
/// * `Ok(())` - Channel closed gracefully
/// * `Err(io::Error)` - Write error or deserialization error occurred
///
/// # Example
///
/// ```ignore
/// let (tx, rx) = roam::channel::<Vec<u8>>();
/// let result = pump_rx_to_write(rx, writer).await;
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub async fn pump_rx_to_write<W: AsyncWrite + Unpin>(
    mut rx: Rx<Vec<u8>>,
    mut writer: W,
) -> io::Result<()> {
    loop {
        match rx.recv().await {
            Ok(Some(data)) => {
                writer.write_all(&data).await?;
            }
            Ok(None) => {
                // Channel closed - flush and exit
                writer.flush().await?;
                break;
            }
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("tunnel receive error: {e}"),
                ));
            }
        }
    }
    Ok(())
}

/// Tunnel a bidirectional stream through a roam Tunnel.
///
/// Spawns two tasks to pump data in both directions:
/// - One task reads from `stream` and sends to `tunnel.tx`
/// - One task receives from `tunnel.rx` and writes to `stream`
///
/// Returns handles to join on completion. Both tasks run until their
/// respective direction completes (EOF/close) or an error occurs.
///
/// # Arguments
///
/// * `stream` - Any type implementing `AsyncRead + AsyncWrite + Unpin + Send + 'static`
/// * `tunnel` - The tunnel to pump data through
/// * `chunk_size` - Maximum bytes to read per chunk (see [`DEFAULT_TUNNEL_CHUNK_SIZE`])
///
/// # Returns
///
/// A tuple of `(read_handle, write_handle)`:
/// - `read_handle` - Completes when the stream reaches EOF or tx closes
/// - `write_handle` - Completes when rx closes or stream write fails
///
/// # Example
///
/// ```ignore
/// let (local, remote) = tunnel_pair();
/// let (read_handle, write_handle) = tunnel_stream(tcp_stream, local, DEFAULT_TUNNEL_CHUNK_SIZE);
///
/// // Wait for both directions to complete
/// let _ = read_handle.await;
/// let _ = write_handle.await;
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub fn tunnel_stream<S>(
    stream: S,
    tunnel: Tunnel,
    chunk_size: usize,
) -> (JoinHandle<io::Result<()>>, JoinHandle<io::Result<()>>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);
    let Tunnel { tx, rx } = tunnel;

    let read_handle = moire::spawn_tracked(
        "roam_tunnel_read",
        async move { pump_read_to_tx(reader, tx, chunk_size).await },
        crate::source_id_for_current_crate(),
    );

    let write_handle = moire::spawn_tracked(
        "roam_tunnel_write",
        async move { pump_rx_to_write(rx, writer).await },
        crate::source_id_for_current_crate(),
    );

    (read_handle, write_handle)
}
