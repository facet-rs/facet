# rapace2 Implementation Guide

This guide describes how to implement rapace2 in Rust with maximum use of the type system
to encode invariants at compile time. The goal: **if it compiles, it's probably correct**.

## Philosophy

> "Don't be careful like in C—make invalid states unrepresentable."

The spec has many invariants:
- SPSC discipline (one producer, one consumer per ring)
- Sender allocates, receiver frees
- Inline payloads have generation=0, offset=0
- header_len >= 24, header_len <= payload_len
- Channel 0 is control-only

In C, you'd "be careful". In Rust, we encode these in types so violations don't compile.

## Module Structure

Unsafe code lives in exactly three places:

```
src/
├── layout.rs       // unsafe: repr(C) types, pointer math
├── shm.rs          // unsafe: mmap, munmap
├── doorbell.rs     // unsafe: eventfd, libc read/write
│
├── ring.rs         // safe: SPSC ring API (wraps layout)
├── alloc.rs        // safe: slab allocator + RAII handles
├── frame.rs        // safe: frame builder, validation
├── header.rs       // safe: MsgHeader encode/decode
├── channel.rs      // safe: channel state machine
├── flow.rs         // safe: credit-based flow control
├── session.rs      // safe: session lifecycle
├── registry.rs     // safe: service registry
├── dispatch.rs     // safe: method dispatch
├── codec.rs        // safe: encoding traits
├── observe.rs      // safe: metrics, telemetry
├── fault.rs        // safe: fault injection
└── lib.rs          // public API
```

Everything outside `layout`, `shm`, and `doorbell` is safe Rust building on those primitives.

---

## 1. Newtypes: No Raw Integers in Public API

Raw `u32`/`u64` everywhere is where invariants go to die.

```rust
// src/types.rs

use std::num::NonZeroU32;

/// Channel ID. 0 is reserved for control channel (not representable here).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChannelId(NonZeroU32);

impl ChannelId {
    /// Create a new data channel ID. Panics if id == 0.
    pub fn new(id: u32) -> Option<Self> {
        NonZeroU32::new(id).map(ChannelId)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Method ID for RPC dispatch.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MethodId(pub(crate) u32);

/// Message ID, monotonically increasing per session.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MsgId(pub(crate) u64);

/// Slot index in the data segment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotIndex(pub(crate) u32);

/// Generation counter for ABA safety.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Generation(pub(crate) u32);

/// Validated byte length (checked against max).
#[derive(Clone, Copy, Debug)]
pub struct ByteLen(u32);

impl ByteLen {
    pub fn new(len: u32, max: u32) -> Option<Self> {
        (len <= max).then_some(ByteLen(len))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}
```

The public API never exposes raw integers for IDs—only these newtypes.
Conversion to/from wire format happens only in `layout.rs`.

---

## 2. Layout Module: Raw, Sealed, Unsafe

Mirror the spec exactly, but keep it `pub(crate)`:

```rust
// src/layout.rs

use std::sync::atomic::{AtomicU32, AtomicU64};

pub(crate) const MAGIC: u64 = u64::from_le_bytes(*b"RAPACE2\0");
pub(crate) const INLINE_PAYLOAD_SIZE: usize = 24;

#[repr(C, align(64))]
pub(crate) struct SegmentHeader {
    pub magic: u64,
    pub version: u32,
    pub flags: u32,
    pub peer_a_epoch: AtomicU64,
    pub peer_b_epoch: AtomicU64,
    pub peer_a_last_seen: AtomicU64,
    pub peer_b_last_seen: AtomicU64,
}

#[repr(C, align(64))]
pub(crate) struct MsgDescHot {
    pub msg_id: u64,
    pub channel_id: u32,
    pub method_id: u32,
    pub payload_slot: u32,
    pub payload_generation: u32,
    pub payload_offset: u32,
    pub payload_len: u32,
    pub flags: u32,
    pub credit_grant: u32,
    pub inline_payload: [u8; INLINE_PAYLOAD_SIZE],
}

#[repr(C, align(64))]
pub(crate) struct DescRing {
    pub visible_head: AtomicU64,
    _pad1: [u8; 56],
    pub tail: AtomicU64,
    _pad2: [u8; 56],
    pub capacity: u32,
    _pad3: [u8; 60],
    // Descriptors follow in memory
}

#[repr(C)]
pub(crate) struct SlotMeta {
    pub generation: AtomicU32,
    pub state: AtomicU32,
}

// Compile-time size checks
const _: () = assert!(std::mem::size_of::<SegmentHeader>() == 64);
const _: () = assert!(std::mem::size_of::<MsgDescHot>() == 64);
```

**Key point:** These types are `pub(crate)`. External code never touches them directly.

---

## 3. SPSC Ring: Type-Level Producer/Consumer Split

