#[cfg(all(feature = "shm", unix))]
mod tests {
    use std::sync::Arc;

    use rapace_core::Transport;
    use rapace_core::shm::{Doorbell, HubConfig, HubHost, HubPeer, ShmTransport};
    use rapace_core::{Frame, FrameFlags, MsgDescHot};

    fn temp_hub_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rapace_hub_test_{}.shm", std::process::id()))
    }

    #[tokio_test_lite::test]
    async fn hub_peer_to_host_and_back() {
        let path = temp_hub_path();
        let _ = std::fs::remove_file(&path);

        let host = Arc::new(HubHost::create(&path, HubConfig::default()).expect("create hub"));
        let peer_info = host.add_peer().expect("add peer");

        let peer = Arc::new(HubPeer::open(&path, peer_info.peer_id).expect("open peer"));
        peer.register();

        // Plugin wraps the inherited fd end.
        let peer_doorbell =
            Doorbell::from_raw_fd(peer_info.peer_doorbell_fd).expect("peer doorbell");

        let host_transport = Transport::Shm(ShmTransport::hub_host_peer(
            host.clone(),
            peer_info.peer_id,
            peer_info.doorbell,
        ));
        let peer_transport =
            Transport::Shm(ShmTransport::hub_peer(peer.clone(), peer_doorbell, "peer"));

        // Peer -> host
        let mut desc = MsgDescHot::new();
        desc.channel_id = 7;
        desc.method_id = 123;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;
        let frame = Frame::with_payload(desc, b"hello".to_vec());
        peer_transport.send_frame(frame).await.expect("send");

        let received = host_transport.recv_frame().await.expect("recv");
        assert_eq!(received.desc.channel_id, 7);
        assert_eq!(received.payload_bytes(), b"hello");

        // Host -> peer
        let mut desc2 = MsgDescHot::new();
        desc2.channel_id = 8;
        desc2.method_id = 321;
        desc2.flags = FrameFlags::DATA | FrameFlags::EOS;
        let frame2 = Frame::with_payload(desc2, b"world".to_vec());
        host_transport.send_frame(frame2).await.expect("send2");

        let received2 = peer_transport.recv_frame().await.expect("recv2");
        assert_eq!(received2.desc.channel_id, 8);
        assert_eq!(received2.payload_bytes(), b"world");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio_test_lite::test]
    async fn hub_via_shm_transport_enum() {
        let path = temp_hub_path();
        let _ = std::fs::remove_file(&path);

        let host = Arc::new(HubHost::create(&path, HubConfig::default()).expect("create hub"));
        let peer_info = host.add_peer().expect("add peer");

        let peer = Arc::new(HubPeer::open(&path, peer_info.peer_id).expect("open peer"));
        peer.register();
        let peer_doorbell =
            Doorbell::from_raw_fd(peer_info.peer_doorbell_fd).expect("peer doorbell");

        let host_transport = Transport::Shm(ShmTransport::hub_host_peer(
            host.clone(),
            peer_info.peer_id,
            peer_info.doorbell,
        ));
        let peer_transport =
            Transport::Shm(ShmTransport::hub_peer(peer.clone(), peer_doorbell, "peer"));

        let mut desc = MsgDescHot::new();
        desc.channel_id = 1;
        desc.method_id = 1;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;
        peer_transport
            .send_frame(Frame::with_payload(desc, vec![1, 2, 3]))
            .await
            .expect("send");

        let got = host_transport.recv_frame().await.expect("recv");
        assert_eq!(got.payload_bytes(), &[1, 2, 3]);

        let _ = std::fs::remove_file(&path);
    }
}
