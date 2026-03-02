use moire::task::FutureExt;
use roam_types::{Link, Parity};

use crate::session::{acceptor, initiator};
use crate::{BareConduit, Driver};

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

    let server_task = moire::task::spawn(
        async move {
            let (mut server_session, server_handle, _sh) = acceptor(server_conduit)
                .establish()
                .await
                .expect("server handshake failed");
            let dispatcher = AdderDispatcher::new(MyAdder);
            let mut server_driver = Driver::new(server_handle, dispatcher, Parity::Even);
            moire::task::spawn(async move { server_session.run().await }.named("server_session"));
            moire::task::spawn(async move { server_driver.run().await }.named("server_driver"));
        }
        .named("server_setup"),
    );

    let (mut client_session, client_handle, _sh) = initiator(client_conduit)
        .establish()
        .await
        .expect("client handshake failed");
    let mut client_driver = Driver::new(client_handle, (), Parity::Odd);
    let caller = client_driver.caller();
    moire::task::spawn(async move { client_session.run().await }.named("client_session"));
    moire::task::spawn(async move { client_driver.run().await }.named("client_driver"));

    server_task.await.expect("server setup failed");

    let client = AdderClient::new(caller);
    let response = client.add(3, 5).await.expect("add call should succeed");
    assert_eq!(response, 8);

    let response = client.add(100, -42).await.expect("add call should succeed");
    assert_eq!(response, 58);
}
