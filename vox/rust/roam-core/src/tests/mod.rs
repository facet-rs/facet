use facet::Facet;
use facet_core::ConstParamKind;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use roam_types::{
    Backing, ChannelClose, ChannelItem, ChannelReset, ChannelSink, Conduit, ConduitRx, ConduitTx,
    ConduitTxPermit, IncomingChannelMessage, Metadata, MsgFamily, Payload, Rx, RxError, SelfRef,
    Tx, TxError,
};
use tokio::sync::{Notify, mpsc};

use crate::{BareConduit, MemoryLink, memory_link_pair};

struct StringFamily;

impl MsgFamily for StringFamily {
    type Msg<'a> = String;

    fn shape() -> &'static facet_core::Shape {
        String::SHAPE
    }
}

type StringConduit = BareConduit<StringFamily, MemoryLink>;

/// Create a connected pair of BareConduits over MemoryLink for String messages.
fn conduit_pair() -> (StringConduit, StringConduit) {
    let (a, b) = memory_link_pair(16);
    (BareConduit::new(a), BareConduit::new(b))
}

#[tokio::test]
async fn send_recv_single() {
    let (client, server) = conduit_pair();
    let (client_tx, _client_rx) = client.split();
    let (_server_tx, mut server_rx) = server.split();

    let permit = client_tx.reserve().await.unwrap();
    permit.send("hello".to_string()).unwrap();

    let received = server_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*received, "hello");
}

#[tokio::test]
async fn send_recv_multiple_in_order() {
    let (client, server) = conduit_pair();
    let (client_tx, _client_rx) = client.split();
    let (_server_tx, mut server_rx) = server.split();

    for i in 0..10 {
        let permit = client_tx.reserve().await.unwrap();
        permit.send(format!("msg-{i}")).unwrap();
    }

    for i in 0..10 {
        let received = server_rx.recv().await.unwrap().unwrap();
        assert_eq!(&*received, &format!("msg-{i}"));
    }
}

#[tokio::test]
async fn bidirectional() {
    let (client, server) = conduit_pair();
    let (client_tx, mut client_rx) = client.split();
    let (server_tx, mut server_rx) = server.split();

    let permit = client_tx.reserve().await.unwrap();
    permit.send("from-client".to_string()).unwrap();

    let received = server_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*received, "from-client");

    let permit = server_tx.reserve().await.unwrap();
    permit.send("from-server".to_string()).unwrap();

    let received = client_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*received, "from-server");
}

#[tokio::test]
async fn close_signals_end() {
    let (client, server) = conduit_pair();
    let (client_tx, _client_rx) = client.split();
    let (_server_tx, mut server_rx) = server.split();

    let permit = client_tx.reserve().await.unwrap();
    permit.send("last".to_string()).unwrap();
    client_tx.close().await.unwrap();

    let received = server_rx.recv().await.unwrap().unwrap();
    assert_eq!(&*received, "last");

    let end = server_rx.recv().await.unwrap();
    assert!(end.is_none());
}

#[tokio::test]
async fn interleaved_send_recv() {
    let (client, server) = conduit_pair();
    let (client_tx, mut client_rx) = client.split();
    let (server_tx, mut server_rx) = server.split();

    for i in 0..5 {
        let permit = client_tx.reserve().await.unwrap();
        permit.send(format!("c2s-{i}")).unwrap();

        let received = server_rx.recv().await.unwrap().unwrap();
        assert_eq!(&*received, &format!("c2s-{i}"));

        let permit = server_tx.reserve().await.unwrap();
        permit.send(format!("s2c-{i}")).unwrap();

        let received = client_rx.recv().await.unwrap().unwrap();
        assert_eq!(&*received, &format!("s2c-{i}"));
    }
}

#[test]
fn tx_rx_const_param_n() {
    let tx_shape = Tx::<String, 32>::SHAPE;
    let cp = tx_shape.const_params;
    assert_eq!(cp.len(), 1);
    assert_eq!(cp[0].name, "N");
    assert_eq!(cp[0].kind, ConstParamKind::Usize);
    assert_eq!(cp[0].value, 32);

    let rx_shape = Rx::<String, 8>::SHAPE;
    let cp = rx_shape.const_params;
    assert_eq!(cp.len(), 1);
    assert_eq!(cp[0].name, "N");
    assert_eq!(cp[0].kind, ConstParamKind::Usize);
    assert_eq!(cp[0].value, 8);
}

struct TestSink {
    gate: Arc<Notify>,
    send_count: Arc<AtomicUsize>,
    close_count: Arc<AtomicUsize>,
    saw_owned_payload: Arc<AtomicBool>,
}

struct DropCloseSink {
    close_count: Arc<AtomicUsize>,
}