The ring should be impossible to misuse from both sides:

```rust
// src/ring.rs

use std::ptr::NonNull;
use std::sync::atomic::Ordering;
use crate::layout::{DescRing, MsgDescHot};

/// Shared ring state. Not directly usable—must split into Producer/Consumer.
pub struct Ring {
    ptr: NonNull<DescRing>,
    capacity: u64,
}

/// Producer half of the ring. Only one exists per ring.
pub struct Producer<'ring> {
    ring: &'ring Ring,
    local_head: u64,  // Private to producer, not in SHM
}

/// Consumer half of the ring. Only one exists per ring.
pub struct Consumer<'ring> {
    ring: &'ring Ring,
}

#[derive(Debug)]
pub struct RingFull;

impl Ring {
    /// # Safety
    /// `ptr` must point to a valid, mapped DescRing that outlives the Ring.
    pub(crate) unsafe fn from_ptr(ptr: NonNull<DescRing>) -> Self {
        let capacity = unsafe { (*ptr.as_ptr()).capacity as u64 };
        Ring { ptr, capacity }
    }

    /// Split into producer and consumer halves.
    ///
    /// This enforces SPSC at the type level: you get exactly one of each,
    /// and they borrow the ring so it can't be split again.
    pub fn split(&mut self) -> (Producer<'_>, Consumer<'_>) {
        (
            Producer { ring: self, local_head: 0 },
            Consumer { ring: self },
        )
    }

    fn ring(&self) -> &DescRing {
        unsafe { self.ptr.as_ref() }
    }
}

impl<'ring> Producer<'ring> {
    /// Try to enqueue a descriptor. Returns Err if ring is full.
    pub fn try_enqueue(&mut self, desc: MsgDescHot) -> Result<(), RingFull> {
        let ring = self.ring.ring();
        let tail = ring.tail.load(Ordering::Acquire);

        if self.local_head.wrapping_sub(tail) >= self.ring.capacity {
            return Err(RingFull);
        }

        let idx = (self.local_head & (self.ring.capacity - 1)) as usize;

        // Write descriptor
        unsafe {
            let slot = self.desc_slot(idx);
            std::ptr::write(slot, desc);
        }

        self.local_head += 1;

        // Publish
        ring.visible_head.store(self.local_head, Ordering::Release);

        Ok(())
    }

    unsafe fn desc_slot(&self, idx: usize) -> *mut MsgDescHot {
        let base = self.ring.ptr.as_ptr().add(1) as *mut MsgDescHot;
        base.add(idx)
    }
}

impl<'ring> Consumer<'ring> {
    /// Try to dequeue a descriptor. Returns None if ring is empty.
    pub fn try_dequeue(&mut self) -> Option<MsgDescHot> {
        let ring = self.ring.ring();
        let tail = ring.tail.load(Ordering::Relaxed);
        let visible = ring.visible_head.load(Ordering::Acquire);

        if tail >= visible {
            return None;
        }

        let idx = (tail & (self.ring.capacity - 1)) as usize;

        let desc = unsafe {
            let slot = self.desc_slot(idx);
            std::ptr::read(slot)
        };

        ring.tail.store(tail + 1, Ordering::Release);

        Some(desc)
    }

    /// Drain up to `max` descriptors.
    pub fn drain(&mut self, max: usize) -> impl Iterator<Item = MsgDescHot> + '_ {
        std::iter::from_fn(move || self.try_dequeue()).take(max)
    }

    unsafe fn desc_slot(&self, idx: usize) -> *const MsgDescHot {
        let base = self.ring.ptr.as_ptr().add(1) as *const MsgDescHot;
        base.add(idx)
    }
}
```

**Invariants encoded:**
- Exactly one `Producer`, exactly one `Consumer` per ring (by construction)
- `local_head` is only inside `Producer`, can't be accessed elsewhere
- `Consumer` can't call `enqueue`, `Producer` can't call `dequeue`

---

## 4. Data Segment: RAII Payload Handles

The "sender allocates, receiver frees" rule maps perfectly to RAII:

