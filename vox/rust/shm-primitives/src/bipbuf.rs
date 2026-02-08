use crate::region::Region;
use crate::sync::{AtomicU32, Ordering};

/// BipBuffer header (128 bytes, two cache lines).
///
/// Cache line 0 is producer-owned: `write` and `watermark`.
/// Cache line 1 is consumer-owned: `read`.
///
/// The data region of `capacity` bytes immediately follows this header,
/// 64-byte aligned.
///
/// shm[impl shm.bipbuf.header]
#[repr(C, align(64))]
pub struct BipBufHeader {
    // --- Cache line 0: producer-owned ---
    /// Byte offset of the next write position.
    pub write: AtomicU32,
    /// Wrap boundary. 0 means no wrap is active.
    pub watermark: AtomicU32,
    /// Data region size in bytes (immutable after init).
    pub capacity: u32,
    _pad0: [u8; 52],

    // --- Cache line 1: consumer-owned ---
    /// Byte offset of the consumed frontier.
    pub read: AtomicU32,
    _pad1: [u8; 60],
}

#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<BipBufHeader>() == 128);

/// Size of the BipBuffer header in bytes.
pub const BIPBUF_HEADER_SIZE: usize = 128;

impl BipBufHeader {
    /// Initialize a new BipBuffer header.
    ///
    /// shm[impl shm.bipbuf.initialization]
    pub fn init(&mut self, capacity: u32) {
        assert!(capacity > 0, "capacity must be > 0");
        self.write = AtomicU32::new(0);
        self.watermark = AtomicU32::new(0);
        self.capacity = capacity;
        self._pad0 = [0; 52];
        self.read = AtomicU32::new(0);
        self._pad1 = [0; 60];
    }

    /// Reset the buffer to empty state (e.g., after crash recovery).
    pub fn reset(&mut self) {
        self.write = AtomicU32::new(0);
        self.watermark = AtomicU32::new(0);
        self.read = AtomicU32::new(0);
    }
}

/// A variable-length byte SPSC ring buffer (BipBuffer) in a shared memory region.
///
/// This is a convenience wrapper around `BipBufRaw` that manages memory
/// through a `Region`.
pub struct BipBuf {
    #[allow(dead_code)]
    region: Region,
    inner: BipBufRaw,
}

unsafe impl Send for BipBuf {}
unsafe impl Sync for BipBuf {}

impl BipBuf {
    /// Initialize a new BipBuffer in the region.
    ///
    /// # Safety
    ///
    /// The region must be writable and exclusively owned during initialization.
    pub unsafe fn init(region: Region, header_offset: usize, capacity: u32) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );

        let data_offset = header_offset + BIPBUF_HEADER_SIZE;
        let required = data_offset + capacity as usize;
        assert!(required <= region.len(), "region too small for bipbuf");

        let header_ptr = region.offset(header_offset) as *mut BipBufHeader;
        let data_ptr = region.offset(data_offset);

        unsafe { (*header_ptr).init(capacity) };
        let inner = unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) };

        Self { region, inner }
    }

    /// Attach to an existing BipBuffer in the region.
    ///
    /// # Safety
    ///
    /// The region must contain a valid, initialized BipBuffer header.
    pub unsafe fn attach(region: Region, header_offset: usize) -> Self {
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );

        let data_offset = header_offset + BIPBUF_HEADER_SIZE;
        let header_ptr = region.offset(header_offset) as *mut BipBufHeader;
        let capacity = unsafe { (*header_ptr).capacity };

        assert!(capacity > 0, "invalid bipbuf capacity");
        let required = data_offset + capacity as usize;
        assert!(required <= region.len(), "region too small for bipbuf");

        let data_ptr = region.offset(data_offset);
        let inner = unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) };

        Self { region, inner }
    }

    /// Get a reference to the inner raw buffer.
    #[inline]
    pub fn inner(&self) -> &BipBufRaw {
        &self.inner
    }

    /// Split into producer and consumer handles.
    pub fn split(&self) -> (BipBufProducer<'_>, BipBufConsumer<'_>) {
        (BipBufProducer { buf: self }, BipBufConsumer { buf: self })
    }

    /// Returns the buffer capacity in bytes.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.inner.capacity()
    }
}

