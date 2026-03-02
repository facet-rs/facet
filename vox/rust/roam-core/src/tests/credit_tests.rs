use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use roam_types::{ChannelSink, CreditSink, Metadata, Payload, RpcPlan, Rx, Tx, TxError};

/// A sink that completes immediately, counting sends.
struct ImmediateSink {
    send_count: Arc<AtomicUsize>,
}

impl ImmediateSink {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                send_count: count.clone(),
            },
            count,
        )
    }
}

impl ChannelSink for ImmediateSink {
    fn send_payload<'a>(
        &self,
        payload: Payload<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'a>> {
        let count = self.send_count.clone();
        Box::pin(async move {
            let _ = payload;
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn close_channel(
        &self,
        _metadata: Metadata,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'static>>
    {
        Box::pin(async { Ok(()) })
    }
}

// r[verify rpc.flow-control.credit.exhaustion]
#[tokio::test]
async fn credit_blocks_at_zero() {
    let (inner, _count) = ImmediateSink::new();
    let credit_sink = Arc::new(CreditSink::new(inner, 0));
    let mut tx = Tx::<String, 0>::unbound();
    tx.bind(credit_sink.clone());

    // With zero credit, send should block
    let fut = tx.send("hello".to_string());
    tokio::pin!(fut);
    tokio::select! {
        res = &mut fut => panic!("send completed with zero credit: {res:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }

    // Grant one credit — send should now complete
    credit_sink.credit().add_permits(1);
    fut.await.expect("send should succeed after credit grant");
}

// r[verify rpc.flow-control.credit]
// r[verify rpc.flow-control.credit.initial]
// r[verify rpc.flow-control.credit.grant]
#[tokio::test]
async fn credit_allows_n_sends() {
    let (inner, count) = ImmediateSink::new();
    let credit_sink = Arc::new(CreditSink::new(inner, 4));
    let mut tx = Tx::<String, 4>::unbound();
    tx.bind(credit_sink.clone());

    // Should succeed for 4 sends (initial credit = 4)
    for i in 0..4 {
        tx.send(format!("msg-{i}"))
            .await
            .expect("should have credit");
    }
    assert_eq!(count.load(Ordering::SeqCst), 4);

    // 5th send should block (credit exhausted)
    let fut = tx.send("blocked".to_string());
    tokio::pin!(fut);
    tokio::select! {
        res = &mut fut => panic!("5th send should block: {res:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }

    // Grant 2 more credits
    credit_sink.credit().add_permits(2);

    // Blocked send should complete now
    fut.await.expect("send should succeed after grant");

    // One more should work
    tx.send("also-ok".to_string())
        .await
        .expect("should have one more credit");
    assert_eq!(count.load(Ordering::SeqCst), 6);

    // Next should block again
    let fut = tx.send("blocked-again".to_string());
    tokio::pin!(fut);
    tokio::select! {
        res = &mut fut => panic!("should block again: {res:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }
}

// r[verify rpc.flow-control.credit.grant.additive]
#[tokio::test]
async fn credit_grants_are_additive() {
    let (inner, count) = ImmediateSink::new();
    let credit_sink = Arc::new(CreditSink::new(inner, 0));
    let mut tx = Tx::<String, 0>::unbound();
    tx.bind(credit_sink.clone());

    // Grant credit in two batches: 3 + 5 = 8
    credit_sink.credit().add_permits(3);
    credit_sink.credit().add_permits(5);

    // Should be able to send exactly 8 items
    for i in 0..8 {
        tx.send(format!("msg-{i}"))
            .await
            .expect("should have credit");
    }
    assert_eq!(count.load(Ordering::SeqCst), 8);

    // 9th should block
    let fut = tx.send("blocked".to_string());
    tokio::pin!(fut);
    tokio::select! {
        res = &mut fut => panic!("9th send should block: {res:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }
}

// r[verify rpc.flow-control.credit.initial.zero]
#[tokio::test]
async fn credit_initial_zero_blocks_first_send() {
    let (inner, _count) = ImmediateSink::new();
    let credit_sink = Arc::new(CreditSink::new(inner, 0));
    let mut tx = Tx::<String, 0>::unbound();
    tx.bind(credit_sink.clone());

    // N=0: first send must block
    let fut = tx.send("first".to_string());
    tokio::pin!(fut);
    tokio::select! {
        res = &mut fut => panic!("N=0 should block first send: {res:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }

    // Explicit grant unblocks
    credit_sink.credit().add_permits(1);
    fut.await.expect("should succeed after explicit grant");
}

#[test]
fn initial_credit_from_const_generic() {
    use facet::Facet;
    use roam_types::ChannelKind;

    // A struct containing channels with various N values
    #[derive(Facet)]
    struct MixedChannels {
        tx_default: Tx<String>,     // N = 16
        tx_small: Tx<String, 4>,    // N = 4
        rx_zero: Rx<String, 0>,     // N = 0
        tx_large: Tx<String, 1024>, // N = 1024
    }

    let plan = RpcPlan::for_type::<MixedChannels>();
    assert_eq!(plan.channel_locations.len(), 4);

    assert_eq!(plan.channel_locations[0].kind, ChannelKind::Tx);
    assert_eq!(plan.channel_locations[0].initial_credit, 16);

    assert_eq!(plan.channel_locations[1].kind, ChannelKind::Tx);
    assert_eq!(plan.channel_locations[1].initial_credit, 4);

    assert_eq!(plan.channel_locations[2].kind, ChannelKind::Rx);
    assert_eq!(plan.channel_locations[2].initial_credit, 0);

    assert_eq!(plan.channel_locations[3].kind, ChannelKind::Tx);
    assert_eq!(plan.channel_locations[3].initial_credit, 1024);
}