```rust
// src/alloc.rs

use crate::types::{SlotIndex, Generation, ByteLen};
use crate::layout::SlotMeta;
use std::sync::atomic::Ordering;

/// Slot states
#[repr(u32)]
pub enum SlotState {
    Free = 0,
    Allocated = 1,
    InFlight = 2,
}

/// Data segment for payload allocation.
pub struct DataSegment {
    // ... internal state
}

/// A slot allocated for outbound data. Owns the slot until committed or dropped.
///
/// If dropped without committing, the slot is freed automatically.
pub struct OutboundSlot<'seg> {
    seg: &'seg DataSegment,
    slot: SlotIndex,
    gen: Generation,
    buf: &'seg mut [u8],
    committed: bool,
}

/// A committed slot ready to be referenced in a descriptor.
///
/// Does NOT free on drop—ownership transfers to receiver via the ring.
pub struct CommittedSlot {
    slot: SlotIndex,
    gen: Generation,
    offset: u32,
    len: u32,
}

/// An inbound payload received from peer. Frees the slot on drop.
pub struct InboundPayload<'seg> {
    seg: &'seg DataSegment,
    buf: &'seg [u8],
    slot: Option<(SlotIndex, Generation)>,  // None for inline
}

impl DataSegment {
    /// Allocate a slot for outbound payload.
    pub fn alloc(&self) -> Result<OutboundSlot<'_>, AllocError> {
        // Pop from free list, bump generation, set state = Allocated
        // Return OutboundSlot that will free on Drop if not committed
        todo!()
    }

    /// Free a slot (called by receiver after processing).
    pub(crate) fn free(&self, slot: SlotIndex, gen: Generation) {
        // Verify generation, set state = Free, push to free list
        todo!()
    }
}

impl<'seg> OutboundSlot<'seg> {
    /// Get mutable access to the payload buffer.
    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.buf
    }

    /// Commit the slot with the actual payload length.
    ///
    /// After this, the slot will NOT be freed on drop—ownership
    /// transfers to the receiver when the descriptor is enqueued.
    pub fn commit(mut self, len: ByteLen) -> CommittedSlot {
        self.committed = true;
        // Set state = InFlight
        CommittedSlot {
            slot: self.slot,
            gen: self.gen,
            offset: 0,
            len: len.get(),
        }
    }
}

impl Drop for OutboundSlot<'_> {
    fn drop(&mut self) {
        if !self.committed {
            // Slot was allocated but not used—free it
            self.seg.free(self.slot, self.gen);
        }
    }
}

impl CommittedSlot {
    /// Get descriptor fields for this slot.
    pub fn to_descriptor_fields(&self) -> (u32, u32, u32, u32) {
        (self.slot.0, self.gen.0, self.offset, self.len)
    }
}

impl Drop for InboundPayload<'_> {
    fn drop(&mut self) {
        if let Some((slot, gen)) = self.slot {
            self.seg.free(slot, gen);
        }
    }
}

#[derive(Debug)]
pub struct AllocError;
```

**Lifecycle:**
- Sender: `alloc()` → `OutboundSlot` → `commit()` → `CommittedSlot` → enqueue → handle dropped (no free)
- Receiver: dequeue → `InboundPayload` → process → drop (frees slot)

**Invariants encoded:**
- Can't forget to free: `InboundPayload` frees on drop
- Can't double-free: slot reference consumed on free
- Can't leak allocated slots: `OutboundSlot` frees if not committed

---

## 5. Frame Builder: Safe Descriptor Construction

Never let users construct `MsgDescHot` directly:

