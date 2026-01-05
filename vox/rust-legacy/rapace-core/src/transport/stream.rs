use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    BufferPool, DecodeError, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, MsgDescHot, Payload,
    TransportError, ValidationError,
};

use super::Transport;

/// Size of MsgDescHot in bytes (must be 64).
const DESC_SIZE: usize = 64;

const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

/// Maximum varint length in bytes.
/// Spec: `[impl transport.stream.varint-limit]`
const MAX_VARINT_LEN: usize = 10;

/// Default maximum payload size (16 MB).
/// This can be overridden during handshake negotiation.
const DEFAULT_MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;

/// Encode a u64 value as a varint into a buffer.
/// Returns the number of bytes written.
fn encode_varint(mut value: u64, buf: &mut [u8; MAX_VARINT_LEN]) -> usize {
    let mut i = 0;
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            buf[i] = byte;
            return i + 1;
        } else {
            buf[i] = byte | 0x80;
            i += 1;
        }
    }
}

/// Result of reading a varint from a stream.
enum VarintResult {
    /// Successfully read a varint value.
    Value(u64),
    /// Stream ended cleanly before any varint bytes were read.
    /// This represents a graceful connection close.
    CleanEof,
    /// Stream ended after reading some varint bytes but before termination.
    /// Spec: `[impl transport.stream.varint-terminated]`
    TruncatedVarint,
    /// Varint exceeded 10 bytes without terminating.
    /// Spec: `[impl transport.stream.varint-limit]`
    TooLong,
}

/// Read a varint from the stream.
/// Spec: `[impl transport.stream.varint-limit]` - Max 10 bytes.
/// Spec: `[impl transport.stream.varint-terminated]` - Must terminate before EOF.
async fn read_varint<R: AsyncRead + Unpin>(reader: &mut R) -> Result<VarintResult, std::io::Error> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;

    for (bytes_read, _) in (0..MAX_VARINT_LEN).enumerate() {
        let mut byte = [0u8; 1];
        match reader.read_exact(&mut byte).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Distinguish clean close from truncated varint
                if bytes_read == 0 {
                    return Ok(VarintResult::CleanEof);
                } else {
                    // Spec: `[impl transport.stream.varint-terminated]`
                    return Ok(VarintResult::TruncatedVarint);
                }
            }
            Err(e) => return Err(e),
        }

        value |= ((byte[0] & 0x7F) as u64) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(VarintResult::Value(value));
        }
        shift += 7;
    }

    // If we get here, we've read 10 bytes and the continuation bit is still set
    Ok(VarintResult::TooLong)
}

#[derive(Clone)]
pub struct StreamTransport {
    inner: Arc<StreamInner>,
}

impl std::fmt::Debug for StreamTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamTransport").finish_non_exhaustive()
    }
}

struct StreamInner {
    reader: AsyncMutex<Box<dyn AsyncRead + Unpin + Send + Sync>>,
    writer: AsyncMutex<Box<dyn AsyncWrite + Unpin + Send + Sync>>,
    closed: AtomicBool,
    buffer_pool: BufferPool,
    /// Maximum payload size (negotiated during handshake).
    /// Spec: `[impl transport.stream.size-limits]`
    max_payload_size: AtomicUsize,
}

impl StreamTransport {
    pub fn new<S>(stream: S) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
    {
        Self::with_buffer_pool(stream, BufferPool::new())
    }

    pub fn with_buffer_pool<S>(stream: S, buffer_pool: BufferPool) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
    {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            inner: Arc::new(StreamInner {
                reader: AsyncMutex::new(Box::new(reader)),
                writer: AsyncMutex::new(Box::new(writer)),
                closed: AtomicBool::new(false),
                buffer_pool,
                max_payload_size: AtomicUsize::new(DEFAULT_MAX_PAYLOAD_SIZE),
            }),
        }
    }

    /// Set the maximum payload size for this transport.
    ///
    /// This should be called after handshake negotiation to update the limit.
    /// Spec: `[impl transport.stream.size-limits]`
    pub fn set_max_payload_size(&self, size: usize) {
        self.inner.max_payload_size.store(size, Ordering::Release);
    }

    /// Create a transport from separate reader and writer streams.
    ///
    /// This is useful when you have separate read and write handles,
    /// such as stdin/stdout or split TCP connections.
    pub fn from_split<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + Sync + 'static,
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        Self::from_split_with_buffer_pool(reader, writer, BufferPool::new())
    }

    /// Create a transport from separate reader and writer streams with a custom buffer pool.
    pub fn from_split_with_buffer_pool<R, W>(reader: R, writer: W, buffer_pool: BufferPool) -> Self
    where
        R: AsyncRead + Unpin + Send + Sync + 'static,
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(StreamInner {
                reader: AsyncMutex::new(Box::new(reader)),
                writer: AsyncMutex::new(Box::new(writer)),
                closed: AtomicBool::new(false),
                buffer_pool,
                max_payload_size: AtomicUsize::new(DEFAULT_MAX_PAYLOAD_SIZE),
            }),
        }
    }

    /// Create a transport from stdin and stdout.
    ///
    /// This is useful for CLI tools that communicate via stdio,
    /// such as conformance test subjects.
    ///
    /// Note: Requires the `io-std` tokio feature, which is enabled by default
    /// for this crate on non-WASM targets.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_stdio() -> Self {
        Self::from_split(tokio::io::stdin(), tokio::io::stdout())
    }

    pub fn pair() -> (Self, Self) {
        let (a, b) = tokio::io::duplex(65536);
        (Self::new(a), Self::new(b))
    }

    fn is_closed_inner(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
}