/// Producer handle for the BipBuffer.
pub struct BipBufProducer<'a> {
    buf: &'a BipBuf,
}

/// Consumer handle for the BipBuffer.
pub struct BipBufConsumer<'a> {
    buf: &'a BipBuf,
}

/// Error returned when the buffer doesn't have enough contiguous space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BipBufFull;

impl<'a> BipBufProducer<'a> {
    /// Try to reserve a contiguous region of `len` bytes for writing.
    ///
    /// On success, returns a mutable slice. The caller MUST write their
    /// data into this slice and then call `commit(len)` to make it visible.
    ///
    /// Returns `None` if there isn't enough contiguous space.
    pub fn try_grant(&mut self, len: u32) -> Option<&mut [u8]> {
        self.buf.inner.try_grant(len)
    }

    /// Commit `len` bytes that were previously granted.
    ///
    /// This makes the written bytes visible to the consumer.
    ///
    /// # Panics
    ///
    /// Panics if `len` exceeds the capacity (programming error).
    pub fn commit(&mut self, len: u32) {
        self.buf.inner.commit(len);
    }
}

impl<'a> BipBufConsumer<'a> {
    /// Try to read contiguous bytes from the buffer.
    ///
    /// Returns a slice of available bytes, or `None` if the buffer is empty.
    /// After processing, call `release(n)` to advance the read cursor.
    pub fn try_read(&mut self) -> Option<&[u8]> {
        self.buf.inner.try_read()
    }

    /// Release `len` bytes from the consumer side.
    ///
    /// This advances the read cursor, freeing space for the producer.
    pub fn release(&mut self, len: u32) {
        self.buf.inner.release(len);
    }
}

/// A variable-length byte SPSC ring buffer operating on raw pointers.
///
/// BipBuffer protocol:
///
/// **Grant(n):**
/// - If `write >= read`: try end (`capacity - write >= n`), else wrap
///   (`watermark = write`, `write = 0`, check `n <= read`).
/// - If `write < read`: check `write + n <= read`.
///
/// **Commit(n):** `write += n` with Release.
///
/// **Read:**
/// - Load `write` with Acquire.
/// - If `read < write`: readable = `[read..write)`.
/// - If `read >= write` and `watermark != 0`: readable = `[read..watermark)`.
///
/// **Release(n):** `read += n` with Release.
/// - If `read == watermark`: `read = 0`, `watermark = 0`.
pub struct BipBufRaw {
    header: *mut BipBufHeader,
    data: *mut u8,
}

unsafe impl Send for BipBufRaw {}
unsafe impl Sync for BipBufRaw {}

impl BipBufRaw {
    /// Create a BipBuffer view from raw pointers.
    ///
    /// # Safety
    ///
    /// - `header` must point to a valid, initialized `BipBufHeader`
    /// - `data` must point to `header.capacity` bytes of writable memory
    /// - The memory must remain valid for the lifetime of this buffer
    #[inline]
    pub unsafe fn from_raw(header: *mut BipBufHeader, data: *mut u8) -> Self {
        Self { header, data }
    }

    #[inline]
    fn header(&self) -> &BipBufHeader {
        unsafe { &*self.header }
    }

