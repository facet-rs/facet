use std::sync::Arc;

use tokio::sync::mpsc;

use crate::{BufferPool, Frame, Payload, TransportError};

use super::Transport;

const CHANNEL_CAPACITY: usize = 64;

#[derive(Clone, Debug)]
pub struct MemTransport {
    inner: Arc<MemInner>,
    buffer_pool: BufferPool,
}

#[derive(Debug)]
struct MemInner {
    tx: mpsc::Sender<Frame>,
    rx: tokio::sync::Mutex<mpsc::Receiver<Frame>>,
    closed: std::sync::atomic::AtomicBool,
}

impl MemTransport {
    pub fn pair() -> (Self, Self) {
        let (tx_a, rx_a) = mpsc::channel(CHANNEL_CAPACITY);
        let (tx_b, rx_b) = mpsc::channel(CHANNEL_CAPACITY);

        let inner_a = Arc::new(MemInner {
            tx: tx_b,
            rx: tokio::sync::Mutex::new(rx_a),
            closed: std::sync::atomic::AtomicBool::new(false),
        });

        let inner_b = Arc::new(MemInner {
            tx: tx_a,
            rx: tokio::sync::Mutex::new(rx_b),
            closed: std::sync::atomic::AtomicBool::new(false),
        });

        (
            Self {
                inner: inner_a,
                buffer_pool: BufferPool::new(),
            },
            Self {
                inner: inner_b,
                buffer_pool: BufferPool::new(),
            },
        )
    }

    fn is_closed_inner(&self) -> bool {
        self.inner.closed.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Transport for MemTransport {
    async fn send_frame(&self, mut frame: Frame) -> Result<(), TransportError> {
        if self.is_closed_inner() {
            return Err(TransportError::Closed);
        }

        if frame.desc.is_inline() {
            frame.payload = Payload::Inline;
        }

        self.inner
            .tx
            .send(frame)
            .await
            .map_err(|_| TransportError::Closed)
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        if self.is_closed_inner() {
            return Err(TransportError::Closed);
        }

        let frame = {
            let mut rx = self.inner.rx.lock().await;
            rx.recv().await.ok_or(TransportError::Closed)?
        };

        Ok(frame)
    }

    fn close(&self) {
        self.inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn is_closed(&self) -> bool {
        self.is_closed_inner()
    }

    fn buffer_pool(&self) -> &BufferPool {
        &self.buffer_pool
    }
}
