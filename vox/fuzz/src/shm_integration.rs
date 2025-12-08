//! Integration model for SHM transport fuzzing.
//!
//! This module combines the ring and slab models to fuzz the full
//! SHM send/receive pattern:
//! 1. Allocate slot from slab
//! 2. Write payload to slot
//! 3. Enqueue descriptor to ring
//! 4. Mark slot as in-flight
//! 5. Consumer dequeues descriptor
//! 6. Consumer reads payload
//! 7. Consumer frees slot

use crate::ring_model::{RingModel, TestDescriptor, RingError};
use crate::slab_model::{SlabError, SlabModel, SlotHandle};

/// Simulated message being sent through SHM.
#[derive(Debug, Clone)]
pub struct Message {
    pub id: u64,
    pub payload_len: u32,
}

/// Producer side of the SHM transport.
pub struct ShmProducer {
    ring: RingModel,
    slab: SlabModel,
    next_msg_id: u64,
    /// Slots that are allocated but not yet in-flight.
    pending_slots: Vec<(SlotHandle, Message)>,
}

impl ShmProducer {
    pub fn new(ring_capacity: u32, slot_count: u32) -> Self {
        Self {
            ring: RingModel::new(ring_capacity),
            slab: SlabModel::new(slot_count),
            next_msg_id: 0,
            pending_slots: Vec::new(),
        }
    }

    /// Try to send a message.
    ///
    /// Returns the message ID on success.
    pub fn send(&mut self, payload_len: u32) -> Result<u64, ShmError> {
        // 1. Allocate slot
        let slot = self.slab.alloc().map_err(ShmError::Slab)?;

        let msg_id = self.next_msg_id;
        self.next_msg_id += 1;

        let message = Message { id: msg_id, payload_len };

        // 2. Enqueue descriptor (uses slot index as the descriptor ID for simplicity)
        let desc = TestDescriptor { id: encode_desc(slot.index, slot.generation, msg_id) };

        match self.ring.enqueue(desc) {
            Ok(()) => {
                // 3. Mark slot as in-flight
                self.slab.mark_in_flight(slot).map_err(ShmError::Slab)?;
                Ok(msg_id)
            }
            Err(RingError::Full) => {
                // Ring is full - we need to "free" the slot (back to Free state)
                // In real code, we'd keep the slot Allocated and retry later.
                // For the model, we'll simulate "abort" by transitioning back.
                // But our slab model doesn't have a direct Allocated->Free transition.
                // Instead, we track pending slots separately.
                self.pending_slots.push((slot, message));
                Err(ShmError::RingFull)
            }
        }
    }

    /// Retry sending pending messages after the consumer freed some space.
    pub fn retry_pending(&mut self) -> Vec<u64> {
        let mut sent = Vec::new();
        let pending = std::mem::take(&mut self.pending_slots);

        for (slot, message) in pending {
            let desc = TestDescriptor { id: encode_desc(slot.index, slot.generation, message.id) };

            match self.ring.enqueue(desc) {
                Ok(()) => {
                    if self.slab.mark_in_flight(slot).is_ok() {
                        sent.push(message.id);
                    }
                }
                Err(RingError::Full) => {
                    // Still full, put back
                    self.pending_slots.push((slot, message));
                }
            }
        }

        sent
    }

    /// Get the slab model for inspection.
    pub fn slab(&self) -> &SlabModel {
        &self.slab
    }

    /// Get the ring model for inspection.
    pub fn ring(&self) -> &RingModel {
        &self.ring
    }
}

/// Consumer side of the SHM transport.
pub struct ShmConsumer {
    /// Reference to producer's ring (in real SHM, this is in shared memory).
    /// For the model, we pass it by reference.
    ring: RingModel,
    /// Reference to producer's slab.
    slab: SlabModel,
    /// Messages that have been received but not yet freed.
    received: Vec<(SlotHandle, Message)>,
}

