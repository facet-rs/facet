use std::sync::Arc;

use afl::fuzz;
use vox_shm::varslot::SizeClassConfig;
use vox_shm::{Segment, SegmentConfig, create_test_link_pair};
use vox_types::{Link, LinkRx, LinkTx, LinkTxPermit, WriteSlot};
use shm_primitives::FileCleanup;

fn main() {
    fuzz!(|data: &[u8]| {
        if data.is_empty() {
            return;
        }

        // Keep inputs bounded so each fuzz case stays fast/deterministic.
        let payload_len = usize::from(data[0]).min(192);
        let payload = &data[1..data.len().min(1 + payload_len)];

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create runtime");
        rt.block_on(async {
            let classes = [SizeClassConfig {
                slot_size: 512,
                slot_count: 4,
            }];
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("fuzz.shm");
            let Ok(segment) = Segment::create(
                &path,
                SegmentConfig {
                    max_guests: 1,
                    bipbuf_capacity: 4096,
                    max_payload_size: 4096,
                    inline_threshold: 64,
                    heartbeat_interval: 0,
                    size_classes: &classes,
                },
                FileCleanup::Manual,
            ) else {
                return;
            };
            let Ok((a, b)) = create_test_link_pair(Arc::new(segment)).await else {
                return;
            };
            let (a_tx, _a_rx) = a.split();
            let (_b_tx, mut b_rx) = b.split();

            let permit = match a_tx.reserve().await {
                Ok(p) => p,
                Err(_) => return,
            };
            let mut slot = match permit.alloc(payload.len()) {
                Ok(s) => s,
                Err(_) => return,
            };
            slot.as_mut_slice().copy_from_slice(payload);
            slot.commit();

            if let Ok(Some(backing)) = b_rx.recv().await {
                assert_eq!(backing.as_bytes(), payload);
            }
        });
    });
}
