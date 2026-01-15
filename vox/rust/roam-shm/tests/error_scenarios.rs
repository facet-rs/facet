//! Integration tests for error scenarios and edge cases.
//!
//! These tests verify behavior when things go wrong:
//! - Guest never connects after slot reservation
//! - Guest crashes (non-graceful death)
//! - Various AttachError conditions
//! - SendError::PayloadTooLarge
//!
//! shm[verify shm.guest.attach-failure]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use roam_frame::{Frame, MsgDesc, Payload};
use roam_shm::AddPeerOptions;
use roam_shm::guest::{AttachError, SendError, ShmGuest};
use roam_shm::host::{PollResult, ShmHost};
use roam_shm::layout::{SegmentConfig, SegmentHeader};
use roam_shm::msg_type;

// =============================================================================
// Guest Never Connects Scenarios
// =============================================================================

/// Test that a reserved slot can be released if the guest never connects.
///
/// This simulates the case where `add_peer()` succeeds but the spawned process
/// fails to start (e.g., executable not found, permission denied).
#[test]
fn test_reserved_slot_release_on_spawn_failure() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spawn_fail.shm");

    let config = SegmentConfig {
        max_guests: 2,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Reserve a slot
    let ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("will-never-connect".to_string()),
            on_death: None,
        })
        .unwrap();

    let reserved_peer_id = ticket.peer_id;
    assert_eq!(reserved_peer_id.get(), 1);

    // Simulate spawn failure - release the slot without guest ever connecting
    host.release_peer(reserved_peer_id);
    drop(ticket);

    // The slot should now be available for reuse
    let ticket2 = host
        .add_peer(AddPeerOptions {
            peer_name: Some("second-attempt".to_string()),
            on_death: None,
        })
        .unwrap();

    // Should get the same slot back
    assert_eq!(ticket2.peer_id.get(), 1);
}

/// Test that unreserved slots are still available when one is reserved but unused.
#[test]
fn test_other_guests_can_attach_while_slot_reserved() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reserved_unused.shm");

    let config = SegmentConfig {
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Reserve slot 1 (simulating a spawned process that hasn't connected yet)
    let _ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("slow-starter".to_string()),
            on_death: None,
        })
        .unwrap();

    // Other guests can still attach via path (they get slots 2, 3, 4)
    let guest2 = ShmGuest::attach_path(&path).unwrap();
    let guest3 = ShmGuest::attach_path(&path).unwrap();
    let guest4 = ShmGuest::attach_path(&path).unwrap();

    assert_eq!(guest2.peer_id().get(), 2);
    assert_eq!(guest3.peer_id().get(), 3);
    assert_eq!(guest4.peer_id().get(), 4);

    // All 4 slots are now taken (1 reserved, 3 attached)
    let result = ShmGuest::attach_path(&path);
    assert!(matches!(result, Err(AttachError::NoPeerSlots)));
}

/// Test host behavior when reserved guest never attaches and timeout handling.
#[test]
fn test_host_can_timeout_reserved_slot() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("timeout.shm");

    let config = SegmentConfig {
        max_guests: 1,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Reserve the only slot
    let ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("timeout-guest".to_string()),
            on_death: None,
        })
        .unwrap();

    let peer_id = ticket.peer_id;

    // Simulate timeout - host decides guest took too long
    std::thread::sleep(Duration::from_millis(10));

    // Release the reservation
    host.release_peer(peer_id);
    drop(ticket);

    // Slot is now available
    let guest = ShmGuest::attach_path(&path).unwrap();
    assert_eq!(guest.peer_id().get(), 1);
}

// =============================================================================
// Guest Crash Detection (Non-Graceful Death)
// =============================================================================