impl ShmConsumer {
    pub fn new(ring_capacity: u32, slot_count: u32) -> Self {
        Self {
            ring: RingModel::new(ring_capacity),
            slab: SlabModel::new(slot_count),
            received: Vec::new(),
        }
    }

    /// Try to receive a message from the producer.
    ///
    /// Returns None if no messages available.
    pub fn recv(&mut self) -> Option<Message> {
        // Dequeue descriptor from ring
        let desc = self.ring.dequeue()?;

        // Decode slot info
        let (slot_idx, slot_gen, msg_id) = decode_desc(desc.id);

        let slot = SlotHandle {
            index: slot_idx,
            generation: slot_gen,
        };

        let message = Message {
            id: msg_id,
            payload_len: 0, // Simplified - we don't track payload in the model
        };

        self.received.push((slot, message.clone()));

        Some(message)
    }

    /// Free a received message's slot.
    ///
    /// Returns true if the slot was freed.
    pub fn free(&mut self, msg_id: u64) -> Result<(), ShmError> {
        if let Some(pos) = self.received.iter().position(|(_, m)| m.id == msg_id) {
            let (slot, _) = self.received.remove(pos);
            self.slab.free(slot).map_err(ShmError::Slab)?;
            Ok(())
        } else {
            Err(ShmError::MessageNotFound)
        }
    }

    /// Get the slab model for inspection.
    pub fn slab(&self) -> &SlabModel {
        &self.slab
    }

    /// Get the ring model for inspection.
    pub fn ring(&self) -> &RingModel {
        &self.ring
    }

    /// Get count of received but not freed messages.
    pub fn pending_count(&self) -> usize {
        self.received.len()
    }
}

/// Encode slot info into a descriptor ID.
fn encode_desc(slot_idx: u32, slot_gen: u32, msg_id: u64) -> u64 {
    // Pack: [msg_id:32][slot_gen:16][slot_idx:16]
    ((msg_id & 0xFFFF_FFFF) << 32) | ((slot_gen as u64 & 0xFFFF) << 16) | (slot_idx as u64 & 0xFFFF)
}

