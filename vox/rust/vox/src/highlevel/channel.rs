use vox_types::MaybeSend;

use super::VoxListener;

#[cfg(not(target_arch = "wasm32"))]
type SendError<T> = moire::sync::mpsc::error::SendError<T>;
#[cfg(target_arch = "wasm32")]
type SendError<T> = moire::sync::mpsc::SendError<T>;

/// A [`VoxListener`] backed by a channel.
///
/// Use this when you control how connections arrive (e.g. from an axum
/// WebSocket upgrade handler) and want to feed them into [`super::serve_listener()`].
pub struct ChannelListener<L> {
    rx: moire::sync::mpsc::Receiver<L>,
}

/// Sender half of a [`ChannelListener`].
#[derive(Clone)]
pub struct ChannelListenerSender<L> {
    tx: moire::sync::mpsc::Sender<L>,
}

impl<L: vox_types::Link + MaybeSend + 'static> ChannelListener<L> {
    /// Create a new channel listener with the given buffer capacity.
    pub fn new(buffer: usize) -> (Self, ChannelListenerSender<L>) {
        let (tx, rx) = moire::sync::mpsc::channel("channel-listener", buffer);
        (Self { rx }, ChannelListenerSender { tx })
    }
}

impl<L: vox_types::Link + MaybeSend + 'static> ChannelListenerSender<L> {
    /// Send a link to the listener.
    pub async fn send(&self, link: L) -> Result<(), SendError<L>> {
        self.tx.send(link).await
    }
}

impl<L> VoxListener for ChannelListener<L>
where
    L: vox_types::Link + MaybeSend + 'static,
{
    type Link = L;

    async fn accept(&mut self) -> std::io::Result<Self::Link> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel closed"))
    }
}