```rust
// src/frame.rs

use crate::layout::{MsgDescHot, INLINE_PAYLOAD_SIZE};
use crate::types::{ChannelId, MethodId, MsgId};
use crate::alloc::CommittedSlot;

/// Builder for outbound frames. Enforces invariants at construction time.
pub struct FrameBuilder {
    desc: MsgDescHot,
    _slot: Option<CommittedSlot>,  // Keep alive if using slot payload
}

#[derive(Debug)]
pub struct PayloadTooLarge;

impl FrameBuilder {
    /// Create a new data frame builder.
    pub fn data(channel: ChannelId, method: MethodId, msg_id: MsgId) -> Self {
        FrameBuilder {
            desc: MsgDescHot {
                msg_id: msg_id.0,
                channel_id: channel.get(),
                method_id: method.0,
                payload_slot: u32::MAX,
                payload_generation: 0,
                payload_offset: 0,
                payload_len: 0,
                flags: FrameFlags::DATA.bits(),
                credit_grant: 0,
                inline_payload: [0; INLINE_PAYLOAD_SIZE],
            },
            _slot: None,
        }
    }

    /// Create a control frame builder.
    pub fn control(method: ControlMethod, msg_id: MsgId) -> Self {
        FrameBuilder {
            desc: MsgDescHot {
                msg_id: msg_id.0,
                channel_id: 0,  // Control channel
                method_id: method as u32,
                payload_slot: u32::MAX,
                payload_generation: 0,
                payload_offset: 0,
                payload_len: 0,
                flags: FrameFlags::CONTROL.bits(),
                credit_grant: 0,
                inline_payload: [0; INLINE_PAYLOAD_SIZE],
            },
            _slot: None,
        }
    }

    /// Set inline payload. Enforces: slot=MAX, generation=0, offset=0.
    pub fn inline_payload(mut self, payload: &[u8]) -> Result<Self, PayloadTooLarge> {
        if payload.len() > INLINE_PAYLOAD_SIZE {
            return Err(PayloadTooLarge);
        }

        // Invariants enforced by construction:
        self.desc.payload_slot = u32::MAX;
        self.desc.payload_generation = 0;  // MUST be 0 for inline
        self.desc.payload_offset = 0;      // MUST be 0 for inline
        self.desc.payload_len = payload.len() as u32;
        self.desc.inline_payload[..payload.len()].copy_from_slice(payload);

        Ok(self)
    }

    /// Set slot payload from a committed slot.
    pub fn slot_payload(mut self, slot: CommittedSlot) -> Self {
        let (idx, gen, offset, len) = slot.to_descriptor_fields();
        self.desc.payload_slot = idx;
        self.desc.payload_generation = gen;
        self.desc.payload_offset = offset;
        self.desc.payload_len = len;
        self._slot = Some(slot);
        self
    }

    /// Set frame flags.
    pub fn flags(mut self, flags: FrameFlags) -> Self {
        self.desc.flags = flags.bits();
        self
    }

    /// Set credit grant.
    pub fn credits(mut self, credits: u32) -> Self {
        self.desc.credit_grant = credits;
        self.desc.flags |= FrameFlags::CREDITS.bits();
        self
    }

    /// Build the final descriptor.
    pub(crate) fn build(self) -> MsgDescHot {
        self.desc
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct FrameFlags: u32 {
        const DATA          = 0b00000001;
        const CONTROL       = 0b00000010;
        const EOS           = 0b00000100;
        const CANCEL        = 0b00001000;
        const ERROR         = 0b00010000;
        const HIGH_PRIORITY = 0b00100000;
        const CREDITS       = 0b01000000;
        const METADATA_ONLY = 0b10000000;
    }
}

#[repr(u32)]
pub enum ControlMethod {
    OpenChannel = 1,
    CloseChannel = 2,
    CancelChannel = 3,
    GrantCredits = 4,
    Ping = 5,
    Pong = 6,
    // ... etc
}
```

**Invariants encoded:**
- Inline mode automatically sets generation=0, offset=0
- Can't mix inline and slot payload (last one wins, but API guides you)
- Control frames always have channel_id=0

---

## 6. Descriptor Validation: Raw vs Validated Types

Separate "untrusted wire data" from "validated, safe to use":

```rust
// src/frame.rs (continued)

use crate::alloc::DataSegment;

/// Raw descriptor from the wire. Untrusted.
pub struct RawDescriptor(pub(crate) MsgDescHot);

/// Validated descriptor. Safe to access payload.
pub struct ValidDescriptor<'seg> {
    pub(crate) raw: MsgDescHot,
    pub(crate) seg: &'seg DataSegment,
}

/// Descriptor validation limits.
pub struct DescriptorLimits {
    pub max_payload_len: u32,
    pub max_channels: u32,
}

impl Default for DescriptorLimits {
    fn default() -> Self {
        DescriptorLimits {
            max_payload_len: 1024 * 1024,  // 1MB
            max_channels: 1024,
        }
    }
}

#[derive(Debug)]
pub enum ValidationError {
    SlotOutOfBounds,
    PayloadOutOfBounds,
    StaleGeneration,
    InlinePayloadTooLarge,
    PayloadTooLarge,
}

impl RawDescriptor {
    /// Validate descriptor against segment and limits.
    ///
    /// On success, returns a ValidDescriptor that's safe to use.
    pub fn validate<'seg>(
        self,
        seg: &'seg DataSegment,
        limits: &DescriptorLimits,
    ) -> Result<ValidDescriptor<'seg>, ValidationError> {
        let desc = &self.0;

        // Check payload length limit
        if desc.payload_len > limits.max_payload_len {
            return Err(ValidationError::PayloadTooLarge);
        }

        if desc.payload_slot == u32::MAX {
            // Inline payload
            if desc.payload_len > INLINE_PAYLOAD_SIZE as u32 {
                return Err(ValidationError::InlinePayloadTooLarge);
            }
            // Note: we don't strictly enforce generation=0, offset=0 on receive
            // (be liberal in what you accept), but we DO enforce on send
        } else {
            // Slot payload - validate bounds
            if desc.payload_slot >= seg.slot_count() {
                return Err(ValidationError::SlotOutOfBounds);
            }

            let end = desc.payload_offset.saturating_add(desc.payload_len);
            if end > seg.slot_size() {
                return Err(ValidationError::PayloadOutOfBounds);
            }

            // Generation check
            if seg.generation(SlotIndex(desc.payload_slot)) != Generation(desc.payload_generation) {
                return Err(ValidationError::StaleGeneration);
            }
        }

        Ok(ValidDescriptor { raw: self.0, seg })
    }
}

impl<'seg> ValidDescriptor<'seg> {
    /// Get the payload as InboundPayload (takes ownership for freeing).
    pub fn into_payload(self) -> InboundPayload<'seg> {
        if self.raw.payload_slot == u32::MAX {
            // Inline
            InboundPayload {
                seg: self.seg,
                buf: &self.raw.inline_payload[..self.raw.payload_len as usize],
                slot: None,
            }
        } else {
            // Slot
            let buf = self.seg.slot_data(
                SlotIndex(self.raw.payload_slot),
                self.raw.payload_offset,
                self.raw.payload_len,
            );
            InboundPayload {
                seg: self.seg,
                buf,
                slot: Some((
                    SlotIndex(self.raw.payload_slot),
                    Generation(self.raw.payload_generation),
                )),
            }
        }
    }

    pub fn channel_id(&self) -> Option<ChannelId> {
        ChannelId::new(self.raw.channel_id)
    }

    pub fn is_control(&self) -> bool {
        self.raw.channel_id == 0
    }

    pub fn method_id(&self) -> MethodId {
        MethodId(self.raw.method_id)
    }

    pub fn flags(&self) -> FrameFlags {
        FrameFlags::from_bits_truncate(self.raw.flags)
    }
}
```

