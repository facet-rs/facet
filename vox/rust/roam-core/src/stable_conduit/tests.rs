use std::collections::VecDeque;

use roam_types::{Conduit, ConduitRx, ConduitTx, ConduitTxPermit, LinkRx, LinkTx, MsgFamily};

use crate::{MemoryLink, memory_link_pair};

use super::*;

struct StringFamily;

impl MsgFamily for StringFamily {
    type Msg<'a> = String;

    fn shape() -> &'static facet_core::Shape {
        String::SHAPE
    }
}

// A LinkSource backed by a queue of pre-created MemoryLinks.
struct QueuedLinkSource {
    links: VecDeque<(MemoryLink, Option<ClientHello>)>,
}

impl LinkSource for QueuedLinkSource {
    type Link = MemoryLink;

    async fn next_link(&mut self) -> std::io::Result<Attachment<MemoryLink>> {
        let (link, client_hello) = self.links.pop_front().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "no more links")
        })?;
        Ok(Attachment { link, client_hello })
    }
}

fn server_hello(resume_key: ResumeKey, last_received: Option<u32>, rejected: bool) -> ServerHello {
    let mut flags = 0u8;
    if rejected {
        flags |= SH_REJECTED;
    }
    if last_received.is_some() {
        flags |= SH_HAS_LAST_RECEIVED;
    }
    ServerHello {
        magic: LeU32::new(SERVER_HELLO_MAGIC),
        flags,
        resume_key,
        last_received: LeU32::new(last_received.unwrap_or(0)),
    }
}

fn resume_key(b: &[u8]) -> ResumeKey {
    let mut key = [0u8; 16];
    let len = b.len().min(16);
    key[..len].copy_from_slice(&b[..len]);
    ResumeKey(key)
}

// Encode and send a frame directly onto a LinkTx.
async fn send_frame<LTx: LinkTx>(tx: &LTx, seq: u32, ack: Option<u32>, item: &str) {
    let frame = Frame {
        seq: PacketSeq(seq),
        ack: ack.map(|n| PacketAck {
            max_delivered: PacketSeq(n),
        }),
        item: item.to_string(),
    };
    let frame_bytes = facet_postcard::to_vec(&frame).unwrap();

    let permit = tx.reserve().await.unwrap();
    let mut slot = permit.alloc(frame_bytes.len()).unwrap();
    slot.as_mut_slice().copy_from_slice(&frame_bytes);
    slot.commit();
}

// Decode a raw frame payload into (seq, ack_max, item).
fn decode_frame(bytes: &[u8]) -> (u32, Option<u32>, String) {
    let frame: Frame<String> = facet_postcard::from_slice(bytes).unwrap();
    (
        frame.seq.0,
        frame.ack.map(|a| a.max_delivered.0),
        frame.item,
    )
}

// Receive one raw payload from a LinkRx.
async fn recv_raw<LRx: LinkRx>(rx: &mut LRx) -> Vec<u8> {
    let backing = rx.recv().await.unwrap().unwrap();
    match backing {
        roam_types::Backing::Boxed(b) => b.to_vec(),
        roam_types::Backing::Shared(s) => s.as_bytes().to_vec(),
    }
}

// ---------------------------------------------------------------------------
// Basic StableConduit tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stable_send_recv_single() {
    let (c, s) = memory_link_pair(16);

    let source = QueuedLinkSource {
        links: VecDeque::from([(c, None)]),
    };

    // Server-side: complete handshake then send a frame.
    let server = tokio::spawn(async move {
        let (s_tx, mut s_rx) = s.split();
        let _hello = recv_handshake::<_, ClientHello>(&mut s_rx).await.unwrap();
        send_handshake(&s_tx, &server_hello(resume_key(b"key"), None, false))
            .await
            .unwrap();

        // Receive one frame from client.
        let raw = recv_raw(&mut s_rx).await;
        let (seq, _, item) = decode_frame(&raw);
        (seq, item)
    });

    let client = StableConduit::<StringFamily, _>::new(source).await.unwrap();
    let (client_tx, _client_rx) = client.split();

    let permit = client_tx.reserve().await.unwrap();
    permit.send("hello".to_string()).unwrap();

    let (seq, item) = server.await.unwrap();
    assert_eq!(seq, 0);
    assert_eq!(item, "hello");
}

// ---------------------------------------------------------------------------
// Reconnect tests
// ---------------------------------------------------------------------------

