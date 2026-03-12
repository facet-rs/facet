mod service_macro_shared;

use roam_core::{BareConduit, MemoryLink, memory_link_pair};

type MessageConduit = BareConduit<roam_types::MessageFamily, MemoryLink>;

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