**Invariants encoded:**
- Can't access payload without validation
- `ValidDescriptor` proves validation happened
- Payload ownership tracked via `InboundPayload`

---

## 7. Message Header: Strict Encode/Decode

Never let users manually calculate offsets:

```rust
// src/header.rs

use crate::codec::Encoding;

pub const MSG_HEADER_FIXED_SIZE: usize = 24;

/// Message header (safe representation).
#[derive(Debug, Clone)]
pub struct MsgHeader {
    pub version: u16,
    pub encoding: Encoding,
    pub flags: u16,
    pub correlation_id: u64,
    pub deadline_ns: u64,
    pub metadata: Metadata,
}

/// Metadata key-value pairs with enforced limits.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pairs: Vec<(String, Vec<u8>)>,
}

pub const MAX_METADATA_PAIRS: usize = 32;
pub const MAX_METADATA_KEY_LEN: usize = 64;
pub const MAX_METADATA_VALUE_LEN: usize = 4096;

#[derive(Debug)]
pub enum HeaderError {
    TooShort,
    InvalidHeaderLen,
    MetadataTooLarge,
    InvalidEncoding,
}

impl MsgHeader {
    /// Encode header into buffer. Returns total header_len.
    pub fn encode_into(&self, buf: &mut [u8]) -> Result<usize, HeaderError> {
        if buf.len() < MSG_HEADER_FIXED_SIZE {
            return Err(HeaderError::TooShort);
        }

        // Encode metadata to temp buffer to get length
        let metadata_bytes = self.metadata.encode();
        let header_len = MSG_HEADER_FIXED_SIZE + metadata_bytes.len();

        if buf.len() < header_len {
            return Err(HeaderError::TooShort);
        }

        // Write fixed header
        buf[0..2].copy_from_slice(&self.version.to_le_bytes());
        buf[2..4].copy_from_slice(&(header_len as u16).to_le_bytes());
        buf[4..6].copy_from_slice(&(self.encoding as u16).to_le_bytes());
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..16].copy_from_slice(&self.correlation_id.to_le_bytes());
        buf[16..24].copy_from_slice(&self.deadline_ns.to_le_bytes());

        // Write metadata
        buf[24..header_len].copy_from_slice(&metadata_bytes);

        Ok(header_len)
    }

    /// Decode header from buffer. Returns (header, header_len).
    pub fn decode_from(buf: &[u8]) -> Result<(Self, usize), HeaderError> {
        if buf.len() < MSG_HEADER_FIXED_SIZE {
            return Err(HeaderError::TooShort);
        }

        let version = u16::from_le_bytes([buf[0], buf[1]]);
        let header_len = u16::from_le_bytes([buf[2], buf[3]]) as usize;
        let encoding_raw = u16::from_le_bytes([buf[4], buf[5]]);
        let flags = u16::from_le_bytes([buf[6], buf[7]]);
        let correlation_id = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        let deadline_ns = u64::from_le_bytes(buf[16..24].try_into().unwrap());

        // Validate header_len
        if header_len < MSG_HEADER_FIXED_SIZE || header_len > buf.len() {
            return Err(HeaderError::InvalidHeaderLen);
        }

        let encoding = Encoding::try_from(encoding_raw)
            .map_err(|_| HeaderError::InvalidEncoding)?;

        let metadata = if header_len > MSG_HEADER_FIXED_SIZE {
            Metadata::decode(&buf[MSG_HEADER_FIXED_SIZE..header_len])?
        } else {
            Metadata::default()
        };

        Ok((
            MsgHeader {
                version,
                encoding,
                flags,
                correlation_id,
                deadline_ns,
                metadata,
            },
            header_len,
        ))
    }

    /// Get body slice from payload buffer (after header).
    pub fn body_from_payload<'a>(&self, payload: &'a [u8], header_len: usize) -> &'a [u8] {
        &payload[header_len..]
    }
}

impl Metadata {
    pub fn insert(&mut self, key: String, value: Vec<u8>) -> Result<(), HeaderError> {
        if self.pairs.len() >= MAX_METADATA_PAIRS {
            return Err(HeaderError::MetadataTooLarge);
        }
        if key.len() > MAX_METADATA_KEY_LEN || value.len() > MAX_METADATA_VALUE_LEN {
            return Err(HeaderError::MetadataTooLarge);
        }
        self.pairs.push((key, value));
        Ok(())
    }

    fn encode(&self) -> Vec<u8> {
        // Length-prefixed encoding
        todo!()
    }

    fn decode(buf: &[u8]) -> Result<Self, HeaderError> {
        todo!()
    }
}
```