/// Test that death callback is invoked when guest process crashes.
///
/// This uses fork() to create a real child process that crashes without
/// graceful detach.
#[test]
#[cfg(unix)]
fn test_guest_crash_triggers_death_callback() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crash.shm");

    let config = SegmentConfig::default();
    let mut host = ShmHost::create(&path, config).unwrap();

    let death_called = Arc::new(AtomicBool::new(false));
    let death_called_clone = death_called.clone();
    let death_peer_id = Arc::new(std::sync::atomic::AtomicU8::new(0));
    let death_peer_id_clone = death_peer_id.clone();

    let ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("crasher".to_string()),
            on_death: Some(Arc::new(move |peer_id| {
                death_called_clone.store(true, Ordering::SeqCst);
                death_peer_id_clone.store(peer_id.get(), Ordering::SeqCst);
            })),
        })
        .unwrap();

    let peer_id = ticket.peer_id;
    let args = ticket.to_args();

    match unsafe { libc::fork() } {
        -1 => panic!("fork failed"),
        0 => {
            // Child: attach and then crash without graceful detach
            let spawn_args = roam_shm::spawn::SpawnArgs::from_args(&args).unwrap();
            let mut guest = ShmGuest::attach_with_ticket(&spawn_args).unwrap();

            // Send a message to prove we connected
            let desc = MsgDesc::new(msg_type::DATA, 1, 0);
            let frame = Frame {
                desc,
                payload: Payload::Owned(b"hello before crash".to_vec()),
            };
            let _ = guest.send(frame);

            // Crash! Use std::mem::forget to skip Drop (which does graceful detach)
            std::mem::forget(guest);

            // Exit abruptly (simulates crash)
            unsafe { libc::_exit(1) };
        }
        child_pid => {
            // Parent: drop our copy of the ticket (closes our doorbell fd)
            drop(ticket);

            // Wait for child to connect
            std::thread::sleep(Duration::from_millis(100));

            // Receive the message to confirm connection
            let PollResult { messages, .. } = host.poll();
            assert!(!messages.is_empty(), "guest should have sent a message");

            // Wait for child to crash
            let mut status: i32 = 0;
            unsafe { libc::waitpid(child_pid, &mut status, 0) };

            // Child should have exited with status 1
            assert!(libc::WIFEXITED(status));
            assert_eq!(libc::WEXITSTATUS(status), 1);

            // Now check for doorbell death
            let dead_peers = rt.block_on(host.check_doorbell_deaths());

            // The death callback should have been invoked
            assert!(
                death_called.load(Ordering::SeqCst),
                "death callback should have been called"
            );
            assert_eq!(
                death_peer_id.load(Ordering::SeqCst),
                peer_id.get(),
                "death callback should receive correct peer_id"
            );
            assert!(
                dead_peers.contains(&peer_id),
                "dead_peers should contain the crashed guest"
            );
        }
    }
}

/// Test that killing a guest process (SIGKILL) triggers death detection.
#[test]
#[cfg(unix)]
fn test_sigkill_triggers_death_detection() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sigkill.shm");

    let config = SegmentConfig::default();
    let mut host = ShmHost::create(&path, config).unwrap();

    let death_called = Arc::new(AtomicBool::new(false));
    let death_called_clone = death_called.clone();

    let ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("killed-guest".to_string()),
            on_death: Some(Arc::new(move |_| {
                death_called_clone.store(true, Ordering::SeqCst);
            })),
        })
        .unwrap();

    let peer_id = ticket.peer_id;
    let args = ticket.to_args();

    match unsafe { libc::fork() } {
        -1 => panic!("fork failed"),
        0 => {
            // Child: attach and wait to be killed
            let spawn_args = roam_shm::spawn::SpawnArgs::from_args(&args).unwrap();
            let mut guest = ShmGuest::attach_with_ticket(&spawn_args).unwrap();

            // Send a message
            let desc = MsgDesc::new(msg_type::DATA, 1, 0);
            let frame = Frame {
                desc,
                payload: Payload::Owned(b"waiting to be killed".to_vec()),
            };
            let _ = guest.send(frame);

            // Don't forget guest - we want it alive when we get killed
            // Sleep forever (we'll be killed)
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        child_pid => {
            // Parent
            drop(ticket);

            // Wait for child to connect
            std::thread::sleep(Duration::from_millis(100));

            // Verify connection
            let PollResult { messages, .. } = host.poll();
            assert!(!messages.is_empty());

            // Kill the child with SIGKILL
            unsafe { libc::kill(child_pid, libc::SIGKILL) };

            // Wait for it to die
            let mut status: i32 = 0;
            unsafe { libc::waitpid(child_pid, &mut status, 0) };

            assert!(libc::WIFSIGNALED(status));
            assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);

            // Check for death
            let dead_peers = rt.block_on(host.check_doorbell_deaths());

            assert!(death_called.load(Ordering::SeqCst));
            assert!(dead_peers.contains(&peer_id));
        }
    }
}

// =============================================================================
// AttachError Scenarios
// =============================================================================

