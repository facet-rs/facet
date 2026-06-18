//! End-to-end `vox::Fd` passing through a real `#[vox::service]`.
//!
//! A descriptor returned by a service method must arrive on the client as a
//! working file descriptor (proven by reading an *unlinked* temp file
//! through it — only the descriptor keeps the inode alive). Also covers
//! multi-fd, the `SCM_MAX_FD` hard cap, and a non-fd transport refusing to
//! carry an `Fd`.
#![cfg(unix)]

use std::io::{Read, Seek, Write};
use std::os::fd::OwnedFd;

use vox::Fd;
use vox::transport::local::FdStreamLink;
use vox::transport::tcp::StreamLink;

/// Fresh, immediately-unlinked temp file seeded with `seed`, positioned at 0.
/// Reading it back through a transported fd proves the *descriptor* moved,
/// not the path.
fn temp_blob(seed: &[u8]) -> std::fs::File {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("vox-fdpass-it-{}-{nanos}", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    let _ = std::fs::remove_file(&path);
    f.write_all(seed).unwrap();
    f.rewind().unwrap();
    f
}

fn read_fd(fd: Fd) -> String {
    let mut f = std::fs::File::from(fd.into_owned_fd().expect("owned fd"));
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    s
}

#[vox::service]
trait FdVault {
    /// Hand the caller a descriptor to a freshly-created (already unlinked)
    /// blob containing `tag`-derived bytes.
    async fn open_blob(&self, tag: u32) -> Fd;
    /// Hand back several descriptors at once.
    async fn open_many(&self, n: u32) -> Vec<Fd>;
}

#[derive(Clone)]
struct Vault;

impl FdVault for Vault {
    async fn open_blob(&self, tag: u32) -> Fd {
        Fd::new(OwnedFd::from(temp_blob(format!("blob-{tag}").as_bytes())))
    }

    async fn open_many(&self, n: u32) -> Vec<Fd> {
        (0..n)
            .map(|i| Fd::new(OwnedFd::from(temp_blob(format!("m{i}").as_bytes()))))
            .collect()
    }
}

async fn fd_pair() -> (FdVaultClient, vox::ConnectionHandle) {
    let (client_link, server_link) = FdStreamLink::pair().unwrap();
    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_lane(FdVaultDispatcher::new(Vault))
            .establish_connection()
            .await
            .expect("server establish")
    });
    let client = vox::initiator_on(client_link)
        .establish::<FdVaultClient>()
        .await
        .expect("client establish");
    let server_guard = server.await.expect("server task");
    (client, server_guard)
}

#[tokio::test]
async fn single_fd_round_trips_through_service() {
    let (client, _server) = fd_pair().await;
    let fd = client.open_blob(7).await.expect("open_blob call");
    assert_eq!(read_fd(fd), "blob-7");
}

#[tokio::test]
async fn multiple_fds_in_one_response() {
    let (client, _server) = fd_pair().await;
    let fds = client.open_many(4).await.expect("open_many call");
    assert_eq!(fds.len(), 4);
    let got: Vec<String> = fds.into_iter().map(read_fd).collect();
    assert_eq!(got, vec!["m0", "m1", "m2", "m3"]);
}

#[tokio::test]
async fn exceeding_scm_max_fd_is_an_error_not_a_crash() {
    let (client, _server) = fd_pair().await;
    // 300 > SCM_MAX_FD (253): the server's send must fail cleanly and the
    // call surfaces an error rather than corrupting the stream.
    let result = client.open_many(300).await;
    assert!(
        result.is_err(),
        "a response with >253 fds must error, got {result:?}"
    );
}

#[tokio::test]
// r[verify transport.fd.capability]
async fn non_fd_transport_refuses_to_carry_an_fd() {
    // TCP `StreamLink` advertises no fd support; returning an `Fd` over it
    // must fail at send rather than silently dropping the descriptor.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        vox::acceptor_on(StreamLink::tcp(sock))
            .on_lane(FdVaultDispatcher::new(Vault))
            .establish_connection()
            .await
            .expect("server establish")
    });

    let client_sock = tokio::net::TcpStream::connect(addr).await.unwrap();
    let client = vox::initiator_on(StreamLink::tcp(client_sock))
        .establish::<FdVaultClient>()
        .await
        .expect("client establish");
    let _server = server.await.expect("server task");

    let result = client.open_blob(1).await;
    assert!(
        result.is_err(),
        "TCP transport must refuse an Fd-bearing response, got {result:?}"
    );
}
