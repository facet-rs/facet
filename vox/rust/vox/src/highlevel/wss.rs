use std::sync::Arc;

use vox_core::ConnectionAcceptor;

use super::{ServeError, VoxListener, serve_listener};

/// A [`VoxListener`] that accepts TCP connections, terminates TLS, then upgrades to WebSocket.
pub struct WssListener {
    tcp: tokio::net::TcpListener,
    tls: tokio_rustls::TlsAcceptor,
}

impl WssListener {
    /// Bind a WSS listener. Loads the certificate chain and private key from PEM files.
    pub async fn bind(
        addr: impl tokio::net::ToSocketAddrs,
        cert_path: &std::path::Path,
        key_path: &std::path::Path,
    ) -> std::io::Result<Self> {
        let tls = build_tls_acceptor(cert_path, key_path)?;
        let tcp = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self { tcp, tls })
    }

    /// Wrap an existing `TcpListener` with TLS configuration from PEM files.
    pub fn from_tcp(
        tcp: tokio::net::TcpListener,
        cert_path: &std::path::Path,
        key_path: &std::path::Path,
    ) -> std::io::Result<Self> {
        let tls = build_tls_acceptor(cert_path, key_path)?;
        Ok(Self { tcp, tls })
    }
}

fn build_tls_acceptor(
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> std::io::Result<tokio_rustls::TlsAcceptor> {
    use std::io::BufReader;

    let cert_file = std::fs::File::open(cert_path)?;
    let certs: Vec<_> =
        rustls_pemfile::certs(&mut BufReader::new(cert_file)).collect::<Result<_, _>>()?;
    if certs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "no certificates found in cert file",
        ));
    }

    let key_file = std::fs::File::open(key_path)?;
    let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))?.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "no private key found in key file",
        )
    })?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
}

impl VoxListener for WssListener {
    type Link = vox_websocket::WsLink<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>;

    async fn accept(&self) -> std::io::Result<Self::Link> {
        let (stream, _addr) = self.tcp.accept().await?;
        let tls_stream = self.tls.accept(stream).await?;
        vox_websocket::WsLink::server(tls_stream).await
    }
}

/// Parse `host:port?key=val&key2=val2` into `("host:port", {key: val, ...})`.
fn parse_query_params(s: &str) -> (&str, std::collections::HashMap<String, std::path::PathBuf>) {
    match s.split_once('?') {
        None => (s, Default::default()),
        Some((host, query)) => {
            let params = query
                .split('&')
                .filter_map(|pair| {
                    let (k, v) = pair.split_once('=')?;
                    Some((k.to_string(), std::path::PathBuf::from(v)))
                })
                .collect();
            (host, params)
        }
    }
}

pub(super) async fn serve_wss(
    host: &str,
    acceptor: impl ConnectionAcceptor,
) -> Result<(), ServeError> {
    let (host_part, params) = parse_query_params(host);
    let cert = params.get("cert").ok_or_else(|| {
        ServeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "wss:// requires ?cert=/path/to/cert.pem query parameter",
        ))
    })?;
    let key = params.get("key").ok_or_else(|| {
        ServeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "wss:// requires ?key=/path/to/key.pem query parameter",
        ))
    })?;
    let listener = WssListener::bind(host_part, cert.as_ref(), key.as_ref()).await?;
    Ok(serve_listener(listener, acceptor).await?)
}