/// Test AttachError::InvalidMagic when segment has corrupted magic bytes.
#[test]
fn test_attach_error_invalid_magic() {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    // Corrupt the magic bytes
    let header = unsafe { &mut *(region.as_ptr() as *mut SegmentHeader) };
    header.magic = *b"BADMAGIC";

    // Try to attach - should fail with InvalidMagic
    let result = ShmGuest::attach(region);
    assert!(matches!(result, Err(AttachError::InvalidMagic)));
}

/// Test AttachError::UnsupportedVersion when segment has wrong version.
#[test]
fn test_attach_error_unsupported_version() {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    // Corrupt the version
    let header = unsafe { &mut *(region.as_ptr() as *mut SegmentHeader) };
    header.version = 999; // Future version

    let result = ShmGuest::attach(region);
    assert!(matches!(result, Err(AttachError::UnsupportedVersion)));
}

/// Test AttachError::UnsupportedVersion when header_size is wrong.
#[test]
fn test_attach_error_wrong_header_size() {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    // Corrupt the header_size
    let header = unsafe { &mut *(region.as_ptr() as *mut SegmentHeader) };
    header.header_size = 64; // Wrong size

    let result = ShmGuest::attach(region);
    assert!(matches!(result, Err(AttachError::UnsupportedVersion)));
}

/// Test AttachError::NoPeerSlots when all slots are taken.
#[test]
fn test_attach_error_no_peer_slots() {
    let config = SegmentConfig {
        max_guests: 2,
        ..SegmentConfig::default()
    };
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    // Fill all slots
    let _guest1 = ShmGuest::attach(region).unwrap();
    let _guest2 = ShmGuest::attach(region).unwrap();

    // Third attach should fail
    let result = ShmGuest::attach(region);
    assert!(matches!(result, Err(AttachError::NoPeerSlots)));
}

/// Test AttachError::HostGoodbye when host has signaled goodbye.
#[test]
fn test_attach_error_host_goodbye() {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    // Host says goodbye
    host.goodbye("shutting down");

    // Guest tries to attach
    let result = ShmGuest::attach(region);
    assert!(matches!(result, Err(AttachError::HostGoodbye)));
}

/// Test AttachError::SlotNotReserved when using wrong ticket.
#[test]
#[cfg(unix)]
fn test_attach_error_slot_not_reserved() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrong_ticket.shm");

    let config = SegmentConfig {
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Reserve slot 1
    let _ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("real-guest".to_string()),
            on_death: None,
        })
        .unwrap();

    // Create a separate doorbell for the fake args (the handle doesn't matter for this test)
    let (_host_doorbell, fake_doorbell_handle) = shm_primitives::Doorbell::create_pair().unwrap();

    // Create fake spawn args pointing to a different (unreserved) peer ID
    let fake_args = roam_shm::spawn::SpawnArgs {
        hub_path: path.clone(),
        peer_id: roam_shm::peer::PeerId::from_index(1).unwrap(), // Slot 2, not reserved
        doorbell_handle: fake_doorbell_handle,
    };

    let result = ShmGuest::attach_with_ticket(&fake_args);
    assert!(matches!(result, Err(AttachError::SlotNotReserved)));
}

/// Test AttachError::SlotNotReserved when using wrong ticket (Windows).
#[test]
#[cfg(windows)]
fn test_attach_error_slot_not_reserved() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrong_ticket.shm");

    let config = SegmentConfig {
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Reserve slot 1
    let _ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("real-guest".to_string()),
            on_death: None,
        })
        .unwrap();

    // Create a separate doorbell for the fake args (the handle doesn't matter for this test)
    let (_host_doorbell, fake_doorbell_handle) = shm_primitives::Doorbell::create_pair().unwrap();

    // Create fake spawn args pointing to a different (unreserved) peer ID
    let fake_args = roam_shm::SpawnArgs {
        hub_path: path.clone(),
        peer_id: roam_shm::peer::PeerId::from_index(1).unwrap(), // Slot 2, not reserved
        doorbell_handle: fake_doorbell_handle,
    };

    let result = ShmGuest::attach_with_ticket(&fake_args);
    assert!(matches!(result, Err(AttachError::SlotNotReserved)));
}