/// Client sends A and B. Server acks A. Link dies.
/// On reconnect, server reports last_received = Some(0) (saw A).
/// Client replays B (seq 1). Server receives it on the new link.
#[tokio::test]
async fn reconnect_replays_unacked_frames() {
    let (c1, s1) = memory_link_pair(32);
    let (c2, s2) = memory_link_pair(32);

    // Link 1: server receives A and B, acks A, then drops.
    let server1 = tokio::spawn(async move {
        let (s1_tx, mut s1_rx) = s1.split();

        let _hello = recv_handshake::<_, ClientHello>(&mut s1_rx).await.unwrap();
        send_handshake(
            &s1_tx,
            &server_hello(resume_key(b"resume-key-for-test"), None, false),
        )
        .await
        .unwrap();

        // Receive A (seq 0)
        let raw = recv_raw(&mut s1_rx).await;
        let (seq_a, _, item_a) = decode_frame(&raw);
        assert_eq!(seq_a, 0);
        assert_eq!(item_a, "alpha");

        // Receive B (seq 1)
        let raw = recv_raw(&mut s1_rx).await;
        let (seq_b, _, item_b) = decode_frame(&raw);
        assert_eq!(seq_b, 1);
        assert_eq!(item_b, "beta");

        // Send ack for seq 0 (server has received A but NOT B as far as client knows)
        // The client will trim replay buffer entry for seq 0 after receiving this.
        send_frame(&s1_tx, 0, Some(0), "ack-for-alpha").await;

        // Drop — link dies, triggering reconnect on client.
    });

    // Link 2: server handles reconnect, replays, receives replayed B.
    let server2 = tokio::spawn(async move {
        let (s2_tx, mut s2_rx) = s2.split();

        let hello = recv_handshake::<_, ClientHello>(&mut s2_rx).await.unwrap();
        // Client should present a resume key.
        assert!(hello.flags & CH_HAS_RESUME_KEY != 0);
        // Client received one frame from server (seq 0), so last_received = Some(0).
        assert!(hello.flags & CH_HAS_LAST_RECEIVED != 0);
        assert_eq!(hello.last_received.get(), 0);

        // Server says it received up to seq 0 from the client (it saw A but not B).
        send_handshake(
            &s2_tx,
            &server_hello(resume_key(b"resume-key-2"), Some(0), false),
        )
        .await
        .unwrap();

        // Client should replay B (seq 1) automatically.
        let raw = recv_raw(&mut s2_rx).await;
        let (seq, _, item) = decode_frame(&raw);
        assert_eq!(seq, 1);
        assert_eq!(item, "beta");

        // New message after reconnect (seq 2).
        let raw = recv_raw(&mut s2_rx).await;
        let (seq, _, item) = decode_frame(&raw);
        assert_eq!(seq, 2);
        assert_eq!(item, "gamma");
    });

    // Client side.
    let source = QueuedLinkSource {
        links: VecDeque::from([(c1, None), (c2, None)]),
    };
    let client = StableConduit::<StringFamily, _>::new(source).await.unwrap();
    let (client_tx, mut client_rx) = client.split();

    // Send A and B.
    client_tx
        .reserve()
        .await
        .unwrap()
        .send("alpha".to_string())
        .unwrap();
    client_tx
        .reserve()
        .await
        .unwrap()
        .send("beta".to_string())
        .unwrap();

    // Receive the ack frame from server1. This trims seq 0 from replay buffer,
    // leaving only seq 1 (beta) buffered.
    let msg = client_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*msg, "ack-for-alpha");

    // server1 drops — recv triggers reconnect transparently.
    // After reconnect, client replays beta, then we send gamma.
    client_tx
        .reserve()
        .await
        .unwrap()
        .send("gamma".to_string())
        .unwrap();

    server1.await.unwrap();
    server2.await.unwrap();
}

