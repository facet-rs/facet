use super::VoxListener;

/// A [`VoxListener`] backed by a channel.
///
/// Use this when you control how connections arrive (e.g. from an axum
/// WebSocket upgrade handler) and want to feed them into [`super::serve_listener()`].
pub struct ChannelListener<L> {
    rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<L>>,
}

/// Sender half of a [`ChannelListener`].
#[derive(Clone)]
pub struct ChannelListenerSender<L> {
    tx: tokio::sync::mpsc::Sender<L>,
}

impl<L: vox_types::Link + Send + 'static> ChannelListener<L> {
    /// Create a new channel listener with the given buffer capacity.
    pub fn new(buffer: usize) -> (Self, ChannelListenerSender<L>) {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer);
        (
            Self {
                rx: tokio::sync::Mutex::new(rx),
            },
            ChannelListenerSender { tx },
        )
    }
}

impl<L: vox_types::Link + Send + 'static> ChannelListenerSender<L> {
    /// Send a link to the listener.
    pub async fn send(&self, link: L) -> Result<(), tokio::sync::mpsc::error::SendError<L>> {
        self.tx.send(link).await
    }
}

impl<L> VoxListener for ChannelListener<L>
where
    L: vox_types::Link + Send + 'static,
{
    type Link = L;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        self.rx
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed"))
    }
}
