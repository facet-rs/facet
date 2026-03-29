mod service_macro_shared;

use std::sync::Arc;

use shm_primitives::FileCleanup;
use vox_core::BareConduit;
use vox_shm::varslot::SizeClassConfig;
use vox_shm::{Segment, SegmentConfig, ShmLink, create_test_link_pair};

type MessageConduit = BareConduit<vox_types::MessageFamily, ShmLink>;

async fn message_conduit_pair() -> (MessageConduit, MessageConduit, tempfile::TempDir) {
    let classes = [SizeClassConfig {
        slot_size: 4096,
        slot_count: 16,
    }];
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("shm-service-macro-test.shm");
    let segment = Arc::new(
        Segment::create(
            &path,
            SegmentConfig {
                max_guests: 1,
                bipbuf_capacity: 1 << 16,
                max_payload_size: 1 << 20,
                inline_threshold: 256,
                heartbeat_interval: 0,
                size_classes: &classes,
            },
            FileCleanup::Manual,
        )
        .expect("create segment"),
    );
    let (a, b) = create_test_link_pair(segment)
        .await
        .expect("create_test_link_pair");
    (BareConduit::new(a), BareConduit::new(b), dir)
}

#[tokio::test]
async fn adder_service_macro_end_to_end_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_adder_end_to_end(|| (a, b)).await;
}

#[tokio::test]
async fn request_context_opt_in_end_to_end_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_request_context_end_to_end(|| (a, b)).await;
}

#[tokio::test]
async fn server_middleware_end_to_end_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_server_middleware_end_to_end(|| (a, b)).await;
}

#[tokio::test]
async fn server_request_peek_end_to_end_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_server_request_peek_end_to_end(|| (a, b)).await;
}

#[tokio::test]
async fn server_response_peek_end_to_end_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_server_response_peek_end_to_end(|| (a, b)).await;
}

#[tokio::test]
async fn client_middleware_end_to_end_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_client_middleware_end_to_end(|| (a, b)).await;
}

#[tokio::test]
async fn borrowed_return_survives_teardown_inline_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_borrowed_return_survives_teardown_over_generated_client(
        || (a, b),
        service_macro_shared::BorrowedPayloadKind::Inline,
    )
    .await;
}

#[tokio::test]
async fn borrowed_return_survives_teardown_slot_ref_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_borrowed_return_survives_teardown_over_generated_client(
        || (a, b),
        service_macro_shared::BorrowedPayloadKind::SlotRef,
    )
    .await;
}

#[tokio::test]
async fn borrowed_return_survives_teardown_mmap_ref_over_shm() {
    let (a, b, _dir) = message_conduit_pair().await;
    service_macro_shared::run_borrowed_return_survives_teardown_over_generated_client(
        || (a, b),
        service_macro_shared::BorrowedPayloadKind::MmapRef,
    )
    .await;
}