/// Decode slot info from a descriptor ID.
fn decode_desc(id: u64) -> (u32, u32, u64) {
    let slot_idx = (id & 0xFFFF) as u32;
    let slot_gen = ((id >> 16) & 0xFFFF) as u32;
    let msg_id = (id >> 32) & 0xFFFF_FFFF;
    (slot_idx, slot_gen, msg_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShmError {
    Slab(SlabError),
    RingFull,
    MessageNotFound,
}

/// Bidirectional SHM model (simulates a full duplex SHM session).
pub struct ShmSession {
    /// A→B direction
    pub a_to_b_ring: RingModel,
    pub shared_slab: SlabModel,

    /// Producer state for A side
    pub a_next_msg_id: u64,
    pub a_pending_slots: Vec<(SlotHandle, Message)>,

    /// Consumer state for B side (messages received by B)
    pub b_received: Vec<(SlotHandle, Message)>,
}

impl ShmSession {
    pub fn new(ring_capacity: u32, slot_count: u32) -> Self {
        Self {
            a_to_b_ring: RingModel::new(ring_capacity),
            shared_slab: SlabModel::new(slot_count),
            a_next_msg_id: 0,
            a_pending_slots: Vec::new(),
            b_received: Vec::new(),
        }
    }

    /// A sends a message to B.
    pub fn a_send(&mut self, payload_len: u32) -> Result<u64, ShmError> {
        // Allocate slot
        let slot = self.shared_slab.alloc().map_err(ShmError::Slab)?;

        let msg_id = self.a_next_msg_id;
        self.a_next_msg_id += 1;

        let message = Message { id: msg_id, payload_len };

        // Enqueue descriptor
        let desc = TestDescriptor { id: encode_desc(slot.index, slot.generation, msg_id) };

        match self.a_to_b_ring.enqueue(desc) {
            Ok(()) => {
                self.shared_slab.mark_in_flight(slot).map_err(ShmError::Slab)?;
                Ok(msg_id)
            }
            Err(RingError::Full) => {
                self.a_pending_slots.push((slot, message));
                Err(ShmError::RingFull)
            }
        }
    }

    /// B receives a message from A.
    pub fn b_recv(&mut self) -> Option<Message> {
        let desc = self.a_to_b_ring.dequeue()?;

        let (slot_idx, slot_gen, msg_id) = decode_desc(desc.id);

        let slot = SlotHandle {
            index: slot_idx,
            generation: slot_gen,
        };

        let message = Message {
            id: msg_id,
            payload_len: 0,
        };

        self.b_received.push((slot, message.clone()));

        Some(message)
    }

    /// B frees a received message's slot.
    pub fn b_free(&mut self, msg_id: u64) -> Result<(), ShmError> {
        if let Some(pos) = self.b_received.iter().position(|(_, m)| m.id == msg_id) {
            let (slot, _) = self.b_received.remove(pos);
            self.shared_slab.free(slot).map_err(ShmError::Slab)?;
            Ok(())
        } else {
            Err(ShmError::MessageNotFound)
        }
    }

    /// A retries pending sends.
    pub fn a_retry_pending(&mut self) -> Vec<u64> {
        let mut sent = Vec::new();
        let pending = std::mem::take(&mut self.a_pending_slots);

        for (slot, message) in pending {
            let desc = TestDescriptor { id: encode_desc(slot.index, slot.generation, message.id) };

            match self.a_to_b_ring.enqueue(desc) {
                Ok(()) => {
                    if self.shared_slab.mark_in_flight(slot).is_ok() {
                        sent.push(message.id);
                    }
                }
                Err(RingError::Full) => {
                    self.a_pending_slots.push((slot, message));
                }
            }
        }

        sent
    }
}

/// Operations for SHM integration fuzzing.
#[derive(Clone, Debug)]
pub enum ShmOp {
    /// A sends a message with given payload length.
    Send(u16),
    /// B receives a message (if available).
    Recv,
    /// B frees a message (uses index into received list).
    Free(u8),
    /// A retries pending sends.
    Retry,
}

/// Execute a sequence of SHM operations and verify invariants.
pub fn execute_and_verify(ring_capacity: u32, slot_count: u32, ops: &[ShmOp]) -> Result<(), String> {
    let mut session = ShmSession::new(ring_capacity, slot_count);
    let mut sent_ids: Vec<u64> = Vec::new();
    let mut received_ids: Vec<u64> = Vec::new();
    let mut freed_ids: Vec<u64> = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        match op {
            ShmOp::Send(payload_len) => {
                match session.a_send(*payload_len as u32) {
                    Ok(msg_id) => {
                        sent_ids.push(msg_id);
                    }
                    Err(ShmError::RingFull) => {
                        // Expected when ring is full
                    }
                    Err(ShmError::Slab(SlabError::NoFreeSlots)) => {
                        // Expected when all slots are in use
                    }
                    Err(e) => {
                        return Err(format!("op {}: unexpected send error: {:?}", i, e));
                    }
                }
            }
            ShmOp::Recv => {
                if let Some(msg) = session.b_recv() {
                    received_ids.push(msg.id);
                }
            }
            ShmOp::Free(idx) => {
                if !received_ids.is_empty() {
                    let idx = (*idx as usize) % received_ids.len();
                    let msg_id = received_ids[idx];

                    match session.b_free(msg_id) {
                        Ok(()) => {
                            received_ids.remove(idx);
                            freed_ids.push(msg_id);
                        }
                        Err(e) => {
                            return Err(format!("op {}: free failed: {:?}", i, e));
                        }
                    }
                }
            }
            ShmOp::Retry => {
                let retried = session.a_retry_pending();
                sent_ids.extend(retried);
            }
        }

        // Verify invariants
        verify_shm_invariants(&session, &sent_ids, &received_ids, &freed_ids, i)?;
    }

    Ok(())
}

fn verify_shm_invariants(
    session: &ShmSession,
    sent_ids: &[u64],
    received_ids: &[u64],
    freed_ids: &[u64],
    op_idx: usize,
) -> Result<(), String> {
    // INVARIANT: Ring items + dequeued = total sent (minus pending)
    let ring_len = session.a_to_b_ring.len();
    let in_flight = session.b_received.len();
    let pending = session.a_pending_slots.len();

    // sent = ring + received_but_not_freed + freed + pending
    // But our sent_ids only tracks successfully enqueued, not pending
    // So: sent_ids.len() = ring_len + received_ids.len() + freed_ids.len()
    let accounted = ring_len + received_ids.len() + freed_ids.len();
    if sent_ids.len() != accounted {
        return Err(format!(
            "after op {}: sent_ids({}) != ring({}) + received({}) + freed({})",
            op_idx, sent_ids.len(), ring_len, received_ids.len(), freed_ids.len()
        ));
    }

    // INVARIANT: Slab state is consistent
    // Free slots + Allocated slots + InFlight slots = total
    let free = session.shared_slab.free_count();
    let allocated = session.shared_slab.allocated_count();
    let in_flight_slab = session.shared_slab.in_flight_count();
    let total = free + allocated + in_flight_slab;

    if total != session.shared_slab.slot_count() as usize {
        return Err(format!(
            "after op {}: slab state inconsistent: free({}) + allocated({}) + in_flight({}) = {} != slot_count({})",
            op_idx, free, allocated, in_flight_slab, total, session.shared_slab.slot_count()
        ));
    }

    // INVARIANT: InFlight slots = ring items + b_received items
    // (Pending are still Allocated, not InFlight)
    if in_flight_slab != ring_len + in_flight {
        return Err(format!(
            "after op {}: in_flight_slab({}) != ring_len({}) + b_received({})",
            op_idx, in_flight_slab, ring_len, in_flight
        ));
    }

    // INVARIANT: Allocated slots = pending count
    if allocated != pending {
        return Err(format!(
            "after op {}: allocated({}) != pending({})",
            op_idx, allocated, pending
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_send_recv_free() {
        let mut session = ShmSession::new(4, 8);

        // Send
        let id1 = session.a_send(100).unwrap();
        let id2 = session.a_send(200).unwrap();

        // Receive
        let msg1 = session.b_recv().unwrap();
        assert_eq!(msg1.id, id1);

        let msg2 = session.b_recv().unwrap();
        assert_eq!(msg2.id, id2);

        // No more messages
        assert!(session.b_recv().is_none());

        // Free
        session.b_free(id1).unwrap();
        session.b_free(id2).unwrap();
    }

    #[test]
    fn test_ring_full_and_retry() {
        let mut session = ShmSession::new(4, 8);

        // Fill the ring
        for i in 0..4 {
            session.a_send(100).expect(&format!("send {} should succeed", i));
        }

        // Ring is full
        assert_eq!(session.a_send(100), Err(ShmError::RingFull));

        // Receive and free some
        let msg = session.b_recv().unwrap();
        session.b_free(msg.id).unwrap();

        // Retry should work now
        let retried = session.a_retry_pending();
        assert_eq!(retried.len(), 1);
    }

    #[test]
    fn test_slab_exhaustion() {
        let mut session = ShmSession::new(16, 4);

        // Fill the slab
        for _ in 0..4 {
            session.a_send(100).unwrap();
        }

        // Slab is full
        assert_eq!(session.a_send(100), Err(ShmError::Slab(SlabError::NoFreeSlots)));

        // Receive and free
        let msg = session.b_recv().unwrap();
        session.b_free(msg.id).unwrap();

        // Now we can send again
        session.a_send(100).unwrap();
    }
}
