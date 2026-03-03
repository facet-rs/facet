use roam_core::{BareConduit, acceptor, initiator};
use roam_types::Link;

type MessageConduit<L> = BareConduit<roam_types::MessageFamily, L>;

#[roam::service]
trait Adder {
    async fn add(&self, a: i32, b: i32) -> i32;
}

#[derive(Clone)]
struct MyAdder;

impl Adder for MyAdder {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

pub async fn run_adder_end_to_end<L>(
    message_conduit_pair: impl FnOnce() -> (MessageConduit<L>, MessageConduit<L>),
) where
    L: Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = tokio::task::spawn(async move {
        let ((), _sh) = acceptor(server_conduit)
            .establish::<()>(AdderDispatcher::new(MyAdder))
            .await
            .expect("server handshake failed");
    });

    let (client, _sh) = initiator(client_conduit)
        .establish::<AdderClient<_>>(())
        .await
        .expect("client handshake failed");

    server_task.await.expect("server setup failed");
    let response = client.add(3, 5).await.expect("add call should succeed");
    assert_eq!(response, 8);

    let response = client.add(100, -42).await.expect("add call should succeed");
    assert_eq!(response, 58);
}
