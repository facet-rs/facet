use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use vox_types::{
    ChannelDebugContext, ChannelEvent, ChannelId, ChannelSink, ChannelTrySendOutcome, CreditSink,
    LaneId, Metadata, Payload, TrySendError, Tx, TxError, VoxObserver, VoxObserverHandle,
};

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
    ) -> std::pin::Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'a>> {
        let count = self.send_count.clone();
        Box::pin(async move {
            let _ = payload;
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }

    fn try_send_payload<'a>(&self, payload: Payload<'a>) -> Result<(), TrySendError<()>> {
        let _ = payload;
        self.send_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn close_channel(
        &self,
        _metadata: Metadata,
    ) -> std::pin::Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'static>>
    {
        Box::pin(async { Ok(()) })
    }
}

struct RecordingObserver {
    events: Arc<Mutex<Vec<ChannelEvent>>>,
}

impl VoxObserver for RecordingObserver {
    fn channel_event(&self, event: ChannelEvent) {
        self.events.lock().unwrap().push(event);
    }
}

struct ObservedTrySendSink {
    lane_id: LaneId,
    channel_id: ChannelId,
    debug_context: ChannelDebugContext,
    observer: VoxObserverHandle,
    outcome: Option<ChannelTrySendOutcome>,
}

impl ChannelSink for ObservedTrySendSink {
    fn send_payload<'a>(
        &self,
        _payload: Payload<'a>,
    ) -> std::pin::Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn channel_id(&self) -> Option<ChannelId> {
        Some(self.channel_id)
    }

    fn lane_id(&self) -> Option<LaneId> {
        Some(self.lane_id)
    }

    fn debug_context(&self) -> Option<ChannelDebugContext> {
        Some(self.debug_context)
    }

    fn observer(&self) -> Option<VoxObserverHandle> {
        Some(self.observer.clone())
    }

    fn try_send_payload_with_outcome<'a>(
        &self,
        _payload: Payload<'a>,
    ) -> Result<(), ChannelTrySendOutcome> {
        match self.outcome {
            Some(outcome) => Err(outcome),
            None => Ok(()),
        }
    }

    fn close_channel(
        &self,
        _metadata: Metadata,
    ) -> std::pin::Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'static>>
    {
        Box::pin(async { Ok(()) })
    }
}

// r[verify rpc.flow-control.credit.try-send]
// r[verify rpc.flow-control.credit.exhaustion]
#[test]
fn try_send_returns_full_with_value_when_credit_is_exhausted() {
    let (inner, count) = ImmediateSink::new();
    let credit_sink = Arc::new(CreditSink::new(inner, 1));
    let mut tx = Tx::<String>::unbound();
    tx.bind(credit_sink);

    tx.try_send("first".to_string())
        .expect("first send should use initial credit");
    assert_eq!(count.load(Ordering::SeqCst), 1);

    match tx.try_send("second".to_string()) {
        Err(TrySendError::Full(value)) => assert_eq!(value, "second"),
        other => panic!("expected Full(second), got {other:?}"),
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

// r[verify rpc.flow-control.credit.try-send]
#[test]
fn try_send_returns_closed_with_value_when_credit_is_closed() {
    let (inner, count) = ImmediateSink::new();
    let credit_sink = Arc::new(CreditSink::new(inner, 0));
    credit_sink.credit().close();

    let mut tx = Tx::<String>::unbound();
    tx.bind(credit_sink);

    match tx.try_send("closed".to_string()) {
        Err(TrySendError::Closed(value)) => assert_eq!(value, "closed"),
        other => panic!("expected Closed(closed), got {other:?}"),
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

// r[verify rpc.observability.channel.try-send-detail]
#[test]
fn observer_distinguishes_try_send_full_credit_from_runtime_queue() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer: VoxObserverHandle = Arc::new(RecordingObserver {
        events: events.clone(),
    });

    let mut credit_full_tx = Tx::<u32>::unbound();
    credit_full_tx.bind(Arc::new(CreditSink::new(
        ObservedTrySendSink {
            lane_id: LaneId(5),
            channel_id: ChannelId(7),
            debug_context: ChannelDebugContext {
                label: Some("credit-full"),
                ..ChannelDebugContext::default()
            },
            observer: observer.clone(),
            outcome: None,
        },
        0,
    )));
    assert!(matches!(
        credit_full_tx.try_send(1),
        Err(TrySendError::Full(1))
    ));

    let mut runtime_full_tx = Tx::<u32>::unbound();
    runtime_full_tx.bind(Arc::new(CreditSink::new(
        ObservedTrySendSink {
            lane_id: LaneId(5),
            channel_id: ChannelId(9),
            debug_context: ChannelDebugContext {
                label: Some("runtime-full"),
                ..ChannelDebugContext::default()
            },
            observer,
            outcome: Some(ChannelTrySendOutcome::FullRuntimeQueue),
        },
        1,
    )));
    assert!(matches!(
        runtime_full_tx.try_send(2),
        Err(TrySendError::Full(2))
    ));

    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        ChannelEvent::TrySend { channel, outcome: ChannelTrySendOutcome::FullCredit }
            if channel.lane_id == Some(LaneId(5))
                && channel.channel_id == ChannelId(7)
                && channel.debug.and_then(|debug| debug.label) == Some("credit-full")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ChannelEvent::TrySend { channel, outcome: ChannelTrySendOutcome::FullRuntimeQueue }
            if channel.lane_id == Some(LaneId(5))
                && channel.channel_id == ChannelId(9)
                && channel.debug.and_then(|debug| debug.label) == Some("runtime-full")
    )));
}

// r[verify rpc.flow-control.credit.exhaustion]
#[tokio::test]
async fn credit_blocks_at_zero() {
    let (inner, _count) = ImmediateSink::new();
    let credit_sink = Arc::new(CreditSink::new(inner, 0));
    let mut tx = Tx::<String>::unbound();
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
    let mut tx = Tx::<String>::unbound();
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
    let mut tx = Tx::<String>::unbound();
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
    let mut tx = Tx::<String>::unbound();
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
