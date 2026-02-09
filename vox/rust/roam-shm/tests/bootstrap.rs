#![cfg(unix)]

use roam_shm::bootstrap::{SessionId, SessionPaths, unix};
use roam_shm::peer::PeerId;
use shm_primitives::{Doorbell, SignalResult};

#[tokio::test]
async fn bootstrap_transfers_doorbell_fd_and_supports_signaling() {
    let tmp = tempfile::Builder::new()
        .prefix("rshm-boot-")
        .tempdir_in("/tmp")
        .unwrap();
    let container_root = tmp.path().join("app-group");
    std::fs::create_dir_all(&container_root).unwrap();

    let sid = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let paths = SessionPaths::new(&container_root, sid.clone()).unwrap();
    let listener = unix::bind_control_socket(&paths).unwrap();

    let (host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
    let peer_id = PeerId::new(1).unwrap();
    let hub_path = paths.shm_path();
    std::fs::write(&hub_path, b"bootstrap").unwrap();
    let hub_path_for_host = hub_path.clone();

    let host_task = tokio::spawn(async move {
        unix::accept_and_send_ticket(
            &listener,
            &sid,
            peer_id,
            &hub_path_for_host,
            guest_handle.as_raw_fd(),
        )
        .await
    });

    let ticket = unix::request_ticket(
        &paths.control_sock_path(),
        &SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(ticket.peer_id, peer_id);
    assert_eq!(ticket.hub_path, hub_path);

    let guest_doorbell = Doorbell::from_raw_fd(ticket.doorbell_fd).unwrap();

    let guest_to_host = guest_doorbell.signal().await;
    assert!(matches!(
        guest_to_host,
        SignalResult::Sent | SignalResult::BufferFull
    ));
    host_doorbell.wait().await.unwrap();

    let host_to_guest = host_doorbell.signal().await;
    assert!(matches!(
        host_to_guest,
        SignalResult::Sent | SignalResult::BufferFull
    ));
    guest_doorbell.wait().await.unwrap();

    host_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn bootstrap_rejects_wrong_sid() {
    let tmp = tempfile::Builder::new()
        .prefix("rshm-boot-")
        .tempdir_in("/tmp")
        .unwrap();
    let container_root = tmp.path().join("app-group");
    std::fs::create_dir_all(&container_root).unwrap();

    let expected_sid = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let wrong_sid = SessionId::parse("123e4567-e89b-12d3-a456-426614174111").unwrap();

    let paths = SessionPaths::new(&container_root, expected_sid.clone()).unwrap();
    let listener = unix::bind_control_socket(&paths).unwrap();

    let (_host_doorbell, guest_handle) = Doorbell::create_pair().unwrap();
    let hub_path = paths.shm_path();
    std::fs::write(&hub_path, b"bootstrap").unwrap();

    let host_task = tokio::spawn(async move {
        unix::accept_and_send_ticket(
            &listener,
            &expected_sid,
            PeerId::new(1).unwrap(),
            &hub_path,
            guest_handle.as_raw_fd(),
        )
        .await
    });

    let result = unix::request_ticket(&paths.control_sock_path(), &wrong_sid).await;
    assert!(result.is_err());

    let host_result = host_task.await.unwrap();
    assert!(host_result.is_err());
}

#[tokio::test]
async fn bootstrap_fails_if_fd_not_passed() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixListener;

    let tmp = tempfile::tempdir().unwrap();
    let sock_path = tmp.path().join("control.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();

        let mut req = [0u8; 4];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut req)
            .await
            .unwrap();
        assert_eq!(&req, b"RSH0");

        let mut sid_len = [0u8; 2];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut sid_len)
            .await
            .unwrap();
        let sid_len = u16::from_le_bytes(sid_len) as usize;
        let mut sid = vec![0u8; sid_len];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut sid)
            .await
            .unwrap();

        // Write an OK response but intentionally do NOT send SCM_RIGHTS fd.
        let path = b"/tmp/fake.shm";
        stream.write_all(b"RSP0").await.unwrap();
        stream.write_all(&[0]).await.unwrap(); // STATUS_OK
        stream.write_all(&[1]).await.unwrap(); // peer_id
        stream
            .write_all(&(path.len() as u16).to_le_bytes())
            .await
            .unwrap();
        stream.write_all(path).await.unwrap();
        stream.flush().await.unwrap();
    });

    let sid = SessionId::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let result = unix::request_ticket(&sock_path, &sid).await;
    assert!(result.is_err());

    server.await.unwrap();
}
