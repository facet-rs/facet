use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    BufferPool, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, MsgDescHot, Payload,
    TransportError,
};

use super::Transport;

/// Size of MsgDescHot in bytes (must be 64).
const DESC_SIZE: usize = 64;

const _: () = assert!(std::mem::size_of::<MsgDescHot>() == DESC_SIZE);

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
            }),
        }
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
    unsafe { std::mem::transmute_copy(desc) }
}

fn bytes_to_desc(bytes: &[u8; DESC_SIZE]) -> MsgDescHot {
    unsafe { std::mem::transmute_copy(bytes) }
}

impl Transport for StreamTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        if self.is_closed_inner() {
            return Err(TransportError::Closed);
        }

        let payload = frame.payload_bytes();
        let frame_len = DESC_SIZE + payload.len();
        let desc_bytes = desc_to_bytes(&frame.desc);

        let mut writer = self.inner.writer.lock().await;
        writer
            .write_all(&(frame_len as u32).to_le_bytes())
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

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        if self.is_closed_inner() {
            return Err(TransportError::Closed);
        }

        let mut reader = self.inner.reader.lock().await;

        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                TransportError::Closed
            } else {
                TransportError::Io(e.into())
            }
        })?;
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        if frame_len < DESC_SIZE {
            return Err(TransportError::Io(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("frame too small: {} < {}", frame_len, DESC_SIZE),
                )
                .into(),
            ));
        }

        let mut desc_buf = [0u8; DESC_SIZE];
        reader
            .read_exact(&mut desc_buf)
            .await
            .map_err(|e| TransportError::Io(e.into()))?;
        let mut desc = bytes_to_desc(&desc_buf);

        let payload_len = frame_len - DESC_SIZE;
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

        desc.payload_len = payload_len as u32;

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