/// On reconnect, server says it has seen everything. Client sends nothing extra.
#[tokio::test]
async fn reconnect_no_replay_when_all_acked() {
    let (c1, s1) = memory_link_pair(32);
    let (c2, s2) = memory_link_pair(32);

    let server1 = tokio::spawn(async move {
        let (s1_tx, mut s1_rx) = s1.split();
        let _ = recv_handshake::<_, ClientHello>(&mut s1_rx).await.unwrap();
        send_handshake(&s1_tx, &server_hello(resume_key(b"key1"), None, false))
            .await
            .unwrap();

        // Receive A and B.
        recv_raw(&mut s1_rx).await;
        recv_raw(&mut s1_rx).await;

        // Ack both.
        send_frame(&s1_tx, 0, Some(1), "ack-both").await;
        // Drop.
    });

    let server2 = tokio::spawn(async move {
        let (s2_tx, mut s2_rx) = s2.split();
        let hello = recv_handshake::<_, ClientHello>(&mut s2_rx).await.unwrap();
        assert!(hello.flags & CH_HAS_RESUME_KEY != 0);

        // Server has seen everything (up to seq 1).
        send_handshake(&s2_tx, &server_hello(resume_key(b"key2"), Some(1), false))
            .await
            .unwrap();

        // Only the new message (seq 2) should arrive — no replay.
        let raw = recv_raw(&mut s2_rx).await;
        let (seq, _, item) = decode_frame(&raw);
        assert_eq!(seq, 2);
        assert_eq!(item, "gamma");
    });

    let source = QueuedLinkSource {
        links: VecDeque::from([(c1, None), (c2, None)]),
    };
    let client = StableConduit::<StringFamily, _>::new(source).await.unwrap();
    let (client_tx, mut client_rx) = client.split();

    client_tx
        .reserve()
        .await
        .unwrap()
        .send("alpha".to_string())
        .unwrap();
    client_tx
        .reserve()
        .await
        .unwrap()
        .send("beta".to_string())
        .unwrap();

    let msg = client_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*msg, "ack-both");

    // Reconnect happens transparently here.
    client_tx
        .reserve()
        .await
        .unwrap()
        .send("gamma".to_string())
        .unwrap();

    server1.await.unwrap();
    server2.await.unwrap();
}

/// After reconnect, duplicate frames (seq <= last_received) are silently dropped.
#[tokio::test]
async fn duplicate_frames_are_skipped() {
    let (c, s) = memory_link_pair(32);

    let source = QueuedLinkSource {
        links: VecDeque::from([(c, None)]),
    };

    let server = tokio::spawn(async move {
        let (s_tx, mut s_rx) = s.split();
        let _ = recv_handshake::<_, ClientHello>(&mut s_rx).await.unwrap();
        send_handshake(&s_tx, &server_hello(resume_key(b"k"), None, false))
            .await
            .unwrap();

        // Send seq 0, then a duplicate seq 0, then seq 1.
        send_frame(&s_tx, 0, None, "first").await;
        send_frame(&s_tx, 0, None, "duplicate-first").await;
        send_frame(&s_tx, 1, None, "second").await;
    });

    let client = StableConduit::<StringFamily, _>::new(source).await.unwrap();
    let (_client_tx, mut client_rx) = client.split();

    let a = client_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*a, "first");

    // The duplicate seq 0 is silently dropped, so next is "second".
    let b = client_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*b, "second");

    server.await.unwrap();
}

/// When the server rejects the resume_key, recv() returns SessionLost.
// r[verify stable.reconnect.failure]
#[tokio::test]
async fn reconnect_failure_surfaces_session_lost() {
    let (c1, s1) = memory_link_pair(32);
    let (c2, s2) = memory_link_pair(32);

    // Server 1: accept initial connection, send ack, then drop.
    let server1 = tokio::spawn(async move {
        let (s1_tx, mut s1_rx) = s1.split();
        let _ = recv_handshake::<_, ClientHello>(&mut s1_rx).await.unwrap();
        send_handshake(&s1_tx, &server_hello(resume_key(b"known-key"), None, false))
            .await
            .unwrap();
        recv_raw(&mut s1_rx).await;
        // Drop — triggers reconnect on client.
    });

    // Server 2: receives reconnect attempt but rejects the resume_key.
    let server2 = tokio::spawn(async move {
        let (s2_tx, mut s2_rx) = s2.split();
        let hello = recv_handshake::<_, ClientHello>(&mut s2_rx).await.unwrap();
        assert!(hello.flags & CH_HAS_RESUME_KEY != 0);
        // Reject the resume attempt.
        send_handshake(&s2_tx, &server_hello(ResumeKey([0u8; 16]), None, true))
            .await
            .unwrap();
    });

    let source = QueuedLinkSource {
        links: VecDeque::from([(c1, None), (c2, None)]),
    };
    let client = StableConduit::<StringFamily, _>::new(source).await.unwrap();
    let (client_tx, mut client_rx) = client.split();

    client_tx
        .reserve()
        .await
        .unwrap()
        .send("hello".to_string())
        .unwrap();

    // server1 drops → reconnect → server2 rejects → SessionLost
    match client_rx.recv().await {
        Err(StableConduitError::SessionLost) => {}
        other => panic!("expected SessionLost, got: {:?}", other.map(|_| ())),
    }

    server1.await.unwrap();
    server2.await.unwrap();
}