**Invariants encoded:**
- `header_len >= 24` enforced in decode
- Metadata limits enforced in `insert()`
- Body offset calculated correctly (no manual math)

---

## 8. Channel State Machine via Typestates

Encode channel lifecycle in types:

```rust
// src/channel.rs

use std::marker::PhantomData;
use crate::types::ChannelId;

// Typestate markers
pub struct Open;
pub struct HalfClosedLocal;
pub struct HalfClosedRemote;
pub struct Closed;

/// Channel handle with compile-time state tracking.
pub struct Channel<State> {
    id: ChannelId,
    // ... internal state
    _state: PhantomData<State>,
}

// Only Open channels can send
impl Channel<Open> {
    /// Send data on this channel.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), SendError> {
        todo!()
    }

    /// Send and half-close (send EOS).
    pub fn send_and_close(self, data: &[u8]) -> Channel<HalfClosedLocal> {
        // Send data + EOS flag
        todo!()
    }

    /// Half-close without sending (just EOS).
    pub fn close_send(self) -> Channel<HalfClosedLocal> {
        todo!()
    }

    /// Receive data.
    pub async fn recv(&mut self) -> Option<Frame> {
        todo!()
    }
}

// HalfClosedLocal: can only receive
impl Channel<HalfClosedLocal> {
    /// Receive data (peer may still be sending).
    pub async fn recv(&mut self) -> Option<Frame> {
        todo!()
    }

    /// Called when peer sends EOS.
    pub(crate) fn peer_closed(self) -> Channel<Closed> {
        todo!()
    }
}

// HalfClosedRemote: can only send
impl Channel<HalfClosedRemote> {
    /// Send data.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), SendError> {
        todo!()
    }

    /// Close our side.
    pub fn close_send(self) -> Channel<Closed> {
        todo!()
    }
}

// Closed: no operations
impl Channel<Closed> {
    /// Get final statistics.
    pub fn stats(&self) -> ChannelStats {
        todo!()
    }
}

// Cancel works from any state
impl<S> Channel<S> {
    /// Cancel the channel (advisory).
    pub fn cancel(self) -> Channel<Closed> {
        todo!()
    }

    pub fn id(&self) -> ChannelId {
        self.id
    }
}

#[derive(Debug)]
pub struct SendError;
pub struct Frame;
pub struct ChannelStats;
```

**Invariants encoded:**
- Can't `send()` on `HalfClosedLocal` or `Closed`
- Can't `recv()` on `HalfClosedRemote` or `Closed`
- State transitions consume the channel, returning new state

---

## 9. Flow Control: Credits as RAII Permits

Hide raw atomic manipulation behind permits:

```rust
// src/flow.rs

use std::sync::atomic::{AtomicU32, Ordering};
use crate::types::ByteLen;

/// Credit pool for flow control.
pub struct Credits {
    available: AtomicU32,
}

/// A permit representing reserved credits. Released on drop if not used.
pub struct CreditPermit<'a> {
    credits: &'a Credits,
    amount: u32,
    consumed: bool,
}

impl Credits {
    pub fn new(initial: u32) -> Self {
        Credits {
            available: AtomicU32::new(initial),
        }
    }

    /// Try to reserve credits. Returns None if insufficient.
    pub fn try_reserve(&self, needed: ByteLen) -> Option<CreditPermit<'_>> {
        let needed = needed.get();
        loop {
            let current = self.available.load(Ordering::Acquire);
            if current < needed {
                return None;
            }
            if self.available.compare_exchange_weak(
                current,
                current - needed,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ).is_ok() {
                return Some(CreditPermit {
                    credits: self,
                    amount: needed,
                    consumed: false,
                });
            }
        }
    }

    /// Add credits (called when receiving CREDITS frame).
    pub fn grant(&self, amount: u32) {
        self.available.fetch_add(amount, Ordering::Release);
    }
}

impl<'a> CreditPermit<'a> {
    /// Mark credits as consumed (data was sent successfully).
    pub fn consume(mut self) {
        self.consumed = true;
    }
}

impl Drop for CreditPermit<'_> {
    fn drop(&mut self) {
        if !self.consumed {
            // Return credits if not used (e.g., send failed)
            self.credits.grant(self.amount);
        }
    }
}
```