impl DropCloseSink {
    fn new() -> Self {
        Self {
            close_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl ChannelSink for DropCloseSink {
    fn send_payload<'a>(
        &self,
        _payload: Payload<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn close_channel(
        &self,
        _metadata: Metadata,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'static>>
    {
        let close_count = self.close_count.clone();
        Box::pin(async move {
            close_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn close_channel_on_drop(&self) {
        self.close_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl TestSink {
    fn new() -> Self {
        Self {
            gate: Arc::new(Notify::new()),
            send_count: Arc::new(AtomicUsize::new(0)),
            close_count: Arc::new(AtomicUsize::new(0)),
            saw_owned_payload: Arc::new(AtomicBool::new(false)),
        }
    }

    fn open_gate(&self) {
        self.gate.notify_waiters();
    }
}

impl ChannelSink for TestSink {
    fn send_payload<'a>(
        &self,
        payload: Payload<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'a>> {
        let gate = self.gate.clone();
        let send_count = self.send_count.clone();
        let saw_owned_payload = self.saw_owned_payload.clone();
        Box::pin(async move {
            if matches!(payload, Payload::Outgoing { .. }) {
                saw_owned_payload.store(true, Ordering::SeqCst);
            }
            gate.notified().await;
            let _ = payload;
            send_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn close_channel(
        &self,
        metadata: Metadata,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'static>>
    {
        let gate = self.gate.clone();
        let close_count = self.close_count.clone();
        Box::pin(async move {
            gate.notified().await;
            let _ = metadata;
            close_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

#[tokio::test]
async fn tx_send_waits_for_sink_completion() {
    let mut tx = Tx::<String>::unbound();
    let sink = Arc::new(TestSink::new());
    tx.bind(sink.clone());

    let payload = "hello".to_string();
    let fut = tx.send(payload);
    tokio::pin!(fut);
    tokio::select! {
        res = &mut fut => panic!("send completed too early: {res:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {}
    }

    sink.open_gate();
    fut.await.expect("send should complete once sink opens");
    assert_eq!(sink.send_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn tx_drop_triggers_close_signal() {
    let mut tx = Tx::<u32>::unbound();
    let sink = Arc::new(DropCloseSink::new());
    tx.bind(sink.clone());

    drop(tx);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(
        sink.close_count.load(Ordering::SeqCst),
        1,
        "dropping Tx should emit channel close"
    );
}

#[tokio::test]
async fn tx_close_then_drop_emits_single_close_signal() {
    let mut tx = Tx::<u32>::unbound();
    let sink = Arc::new(DropCloseSink::new());
    tx.bind(sink.clone());

    tx.close(Metadata::default())
        .await
        .expect("explicit close should succeed");
    drop(tx);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(
        sink.close_count.load(Ordering::SeqCst),
        1,
        "drop after explicit close should not emit duplicate close"
    );
}

#[derive(Facet)]
struct BorrowedMsg<'a> {
    text: &'a str,
}

#[tokio::test]
async fn tx_send_accepts_borrowed_payloads() {
    let mut tx: Tx<BorrowedMsg<'_>> = Tx::unbound();
    let sink = Arc::new(TestSink::new());
    tx.bind(sink.clone());

    let backing = String::from("borrowed");
    let msg = BorrowedMsg { text: &backing };
    let fut = tx.send(msg);
    tokio::pin!(fut);
    tokio::select! {
        res = &mut fut => panic!("send completed too early: {res:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {}
    }
    sink.open_gate();
    fut.await.expect("borrowed send should succeed");
    assert!(
        sink.saw_owned_payload.load(Ordering::SeqCst),
        "send(value) should route through Payload::Owned"
    );
}

// r[verify rpc.channel.item]
// r[verify rpc.channel.close]
#[tokio::test]
async fn rx_recv_decodes_channel_items() {
    let mut rx = Rx::<u32>::unbound();
    let (tx_items, rx_items) = mpsc::channel(4);
    rx.bind(rx_items);

    let payload_bytes = facet_postcard::to_vec(&42_u32).expect("serialize channel item");
    let backing = Backing::Boxed(payload_bytes.into_boxed_slice());
    let item_ref = SelfRef::try_new(backing, |bytes| {
        Ok::<_, std::convert::Infallible>(ChannelItem {
            item: Payload::Incoming(bytes),
        })
    })
    .unwrap();
    tx_items
        .send(IncomingChannelMessage::Item(item_ref))
        .await
        .expect("send item to rx");

    let received = rx.recv().await.expect("recv data");
    assert_eq!(*received.unwrap(), 42);

    let close = ChannelClose {
        metadata: Metadata::default(),
    };
    let close_ref = SelfRef::owning(Backing::Boxed(Box::<[u8]>::default()), close);
    tx_items
        .send(IncomingChannelMessage::Close(close_ref))
        .await
        .expect("send close to rx");
    assert!(rx.recv().await.expect("recv close").is_none());
}

// r[verify rpc.channel.reset]
#[tokio::test]
async fn rx_recv_signals_reset() {
    let mut rx = Rx::<u32>::unbound();
    let (tx_items, rx_items) = mpsc::channel(4);
    rx.bind(rx_items);

    let reset = ChannelReset {
        metadata: Metadata::default(),
    };
    let reset_ref = SelfRef::owning(Backing::Boxed(Box::<[u8]>::default()), reset);
    tx_items
        .send(IncomingChannelMessage::Reset(reset_ref))
        .await
        .expect("send reset to rx");

    let result = rx.recv().await;
    assert!(
        matches!(result, Err(RxError::Reset)),
        "expected RxError::Reset"
    );
}

mod credit_tests;
mod driver_tests;
mod service_macro_shared;
mod service_macro_tests;
mod shm_driver_tests;
mod shm_service_macro_tests;

#[test]
fn test_deser_postcard_borrowed() {
    // A reply
    #[derive(Facet)]
    struct Reply<'a> {
        s: &'a str,
    }

    let payload = facet_postcard::to_vec(&Reply {
        s: "IAMA borrowed string AMA",
    })
    .unwrap();

    let backing = Backing::Boxed(payload.into_boxed_slice());

    // now deser with Backing
    let reply = crate::deserialize_postcard::<Reply>(backing).unwrap();
    assert_eq!(reply.s, "IAMA borrowed string AMA")
}
