use facet_testhelpers::test;
use roam_shm::driver::establish_multi_peer_host;
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;
use std::future::Future;
use std::pin::Pin;

// Mock dispatcher
#[derive(Clone)]
struct MockDispatcher;
impl roam_session::ServiceDispatcher for MockDispatcher {
    fn dispatch(
        &self,
        _ctx: roam_session::Context,
        _payload: Vec<u8>,
        _registry: &mut roam_session::ChannelRegistry,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {})
    }

    fn method_ids(&self) -> Vec<u64> {
        vec![]
    }
}

#[test(tokio::test)]
async fn verify_queuing_order_on_exhaustion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("order.shm");

    // 1 slot only!
    let config = SegmentConfig {
        slots_per_guest: 1,
        ring_size: 64,
        max_guests: 1,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions::default())
        .unwrap();
    let peer_id = ticket.peer_id;

    // Build the driver
    let (host_driver, mut handles, _, _) =
        establish_multi_peer_host(host, vec![(peer_id, MockDispatcher)]);
    let handle = handles.remove(&peer_id).unwrap();

    // Spawn driver
    tokio::spawn(host_driver.run());

    let spawn_args = ticket.into_spawn_args();
    // Re-attach as guest immediately so Host can send.
    // We attach by path (ticket consumed), but we need to ensure the guest is "Attached".
    let mut guest = roam_shm::guest::ShmGuest::attach_with_ticket(&spawn_args).unwrap();
    // Create doorbell from handle to verify backpressure processing
    let guest_doorbell = shm_primitives::Doorbell::from_handle(spawn_args.doorbell_handle).unwrap();

    // 1. Occupy the SINGLE slot with a large message
    // We can't easily control the "other" side to keep the slot occupied.
    // But we can send TWO large messages.
    // The first will succeed (taking the slot).
    // The second (large) will fail (queued).
    // Then we send a THIRD (small). It MUST be queued (ordering).

    let large_payload = vec![0u8; 1000]; // Assume > 32 bytes (inline limit)
    let small_payload = vec![0u8; 10]; // Assume <= 32 bytes

    // Msg 1: Large (Takes slot)
    // The Guest isn't consuming, so the slot stays allocated!
    // Wait, ShmHost allocates slot for Host->Guest message.
    // Host writes to ring.
    // Slot remains allocated until Guest acknowledges?
    // NO. Slot is freed when Guest *reads* it?
    // NO. Guest reads descriptor. If descriptor format.
    // Guest calls `get_payload` -> frees slot.

    // Since we have no Guest running consuming messages (we control it manually),
    // the Ring will just fill up?
    // NO. We read manually.

    // Use call_raw to send messages
    // Msg 1
    println!("Sending Msg 1 (Large)");
    let h1 = handle.clone();
    let p1 = large_payload.clone();
    tokio::spawn(async move {
        let _ = h1.call_raw(1, p1).await;
    });
    // This should succeed and consume the slot.

    // Msg 2
    println!("Sending Msg 2 (Large) - Expect Queue");
    // Since slot is taken (we haven't read Msg 1 yet), this should be queued.
    let large_payload_2 = large_payload.clone();
    let h2 = handle.clone();
    tokio::spawn(async move {
        let _ = h2.call_raw(2, large_payload_2).await;
    });

    // Give driver time to process and queue Msg 2
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Msg 3: Small
    println!("Sending Msg 3 (Small) - Expect Queue (ordering)");
    let small_payload_3 = small_payload.clone();
    let h3 = handle.clone();
    tokio::spawn(async move {
        let _ = h3.call_raw(3, small_payload_3).await;
    });

    // Give driver time to process Msg 3
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Now, let's verify what's in the ring/queue.

    // Read Msg 1
    let frame1 = guest.recv().expect("Msg 1 missing");
    assert!(!frame1.payload.is_inline(), "Msg 1 should be large");
    println!("Received Msg 1");

    // Reading Msg 1 frees the slot in the guest's view, but Host needs doorbell to know!
    // Signal doorbell manually.
    guest_doorbell.signal().await;

    // Give host driver time to wake up and process retries
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Msg 2 was Queued. It should now be sent (since slot is free).
    // Msg 3 was Queued. It should now be sent (after Msg 2).

    // Check Msg 2
    let frame2 = guest.recv().expect("Msg 2 missing after retry");
    assert!(!frame2.payload.is_inline(), "Msg 2 should be large");
    println!("Received Msg 2");

    // Check Msg 3
    let frame3 = guest.recv().expect("Msg 3 missing after retry");
    assert!(frame3.payload.is_inline(), "Msg 3 should be inline (small)");
    println!("Received Msg 3");

    // Ring empty
    assert!(guest.recv().is_none());
}