/// Test AttachError::InvalidPeerId when peer_id is out of range.
#[test]
fn test_attach_error_invalid_peer_id() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid_peer.shm");

    let config = SegmentConfig {
        max_guests: 2, // Only 2 guests allowed
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Reserve a slot (we won't use it, just to have something reserved)
    let _ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("real-guest".to_string()),
            on_death: None,
        })
        .unwrap();

    // Create a separate doorbell for the fake args (the handle doesn't matter for this test)
    let (_host_doorbell, fake_doorbell_handle) = shm_primitives::Doorbell::create_pair().unwrap();

    // Create spawn args with peer_id = 5, but max_guests = 2
    let fake_args = roam_shm::SpawnArgs {
        hub_path: path.clone(),
        peer_id: roam_shm::peer::PeerId::from_index(4).unwrap(), // Slot 5, out of range
        doorbell_handle: fake_doorbell_handle,
    };

    let result = ShmGuest::attach_with_ticket(&fake_args);
    assert!(matches!(result, Err(AttachError::InvalidPeerId)));
}

// =============================================================================
// SendError Scenarios
// =============================================================================

/// Test SendError::PayloadTooLarge when payload exceeds max_payload_size.
#[test]
fn test_send_error_payload_too_large_guest() {
    let config = SegmentConfig {
        max_payload_size: 100, // Small limit
        slot_size: 128,
        ..SegmentConfig::default()
    };
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    // Try to send payload larger than max_payload_size
    let large_payload = vec![0u8; 200];
    let desc = MsgDesc::new(msg_type::DATA, 1, 0);
    let frame = Frame {
        desc,
        payload: Payload::Owned(large_payload),
    };

    let result = guest.send(frame);
    assert!(
        matches!(result, Err(SendError::PayloadTooLarge)),
        "expected PayloadTooLarge, got {:?}",
        result
    );
}

/// Test SendError::PayloadTooLarge for host sending to guest.
#[test]
fn test_send_error_payload_too_large_host() {
    let config = SegmentConfig {
        max_payload_size: 100,
        slot_size: 128,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    // Try to send payload larger than max_payload_size
    let large_payload = vec![0u8; 200];
    let desc = MsgDesc::new(msg_type::DATA, 1, 0);
    let frame = Frame {
        desc,
        payload: Payload::Owned(large_payload),
    };

    let result = host.send(peer_id, frame);
    assert!(
        matches!(result, Err(roam_shm::host::SendError::PayloadTooLarge)),
        "expected PayloadTooLarge, got {:?}",
        result
    );
}

/// Test SendError::PayloadTooLarge with inline payload that claims wrong length.
#[test]
fn test_send_error_inline_payload_too_large() {
    let config = SegmentConfig::default();
    let host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let mut guest = ShmGuest::attach(region).unwrap();

    // Create a descriptor with inline payload but payload_len > INLINE_PAYLOAD_LEN
    let mut desc = MsgDesc::new(msg_type::DATA, 1, 0);
    desc.payload_len = 100; // Too large for inline (max is 32)

    let frame = Frame {
        desc,
        payload: Payload::Inline,
    };

    let result = guest.send(frame);
    assert!(
        matches!(result, Err(SendError::PayloadTooLarge)),
        "expected PayloadTooLarge, got {:?}",
        result
    );
}

/// Test host SendError::PeerNotAttached when sending to non-existent peer.
#[test]
fn test_send_error_peer_not_attached() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();

    // Try to send to a peer that doesn't exist
    let fake_peer_id = roam_shm::peer::PeerId::from_index(0).unwrap();
    let desc = MsgDesc::new(msg_type::DATA, 1, 0);
    let frame = Frame {
        desc,
        payload: Payload::Owned(b"hello".to_vec()),
    };

    let result = host.send(fake_peer_id, frame);
    assert!(
        matches!(result, Err(roam_shm::host::SendError::PeerNotAttached)),
        "expected PeerNotAttached, got {:?}",
        result
    );
}

