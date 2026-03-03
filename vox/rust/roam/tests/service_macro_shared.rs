use roam_core::{BareConduit, Driver, acceptor, initiator};
use roam_types::{Link, Parity};

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
        let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
            .establish()
            .await
            .expect("server handshake failed");
        let dispatcher = AdderDispatcher::new(MyAdder);
        let mut server_driver = Driver::new(server_handle, dispatcher, Parity::Even);
        let _server_session_task = tokio::task::spawn(async move { server_session.run().await });
        let _server_driver_task = tokio::task::spawn(async move { server_driver.run().await });
    });

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    let _client_session_task = tokio::task::spawn(async move { client_session.run().await });
    let _client_driver_task = tokio::task::spawn(async move { client_driver.run().await });

    server_task.await.expect("server setup failed");

    let client = AdderClient::new(caller);
    let response = client.add(3, 5).await.expect("add call should succeed");
    assert_eq!(response, 8);

    let response = client.add(100, -42).await.expect("add call should succeed");
    assert_eq!(response, 58);
}
