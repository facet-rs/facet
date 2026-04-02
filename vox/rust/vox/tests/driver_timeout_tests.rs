//! Tests for connect timeout behavior.

use std::time::{Duration, Instant};

use vox::memory_link_pair;

#[vox::service]
trait Echo {
    async fn echo(&self, value: u32) -> u32;
}

/// Connecting to a server that never responds should time out.
#[tokio::test]
async fn connect_timeout_fires_when_server_never_responds() {
    let (client_link, _server_link) = memory_link_pair(16);
    // Server side is dropped — the client handshake will hang forever.

    let start = Instant::now();
    let result = vox::initiator_on(client_link, vox::TransportMode::Bare)
        .connect_timeout(Duration::from_millis(200))
        .establish::<EchoClient>()
        .await;

    let elapsed = start.elapsed();
    match result {
        Err(vox::SessionError::ConnectTimeout) => {} // expected
        Err(other) => panic!("expected ConnectTimeout, got: {other}"),
        Ok(_) => panic!("expected ConnectTimeout, but establish succeeded"),
    }
    assert!(
        elapsed < Duration::from_secs(2),
        "timeout took too long: {elapsed:?}"
    );
}
