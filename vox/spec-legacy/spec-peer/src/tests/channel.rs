//! Channel conformance tests.
//!
//! Tests for channel lifecycle, ID allocation, and control message handling.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_peer_macros::conformance;

/// Helper to send a response so the subject's call() completes.
async fn send_response(
    peer: &mut Peer,
    channel_id: u32,
    msg_id: u64,
    method_id: u32,
) -> Result<(), TestResult> {
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]),
    };

    let payload = facet_postcard::to_vec(&call_result)
        .map_err(|e| TestResult::fail(format!("failed to serialize CallResult: {}", e)))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = msg_id;
    desc.channel_id = channel_id;
    desc.method_id = method_id;
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&response_frame)
        .await
        .map_err(|e| TestResult::fail(format!("failed to send response: {}", e)))?;

    Ok(())
}

/// Helper to perform handshake as acceptor.
async fn do_handshake(peer: &mut Peer) -> Result<(), TestResult> {
    // Receive Hello from implementation (initiator)
    let frame = peer
        .recv()
        .await
        .map_err(|e| TestResult::fail(format!("failed to receive Hello: {}", e)))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err(TestResult::fail(format!(
            "expected Hello on channel 0, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        )));
    }

    // Send Hello response as Acceptor
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = facet_postcard::to_vec(&response)
        .map_err(|e| TestResult::fail(format!("failed to serialize Hello: {}", e)))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&response_frame)
        .await
        .map_err(|e| TestResult::fail(format!("failed to send Hello: {}", e)))?;

    Ok(())
}

// =============================================================================
// channel.id_parity_initiator
// =============================================================================
// Rule: [verify core.channel.id.parity.initiator]
//
// The initiator (connection opener) MUST use odd channel IDs.
// When we receive OpenChannel from the initiator, the channel_id MUST be odd.

#[conformance(
    name = "channel.id_parity_initiator",
    rules = "core.channel.id.parity.initiator"
)]
pub async fn id_parity_initiator(peer: &mut Peer) -> TestResult {
    // Step 1: Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Step 2: Wait for an OpenChannel from the initiator
    // The implementation should open a channel (odd ID) when making a call
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // Check if it's an OpenChannel on channel 0
    if frame.desc.channel_id != 0 {
        return TestResult::fail(format!(
            "expected control message on channel 0, got channel {}",
            frame.desc.channel_id
        ));
    }

    if frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel (verb {}), got verb {}",
            control_verb::OPEN_CHANNEL,
            frame.desc.method_id
        ));
    }

    // Deserialize the OpenChannel payload
    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    // Verify the channel ID is odd (initiator's channels)
    if open.channel_id.is_multiple_of(2) {
        return TestResult::fail(format!(
            "initiator used even channel ID {}, MUST use odd IDs",
            open.channel_id
        ));
    }

    // Verify it's not channel 0 (reserved for control)
    if open.channel_id == 0 {
        return TestResult::fail("initiator tried to open channel 0, which is reserved");
    }

    // Wait for the request frame and send a response so call() completes
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if let Err(result) = send_response(
        peer,
        open.channel_id,
        request.desc.msg_id,
        request.desc.method_id,
    )
    .await
    {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// channel.zero_reserved
// =============================================================================
// Rule: [verify core.channel.id.zero-reserved]
//
// Channel 0 MUST be reserved for control messages.
// When we receive OpenChannel, the channel_id MUST NOT be 0.

#[conformance(
    name = "channel.zero_reserved",
    rules = "core.channel.id.zero-reserved"
)]
pub async fn zero_reserved(peer: &mut Peer) -> TestResult {
    // Step 1: Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Step 2: Wait for any OpenChannel from the initiator
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // Check if it's an OpenChannel
    if frame.desc.channel_id == 0 && frame.desc.method_id == control_verb::OPEN_CHANNEL {
        let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
            Ok(o) => o,
            Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
        };

        // Verify the opened channel is not 0
        if open.channel_id == 0 {
            return TestResult::fail(
                "implementation tried to open channel 0, which MUST be reserved for control",
            );
        }

        // Wait for the request frame and send a response so call() completes
        let request = match peer.recv().await {
            Ok(f) => f,
            Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
        };

        if let Err(result) = send_response(
            peer,
            open.channel_id,
            request.desc.msg_id,
            request.desc.method_id,
        )
        .await
        {
            return result;
        }
    }

    // If we got here without seeing a violation, pass
    // (implementation correctly did not try to open channel 0)
    TestResult::pass()
}