fn desc_to_bytes(desc: &MsgDescHot) -> [u8; DESC_SIZE] {
    desc.to_bytes()
}

fn bytes_to_desc(bytes: &[u8; DESC_SIZE]) -> MsgDescHot {
    MsgDescHot::from_bytes(bytes)
}

impl Transport for StreamTransport {
    /// Send a frame over the stream transport.
    ///
    /// Spec: `[impl transport.stream.validation]` - Frames are length-prefixed with varint.
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        if self.is_closed_inner() {
            return Err(TransportError::Closed);
        }

        let payload = frame.payload_bytes();
        let frame_len = DESC_SIZE + payload.len();
        let desc_bytes = desc_to_bytes(&frame.desc);

        // Encode the length as a varint
        let mut varint_buf = [0u8; MAX_VARINT_LEN];
        let varint_len = encode_varint(frame_len as u64, &mut varint_buf);

        let mut writer = self.inner.writer.lock().await;
        writer
            .write_all(&varint_buf[..varint_len])
            .await
            .map_err(|e| TransportError::Io(e.into()))?;
        writer
            .write_all(&desc_bytes)
            .await
            .map_err(|e| TransportError::Io(e.into()))?;
        if !payload.is_empty() {
            writer
                .write_all(payload)
                .await
                .map_err(|e| TransportError::Io(e.into()))?;
        }
        writer
            .flush()
            .await
            .map_err(|e| TransportError::Io(e.into()))?;
        Ok(())
    }

    /// Receive a frame from the stream transport.
    ///
    /// Implements all validation rules from the spec:
    /// - Spec: `[impl transport.stream.varint-limit]` - Reject if varint > 10 bytes
    /// - Spec: `[impl transport.stream.varint-terminated]` - Reject if EOF before varint terminates
    /// - Spec: `[impl transport.stream.min-length]` - Reject if length < 64
    /// - Spec: `[impl transport.stream.max-length]` - Reject if length > max_payload_size + 64
    /// - Spec: `[impl transport.stream.length-match]` - Reject if payload_len != length - 64
    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        if self.is_closed_inner() {
            return Err(TransportError::Closed);
        }

        let mut reader = self.inner.reader.lock().await;

        // Read varint length prefix
        // Spec: `[impl transport.stream.varint-limit]`
        // Spec: `[impl transport.stream.varint-terminated]`
        let frame_len = match read_varint(&mut *reader).await {
            Ok(VarintResult::Value(len)) => len as usize,
            Ok(VarintResult::CleanEof) => {
                // Clean close - no bytes read before EOF
                return Err(TransportError::Closed);
            }
            Ok(VarintResult::TruncatedVarint) => {
                // Spec: `[impl transport.stream.varint-terminated]`
                // EOF after reading some varint bytes - malformed
                return Err(TransportError::Decode(DecodeError::InvalidData(
                    "stream ended before varint length prefix terminated".to_string(),
                )));
            }
            Ok(VarintResult::TooLong) => {
                // Spec: `[impl transport.stream.varint-limit]`
                return Err(TransportError::Decode(DecodeError::InvalidData(
                    "varint length prefix exceeded 10 bytes".to_string(),
                )));
            }
            Err(e) => return Err(TransportError::Io(e.into())),
        };

        // Spec: `[impl transport.stream.min-length]`
        if frame_len < DESC_SIZE {
            return Err(TransportError::Validation(ValidationError::FrameTooSmall {
                len: frame_len,
                min: DESC_SIZE,
            }));
        }

        let payload_len = frame_len - DESC_SIZE;

        // Spec: `[impl transport.stream.max-length]`
        // Spec: `[impl transport.stream.size-limits]`
        let max_payload_size = self.inner.max_payload_size.load(Ordering::Acquire);
        if payload_len > max_payload_size {
            return Err(TransportError::Validation(
                ValidationError::PayloadTooLarge {
                    len: payload_len as u32,
                    max: max_payload_size as u32,
                },
            ));
        }

        let mut desc_buf = [0u8; DESC_SIZE];
        reader
            .read_exact(&mut desc_buf)
            .await
            .map_err(|e| TransportError::Io(e.into()))?;
        let mut desc = bytes_to_desc(&desc_buf);

        // Spec: `[impl transport.stream.length-match]`
        // Validate that payload_len in descriptor matches length - 64
        if desc.payload_len as usize != payload_len {
            return Err(TransportError::Decode(DecodeError::InvalidData(format!(
                "payload_len mismatch: descriptor says {}, length prefix implies {}",
                desc.payload_len, payload_len
            ))));
        }

        let pooled_buf = if payload_len > 0 {
            let mut buf = self.inner.buffer_pool.get();
            buf.resize(payload_len, 0);
            reader
                .read_exact(&mut buf)
                .await
                .map_err(|e| TransportError::Io(e.into()))?;
            Some(buf)
        } else {
            None
        };

        if payload_len <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            if let Some(buf) = pooled_buf {
                desc.inline_payload[..payload_len].copy_from_slice(&buf);
            }
            Ok(Frame {
                desc,
                payload: Payload::Inline,
            })
        } else {
            desc.payload_slot = 0;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            Ok(Frame {
                desc,
                payload: Payload::Pooled(pooled_buf.unwrap()),
            })
        }
    }

    fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
    }

    fn is_closed(&self) -> bool {
        self.is_closed_inner()
    }

    fn buffer_pool(&self) -> &crate::BufferPool {
        &self.inner.buffer_pool
    }
}
