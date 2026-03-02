use crate::{BareConduit, memory_link_pair};

type MessageConduit = BareConduit<roam_types::MessageFamily, crate::MemoryLink>;

fn message_conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
}

#[tokio::test]
async fn adder_service_macro_end_to_end() {
    super::service_macro_shared::run_adder_end_to_end(message_conduit_pair).await;
}