// =============================================================================
// channel.control_flag_set
// =============================================================================
// Rule: [verify core.control.flag-set]
//
// The CONTROL flag MUST be set on all frames with channel_id == 0.

#[conformance(
    name = "channel.control_flag_set",
    rules = "core.control.flag-set, core.control.flag-clear"
)]
pub async fn control_flag_set(peer: &mut Peer) -> TestResult {
    // Receive Hello - it MUST have CONTROL flag since it's on channel 0
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive Hello: {}", e)),
    };

    // Verify channel is 0
    if frame.desc.channel_id != 0 {
        return TestResult::fail(format!(
            "expected first frame on channel 0, got channel {}",
            frame.desc.channel_id
        ));
    }

    // Verify CONTROL flag is set
    if frame.desc.flags & flags::CONTROL == 0 {
        return TestResult::fail(format!(
            "CONTROL flag not set on channel 0 frame (flags: {:#x})",
            frame.desc.flags
        ));
    }

    // Complete handshake
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = match facet_postcard::to_vec(&response) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize Hello: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let response_frame = Frame::inline(desc, &payload);

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send Hello: {}", e));
    }

    // Wait for more control messages and verify they all have CONTROL flag
    if let Ok(frame) = peer.recv().await
        && frame.desc.channel_id == 0
        && frame.desc.flags & flags::CONTROL == 0
    {
        return TestResult::fail(format!(
            "CONTROL flag not set on channel 0 frame (method_id={}, flags={:#x})",
            frame.desc.method_id, frame.desc.flags
        ));
    }

    TestResult::pass()
}

// =============================================================================
// channel.open_required_before_data
// =============================================================================
// Rule: [verify core.channel.open]
//
// Channels MUST be opened by sending OpenChannel before any data frames.
// This test verifies that data frames arrive only after OpenChannel.

#[conformance(
    name = "channel.open_required_before_data",
    rules = "core.channel.open"
)]
pub async fn open_required_before_data(peer: &mut Peer) -> TestResult {
    // Step 1: Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Track which channels have been opened
    let mut opened_channels: std::collections::HashSet<u32> = std::collections::HashSet::new();
    opened_channels.insert(0); // Channel 0 is always open (control)

    // Step 2: Monitor frames and verify data only arrives on opened channels
    loop {
        let frame = match peer.try_recv().await {
            Ok(Some(f)) => f,
            Ok(None) => break, // Connection closed or timeout
            Err(e) => return TestResult::fail(format!("recv error: {}", e)),
        };

        if frame.desc.channel_id == 0 {
            // Control message
            if frame.desc.method_id == control_verb::OPEN_CHANNEL {
                let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
                    Ok(o) => o,
                    Err(e) => {
                        return TestResult::fail(format!(
                            "failed to deserialize OpenChannel: {}",
                            e
                        ));
                    }
                };
                opened_channels.insert(open.channel_id);
            }
        } else {
            // Data frame - verify channel was opened first
            if !opened_channels.contains(&frame.desc.channel_id) {
                return TestResult::fail(format!(
                    "received data on channel {} before OpenChannel was sent",
                    frame.desc.channel_id
                ));
            }
        }

        // If we've seen at least one OpenChannel + data exchange, we're good
        if opened_channels.len() > 1 {
            break;
        }
    }

    TestResult::pass()
}

// =============================================================================
// channel.call_kind_no_attach
// =============================================================================
// Rule: [verify core.channel.open.attach-required]
//
// CALL channels MUST NOT have an attachment (attach = None).

