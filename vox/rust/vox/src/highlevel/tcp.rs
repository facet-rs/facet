use super::VoxListener;

impl VoxListener for tokio::net::TcpListener {
    type Link =
        vox_stream::StreamLink<tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf>;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        let (stream, _addr) = tokio::net::TcpListener::accept(self).await?;
        Ok(vox_stream::StreamLink::tcp(stream))
    }
}