    /// Get the buffer capacity in bytes.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.header().capacity
    }

    /// Try to reserve a contiguous region of `len` bytes for writing.
    ///
    /// Returns a mutable slice on success. The caller must write data
    /// into this slice and then call `commit(len)`.
    ///
    /// # Safety contract
    ///
    /// This method returns `&mut [u8]` from `&self` because `BipBufRaw`
    /// operates on shared memory — the header and data live in an external
    /// memory region, not in `self`. The caller must ensure:
    /// - Only one producer calls `try_grant`/`commit` at a time (SPSC invariant)
    /// - The returned slice is not used after `commit` is called
    /// - `BipBufRaw` is only constructed via `unsafe fn from_raw`
    ///
    /// shm[impl shm.bipbuf.grant]
    #[allow(clippy::mut_from_ref)]
    pub fn try_grant(&self, len: u32) -> Option<&mut [u8]> {
        if len == 0 {
            return Some(&mut []);
        }

        let header = self.header();
        let capacity = header.capacity;
        let write = header.write.load(Ordering::Relaxed);
        let read = header.read.load(Ordering::Acquire);

        if write >= read {
            // Case 1: write is ahead of (or equal to) read.
            // Try to fit at the end of the buffer.
            let space_at_end = capacity - write;
            if space_at_end >= len {
                // Fits at the end.
                let ptr = unsafe { self.data.add(write as usize) };
                return Some(unsafe { core::slice::from_raw_parts_mut(ptr, len as usize) });
            }

            // Not enough at the end. Try wrapping.
            // We can only wrap if read > 0 (otherwise write=0 would collide with read=0
            // and we'd lose the entire buffer contents).
            if read == 0 {
                return None;
            }

            // Set watermark to current write position, then move write to 0.
            header.watermark.store(write, Ordering::Release);
            header.write.store(0, Ordering::Release);

            // Now check if the request fits before read.
            if len <= read {
                let ptr = self.data;
                return Some(unsafe { core::slice::from_raw_parts_mut(ptr, len as usize) });
            }

            // Doesn't fit even after wrapping. Undo the wrap.
            // Restore write and clear watermark.
            header.write.store(write, Ordering::Release);
            header.watermark.store(0, Ordering::Release);
            None
        } else {
            // Case 2: write < read (we previously wrapped).
            // Available space is [write..read).
            if write + len <= read {
                let ptr = unsafe { self.data.add(write as usize) };
                Some(unsafe { core::slice::from_raw_parts_mut(ptr, len as usize) })
            } else {
                None
            }
        }
    }

    /// Commit `len` bytes that were previously granted.
    ///
    /// Makes the data visible to the consumer.
    ///
    /// # Panics
    ///
    /// Panics if `write + len` would exceed the buffer capacity.
    ///
    /// shm[impl shm.bipbuf.commit]
    pub fn commit(&self, len: u32) {
        let header = self.header();
        let write = header.write.load(Ordering::Relaxed);
        let new_write = write.checked_add(len).expect("commit: write overflow");
        assert!(
            new_write <= header.capacity,
            "commit: write ({new_write}) exceeds capacity ({})",
            header.capacity,
        );
        header.write.store(new_write, Ordering::Release);
    }

    /// Try to read contiguous bytes from the buffer.
    ///
    /// Returns a slice of readable bytes, or `None` if the buffer is empty.
    ///
    /// shm[impl shm.bipbuf.read]
    pub fn try_read(&self) -> Option<&[u8]> {
        let header = self.header();
        let read = header.read.load(Ordering::Relaxed);
        let write = header.write.load(Ordering::Acquire);

        if read < write {
            // Normal case: readable region is [read..write).
            let len = write - read;
            let ptr = unsafe { self.data.add(read as usize) };
            Some(unsafe { core::slice::from_raw_parts(ptr, len as usize) })
        } else if read > write || (read == write && read > 0) {
            // We're past the write cursor. Check watermark.
            let watermark = header.watermark.load(Ordering::Acquire);
            if watermark != 0 && read < watermark {
                // Readable region is [read..watermark).
                let len = watermark - read;
                let ptr = unsafe { self.data.add(read as usize) };
                Some(unsafe { core::slice::from_raw_parts(ptr, len as usize) })
            } else if watermark != 0 && read >= watermark {
                // We've reached or passed the watermark. Wrap to beginning.
                // Reset read to 0 and clear watermark.
                header.read.store(0, Ordering::Release);
                header.watermark.store(0, Ordering::Release);
                // Now try again from position 0.
                let write = header.write.load(Ordering::Acquire);
                if write > 0 {
                    let ptr = self.data;
                    Some(unsafe { core::slice::from_raw_parts(ptr, write as usize) })
                } else {
                    None
                }
            } else {
                // No watermark and read >= write: buffer is empty.
                None
            }
        } else {
            // read == write == 0: buffer is empty.
            None
        }
    }

    /// Release `len` bytes from the consumer side.
    ///
    /// Advances the read cursor, freeing space for the producer.
    ///
    /// # Panics
    ///
    /// Panics if `read + len` would overflow or exceed the buffer capacity.
    ///
    /// shm[impl shm.bipbuf.release]
    pub fn release(&self, len: u32) {
        let header = self.header();
        let read = header.read.load(Ordering::Relaxed);
        let new_read = read.checked_add(len).expect("release: read overflow");
        assert!(
            new_read <= header.capacity,
            "release: read ({new_read}) exceeds capacity ({})",
            header.capacity,
        );

        let watermark = header.watermark.load(Ordering::Acquire);
        if watermark != 0 && new_read >= watermark {
            // We've consumed up to or past the watermark. Wrap to 0.
            header.read.store(0, Ordering::Release);
            header.watermark.store(0, Ordering::Release);
        } else {
            header.read.store(new_read, Ordering::Release);
        }
    }

    /// Check if the buffer appears empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        let header = self.header();
        let read = header.read.load(Ordering::Relaxed);
        let write = header.write.load(Ordering::Acquire);
        let watermark = header.watermark.load(Ordering::Acquire);
        read == write && watermark == 0
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::region::HeapRegion;

    fn make_bipbuf(capacity: u32) -> (HeapRegion, BipBuf) {
        let size = BIPBUF_HEADER_SIZE + capacity as usize;
        let region = HeapRegion::new_zeroed(size);
        let buf = unsafe { BipBuf::init(region.region(), 0, capacity) };
        (region, buf)
    }

    #[test]
    fn header_size() {
        assert_eq!(core::mem::size_of::<BipBufHeader>(), 128);
    }

    #[test]
    fn basic_write_read() {
        let (_region, buf) = make_bipbuf(256);
        let (mut producer, mut consumer) = buf.split();

        // Write some data
        let grant = producer.try_grant(10).unwrap();
        grant.copy_from_slice(b"helloworld");
        producer.commit(10);

        // Read it back
        let data = consumer.try_read().unwrap();
        assert_eq!(&data[..10], b"helloworld");
        consumer.release(10);

        // Buffer should be empty now
        assert!(consumer.try_read().is_none());
    }

    #[test]
    fn multiple_writes_and_reads() {
        let (_region, buf) = make_bipbuf(256);
        let (mut producer, mut consumer) = buf.split();

        // Write three messages
        for i in 0..3u8 {
            let grant = producer.try_grant(4).unwrap();
            grant.copy_from_slice(&[i, i + 1, i + 2, i + 3]);
            producer.commit(4);
        }

        // Read all three
        let data = consumer.try_read().unwrap();
        assert_eq!(data.len(), 12);
        assert_eq!(&data[0..4], &[0, 1, 2, 3]);
        assert_eq!(&data[4..8], &[1, 2, 3, 4]);
        assert_eq!(&data[8..12], &[2, 3, 4, 5]);
        consumer.release(12);
    }

    #[test]
    fn wraparound() {
        let (_region, buf) = make_bipbuf(32);
        let (mut producer, mut consumer) = buf.split();

        // Fill most of the buffer
        let grant = producer.try_grant(24).unwrap();
        for (i, byte) in grant.iter_mut().enumerate() {
            *byte = i as u8;
        }
        producer.commit(24);

        // Read and release 20 bytes, leaving 4 bytes unread
        let data = consumer.try_read().unwrap();
        assert_eq!(data.len(), 24);
        consumer.release(20);

        // Now write needs to wrap (only 8 bytes left at end, but we need 16)
        // The write cursor is at 24, read is at 20, capacity is 32.
        // Space at end = 32 - 24 = 8, not enough for 16.
        // But read = 20 > 0, so we can wrap: watermark = 24, write = 0.
        // Space at front = 20 bytes, enough for 16.
        let grant = producer.try_grant(16).unwrap();
        for (i, byte) in grant.iter_mut().enumerate() {
            *byte = 100 + i as u8;
        }
        producer.commit(16);

        // Read the remaining 4 bytes from the first region [20..24)
        let data = consumer.try_read().unwrap();
        assert_eq!(data.len(), 4);
        assert_eq!(data, &[20, 21, 22, 23]);
        consumer.release(4);

        // Now read the wrapped data [0..16)
        let data = consumer.try_read().unwrap();
        assert_eq!(data.len(), 16);
        for (i, &byte) in data.iter().enumerate() {
            assert_eq!(byte, 100 + i as u8);
        }
        consumer.release(16);

        assert!(consumer.try_read().is_none());
    }

    #[test]
    fn full_buffer_returns_none() {
        let (_region, buf) = make_bipbuf(32);
        let (mut producer, _consumer) = buf.split();

        // Fill the buffer completely
        let grant = producer.try_grant(32).unwrap();
        grant.fill(0xAB);
        producer.commit(32);

        // No space left
        assert!(producer.try_grant(1).is_none());
    }

    #[test]
    fn zero_length_grant() {
        let (_region, buf) = make_bipbuf(32);
        let (mut producer, _consumer) = buf.split();

        let grant = producer.try_grant(0).unwrap();
        assert_eq!(grant.len(), 0);
    }

    #[test]
    fn exact_capacity_grant() {
        let (_region, buf) = make_bipbuf(64);
        let (mut producer, mut consumer) = buf.split();

        // Grant exactly the full capacity
        let grant = producer.try_grant(64).unwrap();
        grant.fill(0xFF);
        producer.commit(64);

        // Read it all back
        let data = consumer.try_read().unwrap();
        assert_eq!(data.len(), 64);
        assert!(data.iter().all(|&b| b == 0xFF));
        consumer.release(64);
    }

    #[test]
    fn grant_too_large() {
        let (_region, buf) = make_bipbuf(32);
        let (mut producer, _consumer) = buf.split();

        assert!(producer.try_grant(33).is_none());
    }

    #[test]
    fn interleaved_operations() {
        let (_region, buf) = make_bipbuf(64);
        let (mut producer, mut consumer) = buf.split();

        // Interleave writes and reads
        for round in 0..10u8 {
            let grant = producer.try_grant(8).unwrap();
            for (i, byte) in grant.iter_mut().enumerate() {
                *byte = round * 8 + i as u8;
            }
            producer.commit(8);

            let data = consumer.try_read().unwrap();
            assert_eq!(data.len(), 8);
            for (i, &byte) in data.iter().enumerate() {
                assert_eq!(byte, round * 8 + i as u8);
            }
            consumer.release(8);
        }
    }

    #[test]
    fn wraparound_edge_case_read_at_zero() {
        // When read is at 0 and write fills to capacity, wrapping should fail
        // because write=0 would overlap with read=0.
        let (_region, buf) = make_bipbuf(16);
        let (mut producer, _consumer) = buf.split();

        // Fill 12 bytes
        let grant = producer.try_grant(12).unwrap();
        grant.fill(0xAA);
        producer.commit(12);

        // Can't fit 8 bytes at end (only 4 left) and can't wrap (read=0)
        assert!(producer.try_grant(8).is_none());

        // But we can fit 4 bytes at end
        let grant = producer.try_grant(4).unwrap();
        grant.fill(0xBB);
        producer.commit(4);
    }

    #[test]
    fn stress_many_small_messages() {
        let (_region, buf) = make_bipbuf(1024);
        let (mut producer, mut consumer) = buf.split();

        let mut write_count = 0u32;
        let mut read_count = 0u32;

        for _ in 0..1000 {
            // Write a 4-byte message
            if let Some(grant) = producer.try_grant(4) {
                grant.copy_from_slice(&write_count.to_le_bytes());
                producer.commit(4);
                write_count += 1;
            }

            // Read whatever's available
            if let Some(data) = consumer.try_read() {
                let msg_count = data.len() / 4;
                for i in 0..msg_count {
                    let val = u32::from_le_bytes([
                        data[i * 4],
                        data[i * 4 + 1],
                        data[i * 4 + 2],
                        data[i * 4 + 3],
                    ]);
                    assert_eq!(val, read_count);
                    read_count += 1;
                }
                consumer.release((msg_count * 4) as u32);
            }
        }

        // Drain remaining
        while let Some(data) = consumer.try_read() {
            let msg_count = data.len() / 4;
            for i in 0..msg_count {
                let val = u32::from_le_bytes([
                    data[i * 4],
                    data[i * 4 + 1],
                    data[i * 4 + 2],
                    data[i * 4 + 3],
                ]);
                assert_eq!(val, read_count);
                read_count += 1;
            }
            consumer.release((msg_count * 4) as u32);
        }

        assert_eq!(write_count, read_count);
    }

    #[test]
    fn raw_api_basic() {
        let region = HeapRegion::new_zeroed(BIPBUF_HEADER_SIZE + 64);
        let header_ptr = region.region().as_ptr() as *mut BipBufHeader;
        let data_ptr = unsafe { region.region().as_ptr().add(BIPBUF_HEADER_SIZE) };

        unsafe { (*header_ptr).init(64) };
        let raw = unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) };

        // Write
        let grant = raw.try_grant(10).unwrap();
        grant.copy_from_slice(b"0123456789");
        raw.commit(10);

        // Read
        let data = raw.try_read().unwrap();
        assert_eq!(&data[..10], b"0123456789");
        raw.release(10);

        assert!(raw.is_empty());
    }

    #[test]
    fn raw_api_wraparound() {
        let region = HeapRegion::new_zeroed(BIPBUF_HEADER_SIZE + 32);
        let header_ptr = region.region().as_ptr() as *mut BipBufHeader;
        let data_ptr = unsafe { region.region().as_ptr().add(BIPBUF_HEADER_SIZE) };

        unsafe { (*header_ptr).init(32) };
        let raw = unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) };

        // Write 28 bytes
        let grant = raw.try_grant(28).unwrap();
        grant.fill(0xAA);
        raw.commit(28);

        // Read and release 24 bytes
        let data = raw.try_read().unwrap();
        assert_eq!(data.len(), 28);
        raw.release(24);

        // Write 20 bytes - should wrap
        let grant = raw.try_grant(20).unwrap();
        grant.fill(0xBB);
        raw.commit(20);

        // Read remaining 4 bytes from first region
        let data = raw.try_read().unwrap();
        assert_eq!(data.len(), 4);
        assert!(data.iter().all(|&b| b == 0xAA));
        raw.release(4);

        // Read 20 bytes from wrapped region
        let data = raw.try_read().unwrap();
        assert_eq!(data.len(), 20);
        assert!(data.iter().all(|&b| b == 0xBB));
        raw.release(20);

        assert!(raw.is_empty());
    }

    #[test]
    fn reset_clears_state() {
        let region = HeapRegion::new_zeroed(BIPBUF_HEADER_SIZE + 64);
        let header_ptr = region.region().as_ptr() as *mut BipBufHeader;
        let data_ptr = unsafe { region.region().as_ptr().add(BIPBUF_HEADER_SIZE) };

        unsafe { (*header_ptr).init(64) };
        let raw = unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) };

        // Write some data
        let grant = raw.try_grant(32).unwrap();
        grant.fill(0xFF);
        raw.commit(32);

        // Reset
        unsafe { (*header_ptr).reset() };

        // Should be empty now
        assert!(raw.is_empty());
        assert!(raw.try_read().is_none());

        // Should be able to write again from the start
        let grant = raw.try_grant(64).unwrap();
        assert_eq!(grant.len(), 64);
    }
}