**Invariants encoded:**
- Can't send without credits (must have permit)
- Credits automatically returned if send fails
- No raw atomic access in user code

---

## 10. Session: Role as Type Parameter

Encode peer role at compile time:

```rust
// src/session.rs

use std::marker::PhantomData;
use crate::ring::{Ring, Producer, Consumer};
use crate::alloc::DataSegment;
use crate::layout::SegmentHeader;

/// Marker trait for session role.
pub trait SessionRole: sealed::Sealed {
    const IS_PEER_A: bool;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::PeerA {}
    impl Sealed for super::PeerB {}
}

pub struct PeerA;
pub struct PeerB;

impl SessionRole for PeerA {
    const IS_PEER_A: bool = true;
}

impl SessionRole for PeerB {
    const IS_PEER_A: bool = false;
}

/// A rapace session with compile-time role.
pub struct Session<R: SessionRole> {
    header: *const SegmentHeader,
    outbound_producer: Producer<'static>,
    inbound_consumer: Consumer<'static>,
    outbound_segment: DataSegment,
    inbound_segment: DataSegment,
    _role: PhantomData<R>,
}

impl<R: SessionRole> Session<R> {
    /// Check if our peer is alive.
    pub fn is_peer_alive(&self) -> bool {
        let header = unsafe { &*self.header };
        let timestamp = if R::IS_PEER_A {
            header.peer_b_last_seen.load(Ordering::Acquire)
        } else {
            header.peer_a_last_seen.load(Ordering::Acquire)
        };
        let age = now_nanos() - timestamp;
        age < 1_000_000_000 // 1 second
    }

    /// Update our heartbeat.
    pub fn heartbeat(&self) {
        let header = unsafe { &*self.header };
        let (epoch, timestamp) = if R::IS_PEER_A {
            (&header.peer_a_epoch, &header.peer_a_last_seen)
        } else {
            (&header.peer_b_epoch, &header.peer_b_last_seen)
        };
        epoch.fetch_add(1, Ordering::Release);
        timestamp.store(now_nanos(), Ordering::Release);
    }
}

fn now_nanos() -> u64 {
    // Use CLOCK_MONOTONIC
    todo!()
}
```

**Invariants encoded:**
- Role is fixed at compile time
- No runtime "am I peer A?" checks needed
- Correct heartbeat/liveness field selected by type

---

## 11. Codec Trait

Abstract over encoding while keeping wire format fixed:

```rust
// src/codec.rs

use serde::{Serialize, de::DeserializeOwned};

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Postcard = 1,
    Json = 2,
    Raw = 3,
}

impl TryFrom<u16> for Encoding {
    type Error = ();
    fn try_from(v: u16) -> Result<Self, ()> {
        match v {
            1 => Ok(Encoding::Postcard),
            2 => Ok(Encoding::Json),
            3 => Ok(Encoding::Raw),
            _ => Err(()),
        }
    }
}

/// Codec trait for message serialization.
pub trait Codec {
    const ENCODING: Encoding;
    type EncodeError: std::error::Error;
    type DecodeError: std::error::Error;

    fn encode<T: Serialize>(val: &T) -> Result<Vec<u8>, Self::EncodeError>;
    fn decode<T: DeserializeOwned>(buf: &[u8]) -> Result<T, Self::DecodeError>;
}

/// Postcard codec (default for control messages).
pub struct PostcardCodec;

impl Codec for PostcardCodec {
    const ENCODING: Encoding = Encoding::Postcard;
    type EncodeError = postcard::Error;
    type DecodeError = postcard::Error;

    fn encode<T: Serialize>(val: &T) -> Result<Vec<u8>, Self::EncodeError> {
        postcard::to_allocvec(val)
    }

    fn decode<T: DeserializeOwned>(buf: &[u8]) -> Result<T, Self::DecodeError> {
        postcard::from_bytes(buf)
    }
}

/// JSON codec (for debugging/tooling).
pub struct JsonCodec;

impl Codec for JsonCodec {
    const ENCODING: Encoding = Encoding::Json;
    type EncodeError = serde_json::Error;
    type DecodeError = serde_json::Error;

    fn encode<T: Serialize>(val: &T) -> Result<Vec<u8>, Self::EncodeError> {
        serde_json::to_vec(val)
    }

    fn decode<T: DeserializeOwned>(buf: &[u8]) -> Result<T, Self::DecodeError> {
        serde_json::from_slice(buf)
    }
}
```

