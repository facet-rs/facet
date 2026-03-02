#![cfg(all(test, loom))]

use crate::bipbuf::{BIPBUF_HEADER_SIZE, BipBuf, BipBufHeader, BipBufRaw};
use crate::region::HeapRegion;
use crate::sync::thread;
use loom::sync::Arc;

/// Test BipBuf with concurrent producer and consumer.
#[test]
fn bipbuf_concurrent() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(BIPBUF_HEADER_SIZE + 64));
        let region = region_owner.region();
        let buf = unsafe { BipBuf::init(region, 0, 64) };
        let buf = Arc::new(buf);

        let producer_buf = buf.clone();
        let producer_owner = region_owner.clone();
        let producer_thread = thread::spawn(move || {
            let _keep = producer_owner;
            let (mut producer, _) = producer_buf.split();
            loop {
                if let Some(grant) = producer.try_grant(8) {
                    grant.copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
                    producer.commit(8);
                    break;
                }
                thread::yield_now();
            }
        });

        let consumer_buf = buf.clone();
        let consumer_owner = region_owner.clone();
        let consumer_thread = thread::spawn(move || {
            let _keep = consumer_owner;
            let (_, mut consumer) = consumer_buf.split();
            loop {
                if let Some(data) = consumer.try_read() {
                    let result: Vec<u8> = data[..8].to_vec();
                    consumer.release(8);
                    return result;
                }
                thread::yield_now();
            }
        });

        producer_thread.join().unwrap();
        let received = consumer_thread.join().unwrap();
        assert_eq!(received, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    });
}

/// Test BipBufRaw with concurrent producer and consumer.
#[test]
fn bipbuf_raw_concurrent() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(BIPBUF_HEADER_SIZE + 64));

        let header_ptr = region_owner.region().as_ptr() as *mut BipBufHeader;
        let data_ptr = unsafe { region_owner.region().as_ptr().add(BIPBUF_HEADER_SIZE) };

        unsafe { (*header_ptr).init(64) };
        let raw = unsafe { BipBufRaw::from_raw(header_ptr, data_ptr) };
        let raw = Arc::new(raw);

        let producer_raw = raw.clone();
        let producer_owner = region_owner.clone();
        let producer_thread = thread::spawn(move || {
            let _keep = producer_owner;
            loop {
                if let Some(grant) = producer_raw.try_grant(4) {
                    grant.copy_from_slice(&[10, 20, 30, 40]);
                    producer_raw.commit(4);
                    break;
                }
                thread::yield_now();
            }
        });

        let consumer_raw = raw.clone();
        let consumer_owner = region_owner.clone();
        let consumer_thread = thread::spawn(move || {
            let _keep = consumer_owner;
            loop {
                if let Some(data) = consumer_raw.try_read() {
                    let result: Vec<u8> = data[..4].to_vec();
                    consumer_raw.release(4);
                    return result;
                }
                thread::yield_now();
            }
        });

        producer_thread.join().unwrap();
        let received = consumer_thread.join().unwrap();
        assert_eq!(received, vec![10, 20, 30, 40]);
    });
}

/// Test BipBuf wraparound under concurrency.
///
/// The producer fills most of the buffer, the consumer drains some,
/// then the producer wraps around. This exercises the watermark path
/// that had the load-ordering bug.
#[test]
fn bipbuf_wrap_concurrent() {
    loom::model(|| {
        // Small buffer to force wrapping quickly.
        let region_owner = Arc::new(HeapRegion::new_zeroed(BIPBUF_HEADER_SIZE + 16));
        let region = region_owner.region();
        let buf = unsafe { BipBuf::init(region, 0, 16) };
        let buf = Arc::new(buf);

        let producer_buf = buf.clone();
        let producer_owner = region_owner.clone();
        let producer_thread = thread::spawn(move || {
            let _keep = producer_owner;
            let (mut producer, _) = producer_buf.split();

            // Write 12 bytes (leaves 4 at end).
            loop {
                if let Some(grant) = producer.try_grant(12) {
                    for (i, b) in grant.iter_mut().enumerate() {
                        *b = i as u8;
                    }
                    producer.commit(12);
                    break;
                }
                thread::yield_now();
            }

            // Write 4 more bytes — must wrap (only 4 bytes at end, but
            // consumer may not have freed enough at front yet).
            loop {
                if let Some(grant) = producer.try_grant(4) {
                    grant.copy_from_slice(&[100, 101, 102, 103]);
                    producer.commit(4);
                    break;
                }
                thread::yield_now();
            }
        });

        let consumer_buf = buf.clone();
        let consumer_owner = region_owner.clone();
        let consumer_thread = thread::spawn(move || {
            let _keep = consumer_owner;
            let (_, mut consumer) = consumer_buf.split();
            let mut total = Vec::new();

            // Drain all 16 bytes (12 + 4).
            while total.len() < 16 {
                if let Some(data) = consumer.try_read() {
                    total.extend_from_slice(data);
                    let len = data.len() as u32;
                    consumer.release(len);
                } else {
                    thread::yield_now();
                }
            }
            total
        });

        producer_thread.join().unwrap();
        let received = consumer_thread.join().unwrap();
        assert_eq!(received.len(), 16);
        // First 12 bytes: 0..12
        for (i, &b) in received[..12].iter().enumerate() {
            assert_eq!(b, i as u8);
        }
        // Last 4 bytes: 100..103
        assert_eq!(&received[12..], &[100, 101, 102, 103]);
    });
}
