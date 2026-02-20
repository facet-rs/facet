//! Targeted unit tests for the backpressure/slot-exhaustion mechanism.
//!
//! Each test validates one specific assumption about how the system behaves
//! under slot exhaustion, rather than exercising the full driver stack.

use facet_testhelpers::test;
use roam_shm::host::ShmHost;
use roam_shm::layout::SegmentConfig;

/// Returns a config with a tiny slot pool to provoke exhaustion quickly.
fn tiny_slot_config() -> SegmentConfig {
    SegmentConfig {
        bipbuf_capacity: 64 * 1024,
        max_guests: 2,
        ..SegmentConfig::default()
    }
}

/// Assumption: when the host sends a large message (takes a slot) and the
/// guest reads it (freeing the slot), the same slot becomes available for
/// a subsequent host send.
#[test(tokio::test)]
async fn slot_freed_by_guest_read_is_reusable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slot_reuse.shm");

    let mut host = ShmHost::create(&path, tiny_slot_config()).unwrap();
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions::default())
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();
    let mut guest = roam_shm::guest::ShmGuest::attach_with_ticket(&spawn_args).unwrap();

    // Figure out how many large messages it takes to exhaust all slots by
    // sending until we get SlotExhausted.
    let large = vec![0u8; 5000];
    let mut sent = 0usize;
    loop {
        let msg = roam_shm::msg::ShmMsg::new(1, sent as u32, 1, large.clone());
        match host.send(peer_id, &msg) {
            Ok(()) => sent += 1,
            Err(roam_shm::host::SendError::SlotExhausted) => break,
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
    assert!(
        sent > 0,
        "should have sent at least one message before exhaustion"
    );
    eprintln!("slots exhausted after {sent} messages");

    // Now read ONE message from the guest side, freeing one slot.
    let frame = guest.recv().expect("should have a message");
    assert_eq!(
        frame.payload_bytes().len(),
        5000 + /* overhead framing in ShmMsg payload? */ 0
    );
    drop(frame);

    // The host should now be able to send one more large message.
    let msg = roam_shm::msg::ShmMsg::new(1, 999, 1, large.clone());
    match host.send(peer_id, &msg) {
        Ok(()) => eprintln!("correctly sent after slot freed by guest read"),
        Err(roam_shm::host::SendError::SlotExhausted) => {
            panic!("slot still exhausted after guest freed one slot");
        }
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

/// Assumption: when the guest sends a large message (takes a slot) and the
/// host reads it (via poll, freeing the slot), the host correctly includes
/// this peer in `slots_freed_for`.
#[test(tokio::test)]
async fn slots_freed_for_populated_when_host_reads_guest_slot_msg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("slots_freed_for.shm");

    let mut host = ShmHost::create(&path, tiny_slot_config()).unwrap();
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions::default())
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();
    let mut guest = roam_shm::guest::ShmGuest::attach_with_ticket(&spawn_args).unwrap();

    // Guest sends a large message (will use a slot).
    let large = vec![0u8; 5000];
    let msg = roam_shm::msg::ShmMsg::new(1, 1, 1, large);
    guest.send(&msg).expect("guest send should succeed");

    // Host polls - should see the message and mark peer in slots_freed_for.
    let result = host.poll();
    assert!(
        result.messages.iter().any(|(pid, _)| *pid == peer_id),
        "host should see the guest message"
    );
    assert!(
        result.slots_freed_for.contains(&peer_id),
        "host should report slot freed for peer after reading slot-ref guest message; got: {:?}",
        result.slots_freed_for
    );
}

/// Assumption: when ALL slots are taken by host→guest messages, and the
/// guest reads them all (freeing the slots), the guest can then use those
/// freed slots to send responses back.
#[test(tokio::test)]
async fn guest_can_send_after_host_exhausts_then_guest_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exhaust_then_read.shm");

    let mut host = ShmHost::create(&path, tiny_slot_config()).unwrap();
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions::default())
        .unwrap();
    let peer_id = ticket.peer_id;
    let spawn_args = ticket.into_spawn_args();
    let mut guest = roam_shm::guest::ShmGuest::attach_with_ticket(&spawn_args).unwrap();

    // Fill all slots from host→guest.
    let large = vec![0u8; 5000];
    let mut sent = 0usize;
    loop {
        let msg = roam_shm::msg::ShmMsg::new(1, sent as u32, 1, large.clone());
        match host.send(peer_id, &msg) {
            Ok(()) => sent += 1,
            Err(roam_shm::host::SendError::SlotExhausted) => break,
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
    eprintln!("filled {sent} slots from host→guest");

    // Verify guest CANNOT send (slots exhausted).
    let response = roam_shm::msg::ShmMsg::new(1, 0, 1, large.clone());
    match guest.send(&response) {
        Err(roam_shm::guest::SendError::SlotExhausted) => {
            eprintln!("confirmed: guest slot-exhausted before reading host messages");
        }
        Ok(()) => eprintln!("note: guest was still able to send (pool has slack)"),
        Err(e) => panic!("unexpected send error: {e:?}"),
    }

    // Guest reads ALL host messages (freeing all slots).
    for i in 0..sent {
        let frame = guest.recv().expect(&format!("should have message {i}"));
        drop(frame);
    }
    eprintln!("guest read all {sent} messages from host");

    // Now guest should be able to send a response.
    let response = roam_shm::msg::ShmMsg::new(1, 0, 1, large.clone());
    match guest.send(&response) {
        Ok(()) => eprintln!("guest can send after reading host messages ✓"),
        Err(roam_shm::guest::SendError::SlotExhausted) => {
            panic!(
                "guest STILL slot-exhausted after reading all host messages - slot accounting broken"
            );
        }
        Err(e) => panic!("unexpected send error: {e:?}"),
    }
}

/// Assumption: the H2G and G2H doorbell signals are truly independent.
/// Ringing G2H should not consume an H2G signal, and vice versa.
#[test(tokio::test)]
async fn doorbell_signals_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doorbell_independence.shm");

    let mut host = ShmHost::create(&path, tiny_slot_config()).unwrap();
    let ticket = host
        .add_peer(roam_shm::spawn::AddPeerOptions::default())
        .unwrap();
    let peer_id = ticket.peer_id;
    let host_doorbell = host.take_doorbell(peer_id).unwrap();
    let spawn_args = ticket.into_spawn_args();
    let guest_doorbell = shm_primitives::Doorbell::from_handle(spawn_args.doorbell_handle).unwrap();

    // Ring H2G (host signals guest).
    host_doorbell.signal().await;

    // Ring G2H (guest signals host).
    guest_doorbell.signal().await;

    // Both signals should be independently receivable.
    let h2g_result = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        guest_doorbell.wait(), // guest waits for H2G
    )
    .await;
    assert!(
        h2g_result.is_ok(),
        "H2G signal should arrive at guest without timeout"
    );

    let g2h_result = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        host_doorbell.wait(), // host waits for G2H
    )
    .await;
    assert!(
        g2h_result.is_ok(),
        "G2H signal should arrive at host without timeout"
    );
}