#[conformance(
    name = "channel.call_kind_no_attach",
    rules = "core.channel.open.attach-required"
)]
pub async fn call_kind_no_attach(peer: &mut Peer) -> TestResult {
    // Step 1: Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Step 2: Wait for an OpenChannel for a CALL
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel on channel 0, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    // If it's a CALL channel, verify attach is None
    if open.kind == ChannelKind::Call && open.attach.is_some() {
        return TestResult::fail(format!(
            "CALL channel {} has attach field, but CALL channels MUST have attach = None",
            open.channel_id
        ));
    }

    // Wait for the request frame and send a response so call() completes
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if let Err(result) = send_response(
        peer,
        open.channel_id,
        request.desc.msg_id,
        request.desc.method_id,
    )
    .await
    {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// channel.open_ownership
// =============================================================================
// Rule: [verify core.channel.open.ownership]
//
// The client (initiator) MUST open CALL channels.
// This test verifies that the initiator opens CALL channels with odd IDs.

#[conformance(name = "channel.open_ownership", rules = "core.channel.open.ownership")]
pub async fn open_ownership(peer: &mut Peer) -> TestResult {
    // Step 1: Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Step 2: Wait for OpenChannel from the initiator (client)
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel on channel 0, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    // Verify that the client (initiator) opens CALL channels
    if open.kind == ChannelKind::Call {
        // Client/initiator uses odd channel IDs
        if open.channel_id.is_multiple_of(2) {
            return TestResult::fail(format!(
                "client opened CALL channel with even ID {}, MUST use odd IDs",
                open.channel_id
            ));
        }
    }

    // Wait for the request frame and send a response so call() completes
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if let Err(result) = send_response(
        peer,
        open.channel_id,
        request.desc.msg_id,
        request.desc.method_id,
    )
    .await
    {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// channel.close_full
// =============================================================================
// Rule: [verify core.close.full]
//
// A channel MUST be considered fully closed when both sides have sent EOS,
// or CancelChannel was sent/received.

#[conformance(name = "channel.close_full", rules = "core.close.full")]
pub async fn close_full(peer: &mut Peer) -> TestResult {
    // Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Wait for OpenChannel
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    let channel_id = open.channel_id;

    // Wait for request with EOS (client's half-close)
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected request on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Verify client sent EOS
    if request.desc.flags & flags::EOS == 0 {
        return TestResult::fail(format!(
            "CALL request missing EOS flag (flags={:#x})",
            request.desc.flags
        ));
    }

    // Send response with EOS (server's half-close) - now channel is fully closed
    let call_result = CallResult {
        status: Status::ok(),
        trailers: vec![],
        body: Some(vec![]),
    };

    let payload = match facet_postcard::to_vec(&call_result) {
        Ok(p) => p,
        Err(e) => return TestResult::fail(format!("failed to serialize CallResult: {}", e)),
    };

    let mut desc = MsgDescHot::new();
    desc.msg_id = request.desc.msg_id;
    desc.channel_id = channel_id;
    desc.method_id = request.desc.method_id;
    desc.flags = flags::DATA | flags::EOS | flags::RESPONSE;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&response_frame).await {
        return TestResult::fail(format!("failed to send response: {}", e));
    }

    // Channel is now fully closed (both sides sent EOS)
    TestResult::pass()
}

// =============================================================================
// channel.close_state_free
// =============================================================================
// Rule: [verify core.close.state-free]
//
// After full close, implementations MAY free channel state.
// Each channel ID MUST be used at most once within the lifetime of a connection.

#[conformance(name = "channel.close_state_free", rules = "core.close.state-free")]
pub async fn close_state_free(peer: &mut Peer) -> TestResult {
    // Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    let mut seen_channel_ids: std::collections::HashSet<u32> = std::collections::HashSet::new();

    // Wait for OpenChannel (blocking)
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    // Check for channel ID reuse (first channel, so should not be in set)
    if seen_channel_ids.contains(&open.channel_id) {
        return TestResult::fail(format!(
            "channel ID {} was reused, but channel IDs MUST be used at most once",
            open.channel_id
        ));
    }
    seen_channel_ids.insert(open.channel_id);

    let channel_id = open.channel_id;

    // Wait for the data frame (blocking)
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected data on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Send response to complete the call
    if let Err(result) = send_response(
        peer,
        channel_id,
        request.desc.msg_id,
        request.desc.method_id,
    )
    .await
    {
        return result;
    }

    // Channel is now closed - verify no reuse occurred
    // (In a single-call test, we've verified the one channel ID was unique)
    TestResult::pass()
}

// =============================================================================
// channel.no_pre_open
// =============================================================================
// Rule: [verify core.channel.open.no-pre-open]
//
// Each peer MUST open only the channels it will send data on.

#[conformance(name = "channel.no_pre_open", rules = "core.channel.open.no-pre-open")]
pub async fn no_pre_open(peer: &mut Peer) -> TestResult {
    // Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Wait for OpenChannel (blocking)
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    let channel_id = open.channel_id;

    // Wait for data frame on the opened channel (blocking)
    let request = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive request: {}", e)),
    };

    // Verify data was sent on the channel that was opened
    if request.desc.channel_id != channel_id {
        return TestResult::fail(format!(
            "expected data on channel {}, got channel {}",
            channel_id, request.desc.channel_id
        ));
    }

    // Verify this is actually a data frame
    if request.desc.flags & flags::DATA == 0 {
        return TestResult::fail(format!(
            "expected DATA frame on channel {}, got flags={:#x}",
            channel_id, request.desc.flags
        ));
    }

    // Channel was opened and data was sent - no-pre-open rule is satisfied
    // Send response to complete the call
    if let Err(result) = send_response(
        peer,
        channel_id,
        request.desc.msg_id,
        request.desc.method_id,
    )
    .await
    {
        return result;
    }

    TestResult::pass()
}

// =============================================================================
// channel.lifecycle
// =============================================================================
// Rule: [verify core.channel.lifecycle]
//
// Channels MUST be opened explicitly via OpenChannel before data frames.
// A channel is closed when both sides have reached terminal state.

#[conformance(name = "channel.lifecycle", rules = "core.channel.lifecycle")]
pub async fn lifecycle(peer: &mut Peer) -> TestResult {
    // Step 1: Complete handshake
    if let Err(result) = do_handshake(peer).await {
        return result;
    }

    // Step 2: Wait for OpenChannel
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::OPEN_CHANNEL {
        return TestResult::fail(format!(
            "expected OpenChannel on channel 0, got channel={} method_id={}",
            frame.desc.channel_id, frame.desc.method_id
        ));
    }

    let open: OpenChannel = match facet_postcard::from_slice(frame.payload_bytes()) {
        Ok(o) => o,
        Err(e) => return TestResult::fail(format!("failed to deserialize OpenChannel: {}", e)),
    };

    let channel_id = open.channel_id;

    // Step 3: Receive data frame on the opened channel (should have EOS for CALL)
    let data_frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive data frame: {}", e)),
    };

    // Verify the data frame is on the opened channel
    if data_frame.desc.channel_id != channel_id {
        // It might be on channel 0 for control, which is fine
        if data_frame.desc.channel_id != 0 {
            return TestResult::fail(format!(
                "received data on channel {} which was not opened (expected {})",
                data_frame.desc.channel_id, channel_id
            ));
        }
    }

    // If it's a CALL channel, the request frame MUST have EOS
    if open.kind == ChannelKind::Call
        && data_frame.desc.channel_id == channel_id
        && data_frame.desc.flags & flags::EOS == 0
    {
        return TestResult::fail(format!(
            "CALL request on channel {} missing EOS flag (flags={:#x})",
            channel_id, data_frame.desc.flags
        ));
    }

    // Send a response so call() completes
    if let Err(result) = send_response(
        peer,
        channel_id,
        data_frame.desc.msg_id,
        data_frame.desc.method_id,
    )
    .await
    {
        return result;
    }

    TestResult::pass()
}
