//! Property-based tests for rapace invariants

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use crate::types::*;
    use crate::header::*;
    use crate::flow::*;
    use crate::ring::*;
    use crate::layout::*;
    use std::ptr::NonNull;
    use std::alloc::{alloc_zeroed, dealloc, Layout};

    // === Header roundtrip properties ===

    proptest! {
        #[test]
        fn header_roundtrip_preserves_data(
            version in 0u16..=100,
            encoding in 1u16..=3u16,
            flags in any::<u16>(),
            correlation_id in any::<u64>(),
            deadline_ns in any::<u64>(),
        ) {
            let encoding = match encoding {
                1 => Encoding::Postcard,
                2 => Encoding::Json,
                _ => Encoding::Raw,
            };

            let header = MsgHeader {
                version,
                encoding,
                flags,
                correlation_id,
                deadline_ns,
                metadata: Metadata::default(),
            };

            let mut buf = vec![0u8; 256];
            let len = header.encode_into(&mut buf).unwrap();
            let (decoded, decoded_len) = MsgHeader::decode_from(&buf).unwrap();

            prop_assert_eq!(len, decoded_len);
            prop_assert_eq!(decoded.version, version);
            prop_assert_eq!(decoded.encoding, encoding);
            prop_assert_eq!(decoded.flags, flags);
            prop_assert_eq!(decoded.correlation_id, correlation_id);
            prop_assert_eq!(decoded.deadline_ns, deadline_ns);
        }

        #[test]
        fn header_decode_never_panics(data: Vec<u8>) {
            // Should return Err, never panic
            let _ = MsgHeader::decode_from(&data);
        }
    }

    // === Flow control properties ===

    proptest! {
        #[test]
        fn credits_conservation(
            initial in 1u32..10000,
            reserves in prop::collection::vec(1u32..100, 0..20),
            consumes in prop::collection::vec(any::<bool>(), 0..20),
        ) {
            let credits = Credits::new(initial);
            let mut total_consumed = 0u32;
            let mut permits = Vec::new();

            for (reserve, consume) in reserves.iter().zip(consumes.iter().cycle()) {
                if let Ok(permit) = credits.try_reserve(ByteLen::new(*reserve, 10000).unwrap()) {
                    if *consume {
                        total_consumed += permit.amount();
                        permit.consume();
                    } else {
                        permits.push(permit);
                    }
                }
            }

            // Drop remaining permits (returns credits)
            drop(permits);

            // Conservation: initial = available + consumed
            prop_assert_eq!(credits.available(), initial - total_consumed);
        }

        #[test]
        fn credits_never_go_negative(
            initial in 1u32..1000,
            ops in prop::collection::vec((1u32..500, any::<bool>()), 0..50),
        ) {
            let credits = Credits::new(initial);

            for (amount, is_grant) in ops {
                if is_grant {
                    credits.grant(amount);
                } else {
                    let _ = credits.try_reserve(ByteLen::new(amount, 10000).unwrap());
                }
                // Available should never underflow
                prop_assert!(credits.available() <= u32::MAX / 2); // Sanity check
            }
        }
    }

    // === Ring buffer properties ===

    /// Test helper for ring allocation
    struct TestRing {
        ptr: *mut u8,
        layout: Layout,
        capacity: u32,
    }

    impl TestRing {
        fn new(capacity: u32) -> Self {
            let capacity = capacity.next_power_of_two();
            let header_size = std::mem::size_of::<DescRingHeader>();
            let descs_size = std::mem::size_of::<MsgDescHot>() * capacity as usize;
            let total_size = header_size + descs_size;
            let layout = Layout::from_size_align(total_size, 64).unwrap();

            let ptr = unsafe { alloc_zeroed(layout) };
            assert!(!ptr.is_null());

            unsafe {
                let header = ptr as *mut DescRingHeader;
                (*header).capacity = capacity;
            }

            TestRing { ptr, layout, capacity }
        }

        fn as_ring(&self) -> Ring {
            unsafe {
                Ring::from_raw(
                    NonNull::new(self.ptr as *mut DescRingHeader).unwrap(),
                    self.capacity,
                )
            }
        }
    }

    impl Drop for TestRing {
        fn drop(&mut self) {
            unsafe { dealloc(self.ptr, self.layout) }
        }
    }

    proptest! {
        #[test]
        fn ring_fifo_ordering(
            capacity in (2u32..=64).prop_map(|c| c.next_power_of_two()),
            msg_ids in prop::collection::vec(any::<u64>(), 1..100),
        ) {
            let test_ring = TestRing::new(capacity);
            let mut ring = test_ring.as_ring();
            let (mut producer, mut consumer) = ring.split();

            let mut received = Vec::new();
            let mut sent_count = 0;

            for msg_id in &msg_ids {
                // Try to enqueue
                let mut desc = MsgDescHot::default();
                desc.msg_id = *msg_id;

                if producer.try_enqueue(desc).is_ok() {
                    sent_count += 1;
                }

                // Drain some
                while let Some(d) = consumer.try_dequeue() {
                    received.push(d.msg_id);
                }
            }

            // Drain remaining
            while let Some(d) = consumer.try_dequeue() {
                received.push(d.msg_id);
            }

            // All sent messages should be received in order
            prop_assert_eq!(received.len(), sent_count);

            // Check FIFO ordering
            let expected: Vec<u64> = msg_ids.iter().take(sent_count).copied().collect();
            prop_assert_eq!(received, expected);
        }

        #[test]
        fn ring_never_loses_messages(
            capacity in (2u32..=32).prop_map(|c| c.next_power_of_two()),
            ops in prop::collection::vec(prop::bool::ANY, 1..200),
        ) {
            let test_ring = TestRing::new(capacity);
            let mut ring = test_ring.as_ring();
            let (mut producer, mut consumer) = ring.split();

            let mut sent = 0u64;
            let mut received = 0u64;

            for do_send in ops {
                if do_send {
                    let mut desc = MsgDescHot::default();
                    desc.msg_id = sent;
                    if producer.try_enqueue(desc).is_ok() {
                        sent += 1;
                    }
                } else {
                    if consumer.try_dequeue().is_some() {
                        received += 1;
                    }
                }
            }

            // Drain remaining
            while consumer.try_dequeue().is_some() {
                received += 1;
            }

            // No messages lost
            prop_assert_eq!(sent, received);
        }
    }

    // === ByteLen validation ===

    proptest! {
        #[test]
        fn bytelen_respects_max(len in any::<u32>(), max in any::<u32>()) {
            let result = ByteLen::new(len, max);
            if len <= max {
                prop_assert!(result.is_some());
                prop_assert_eq!(result.unwrap().get(), len);
            } else {
                prop_assert!(result.is_none());
            }
        }
    }

    // === ChannelId validation ===

    proptest! {
        #[test]
        fn channel_id_zero_always_none(id in 0u32..=0u32) {
            prop_assert!(ChannelId::new(id).is_none());
        }

        #[test]
        fn channel_id_nonzero_always_some(id in 1u32..=u32::MAX) {
            let channel = ChannelId::new(id);
            prop_assert!(channel.is_some());
            prop_assert_eq!(channel.unwrap().get(), id);
        }
    }
}