---

## 12. Fault Injection: Centralized Policy

Single point of control for fault injection:

```rust
// src/fault.rs

use crate::types::ChannelId;
use crate::error::ErrorCode;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct FaultInjector {
    global_drop_rate: AtomicU32,    // 0-10000 (0.00%-100.00%)
    global_error_rate: AtomicU32,
    global_delay_ms: AtomicU32,
    // Per-channel overrides would use a concurrent map
}

pub enum FaultAction {
    /// Process normally.
    Pass,
    /// Drop the frame silently.
    Drop,
    /// Inject an error.
    Error(ErrorCode),
    /// Delay processing.
    Delay(std::time::Duration),
}

impl FaultInjector {
    pub fn new() -> Self {
        FaultInjector {
            global_drop_rate: AtomicU32::new(0),
            global_error_rate: AtomicU32::new(0),
            global_delay_ms: AtomicU32::new(0),
        }
    }

    /// Check what action to take for this frame.
    ///
    /// MUST be called AFTER validation (don't mask validation bugs).
    pub fn check(&self, _channel: Option<ChannelId>) -> FaultAction {
        let drop_rate = self.global_drop_rate.load(Ordering::Relaxed);
        if drop_rate > 0 && rand_percent() < drop_rate {
            return FaultAction::Drop;
        }

        let error_rate = self.global_error_rate.load(Ordering::Relaxed);
        if error_rate > 0 && rand_percent() < error_rate {
            return FaultAction::Error(ErrorCode::Internal);
        }

        let delay_ms = self.global_delay_ms.load(Ordering::Relaxed);
        if delay_ms > 0 {
            return FaultAction::Delay(std::time::Duration::from_millis(delay_ms as u64));
        }

        FaultAction::Pass
    }

    pub fn set_drop_rate(&self, rate: u32) {
        self.global_drop_rate.store(rate.min(10000), Ordering::Relaxed);
    }
}

fn rand_percent() -> u32 {
    // Returns 0-9999
    rand::random::<u32>() % 10000
}
```

---

## 13. Error Codes

Match the spec exactly:

```rust
// src/error.rs

/// Error codes aligned with gRPC (0-99) plus rapace-specific (100+).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    // gRPC-aligned
    Ok = 0,
    Cancelled = 1,
    DeadlineExceeded = 2,
    InvalidArgument = 3,
    NotFound = 4,
    AlreadyExists = 5,
    PermissionDenied = 6,
    ResourceExhausted = 7,
    FailedPrecondition = 8,
    Aborted = 9,
    OutOfRange = 10,
    Unimplemented = 11,
    Internal = 12,
    Unavailable = 13,
    DataLoss = 14,

    // rapace-specific
    PeerDied = 100,
    SessionClosed = 101,
    ValidationFailed = 102,
    StaleGeneration = 103,
}
```

---

## 14. Testing Strategy

### Compile-Time Invariants

```rust
// In layout.rs
const _: () = {
    assert!(std::mem::size_of::<MsgDescHot>() == 64);
    assert!(std::mem::size_of::<SegmentHeader>() == 64);
    assert!(std::mem::align_of::<MsgDescHot>() == 64);
};
```

### Property Tests

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn ring_never_loses_messages(ops: Vec<RingOp>) {
            // Model-based testing: compare against VecDeque
        }

        #[test]
        fn allocator_never_double_frees(ops: Vec<AllocOp>) {
            // Track allocations, verify no double-free
        }
    }
}
```

### Fuzzing

```rust
// fuzz/fuzz_targets/descriptor_validation.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 64 {
        let desc: MsgDescHot = unsafe { std::ptr::read(data.as_ptr() as *const _) };
        let raw = RawDescriptor(desc);
        // Should never panic, only return Err
        let _ = raw.validate(&test_segment(), &DescriptorLimits::default());
    }
});
```

---

## Summary

By following this guide, you get:

| Invariant | Enforcement |
|-----------|-------------|
| SPSC discipline | `Producer`/`Consumer` types |
| Sender allocs, receiver frees | RAII handles (`OutboundSlot`, `InboundPayload`) |
| Inline payload fields = 0 | `FrameBuilder` API |
| header_len >= 24 | `MsgHeader::decode_from()` |
| Valid descriptors only | `RawDescriptor` → `ValidDescriptor` |
| Channel lifecycle | Typestate pattern |
| Flow control | Credit permits |
| Role correctness | `Session<PeerA>` vs `Session<PeerB>` |

**If it compiles, it's probably correct.**