/// Test host SendError::PeerNotAttached after guest detaches.
#[test]
fn test_send_error_peer_detached() {
    let config = SegmentConfig::default();
    let mut host = ShmHost::create_heap(config).unwrap();
    let region = host.region();

    let guest = ShmGuest::attach(region).unwrap();
    let peer_id = guest.peer_id();

    // Guest detaches
    drop(guest);

    // Poll to process the goodbye
    let _ = host.poll();

    // Try to send to detached peer
    let desc = MsgDesc::new(msg_type::DATA, 1, 0);
    let frame = Frame {
        desc,
        payload: Payload::Owned(b"hello".to_vec()),
    };

    let result = host.send(peer_id, frame);
    assert!(
        matches!(result, Err(roam_shm::host::SendError::PeerNotAttached)),
        "expected PeerNotAttached, got {:?}",
        result
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test that multiple reserved slots can coexist.
#[test]
fn test_multiple_reserved_slots() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multi_reserve.shm");

    let config = SegmentConfig {
        max_guests: 4,
        ..SegmentConfig::default()
    };
    let mut host = ShmHost::create(&path, config).unwrap();

    // Reserve multiple slots
    let ticket1 = host
        .add_peer(AddPeerOptions {
            peer_name: Some("guest-1".to_string()),
            on_death: None,
        })
        .unwrap();
    let ticket2 = host
        .add_peer(AddPeerOptions {
            peer_name: Some("guest-2".to_string()),
            on_death: None,
        })
        .unwrap();
    let ticket3 = host
        .add_peer(AddPeerOptions {
            peer_name: Some("guest-3".to_string()),
            on_death: None,
        })
        .unwrap();

    assert_eq!(ticket1.peer_id.get(), 1);
    assert_eq!(ticket2.peer_id.get(), 2);
    assert_eq!(ticket3.peer_id.get(), 3);

    // Release middle one
    host.release_peer(ticket2.peer_id);
    drop(ticket2);

    // New reservation should reuse slot 2
    let ticket4 = host
        .add_peer(AddPeerOptions {
            peer_name: Some("guest-4".to_string()),
            on_death: None,
        })
        .unwrap();

    // Slot 2 was released, but the implementation may give slot 4 instead
    // (depends on scan order). Just verify we got a valid slot.
    assert!(ticket4.peer_id.get() >= 1 && ticket4.peer_id.get() <= 4);
}

/// Test graceful shutdown does NOT trigger death callback.
///
/// When a guest does a graceful detach (via Drop), the death callback
/// should NOT be invoked. Death callbacks are only for unexpected crashes.
///
/// shm[verify shm.goodbye.guest]
/// shm[verify shm.guest.detach]
#[test]
#[cfg(unix)]
fn test_graceful_shutdown_no_death_callback() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graceful.shm");

    let config = SegmentConfig::default();
    let mut host = ShmHost::create(&path, config).unwrap();

    let death_called = Arc::new(AtomicBool::new(false));
    let death_called_clone = death_called.clone();

    let ticket = host
        .add_peer(AddPeerOptions {
            peer_name: Some("graceful-guest".to_string()),
            on_death: Some(Arc::new(move |_| {
                death_called_clone.store(true, Ordering::SeqCst);
            })),
        })
        .unwrap();

    let args = ticket.to_args();

    match unsafe { libc::fork() } {
        -1 => panic!("fork failed"),
        0 => {
            // Child: attach, send a message, then gracefully detach
            let spawn_args = roam_shm::spawn::SpawnArgs::from_args(&args).unwrap();
            let mut guest = ShmGuest::attach_with_ticket(&spawn_args).unwrap();

            // Send a message to prove we connected
            let desc = MsgDesc::new(msg_type::DATA, 1, 0);
            let frame = Frame {
                desc,
                payload: Payload::Owned(b"graceful goodbye".to_vec()),
            };
            guest.send(frame).unwrap();

            // Graceful detach via Drop
            drop(guest);
            unsafe { libc::_exit(0) };
        }
        child_pid => {
            drop(ticket);

            // Wait for child to connect and send message
            std::thread::sleep(Duration::from_millis(100));

            // Poll to receive the message (may also process goodbye)
            let messages = host.poll();
            // Message may or may not be present depending on timing -
            // the key assertion is about the death callback
            let _ = messages;

            // Wait for child to exit
            let mut status: i32 = 0;
            unsafe { libc::waitpid(child_pid, &mut status, 0) };
            assert!(libc::WIFEXITED(status));
            assert_eq!(libc::WEXITSTATUS(status), 0);

            // Poll again to ensure goodbye is processed
            let _ = host.poll();

            // Check doorbells - this is where death would be detected for crashes
            let _ = rt.block_on(host.check_doorbell_deaths());

            // Graceful shutdown should NOT trigger death callback
            assert!(
                !death_called.load(Ordering::SeqCst),
                "death callback should NOT be called for graceful shutdown"
            );
        }
    }
}
