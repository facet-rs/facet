use super::VoxListener;

/// A [`VoxListener`] that accepts TCP connections and upgrades them to WebSocket.
pub struct WsListener {
    tcp: tokio::net::TcpListener,
}

impl WsListener {
    /// Bind a WebSocket listener to the given TCP address.
    pub async fn bind(addr: impl tokio::net::ToSocketAddrs) -> std::io::Result<Self> {
        let tcp = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self { tcp })
    }

    /// Wrap an existing `TcpListener` as a WebSocket listener.
    pub fn from_tcp(tcp: tokio::net::TcpListener) -> Self {
        Self { tcp }
    }

    /// Returns the local address this listener is bound to.
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.tcp.local_addr()
    }
}

impl VoxListener for WsListener {
    type Link = vox_websocket::WsLink<tokio::net::TcpStream>;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        let (stream, _addr) = self.tcp.accept().await?;
        vox_websocket::WsLink::server(stream).await
    }
}
