mod service_macro_shared;

use vox_core::{BareConduit, MemoryLink, memory_link_pair};

type MessageConduit = BareConduit<vox_types::MessageFamily, MemoryLink>;

fn message_conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
}

#[tokio::test]
async fn adder_service_macro_end_to_end() {
    service_macro_shared::run_adder_end_to_end(message_conduit_pair).await;
}

#[tokio::test]
async fn request_context_opt_in_end_to_end() {
    service_macro_shared::run_request_context_end_to_end(message_conduit_pair).await;
}

#[tokio::test]
async fn server_middleware_end_to_end() {
    service_macro_shared::run_server_middleware_end_to_end(message_conduit_pair).await;
}

#[tokio::test]
async fn server_request_peek_end_to_end() {
    service_macro_shared::run_server_request_peek_end_to_end(message_conduit_pair).await;
}

#[tokio::test]
async fn server_response_peek_end_to_end() {
    service_macro_shared::run_server_response_peek_end_to_end(message_conduit_pair).await;
}

#[tokio::test]
async fn client_middleware_end_to_end() {
    service_macro_shared::run_client_middleware_end_to_end(message_conduit_pair).await;
}

#[tokio::test]
async fn borrowed_return_survives_teardown_inline() {
    service_macro_shared::run_borrowed_return_survives_teardown_over_generated_client(
        message_conduit_pair,
        service_macro_shared::BorrowedPayloadKind::Inline,
    )
    .await;
}

#[tokio::test]
async fn borrowed_return_survives_teardown_slot_ref() {
    service_macro_shared::run_borrowed_return_survives_teardown_over_generated_client(
        message_conduit_pair,
        service_macro_shared::BorrowedPayloadKind::SlotRef,
    )
    .await;
}

#[tokio::test]
async fn borrowed_return_survives_teardown_mmap_ref() {
    service_macro_shared::run_borrowed_return_survives_teardown_over_generated_client(
        message_conduit_pair,
        service_macro_shared::BorrowedPayloadKind::MmapRef,
    )
    .await;
}
