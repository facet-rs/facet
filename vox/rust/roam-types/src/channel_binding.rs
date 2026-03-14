#![cfg(not(target_arch = "wasm32"))]
//! Channel binding infrastructure for connecting Tx/Rx handles to the driver.
//!
//! Binding functions handle channel binding for request args:
//!
//! - [`bind_channels_caller_args`]: Caller-side, arg position. Allocates IDs,
//!   stores bindings in the shared core so the paired handle can use them.
//! - [`bind_channels_callee_args`]: Callee-side, arg position. Binds deserialized
//!   standalone handles directly using IDs from `Request.channels`.

use std::sync::Arc;

use crate::ChannelId;
use crate::channel::{
    BoundChannelReceiver, BoundChannelSink, ChannelBinding, ChannelLivenessHandle, ChannelSink,
    CoreSlot, ReceiverSlot, ReplenisherSlot, SinkSlot,
};
use crate::rpc_plan::{ChannelKind, RpcPlan};
use facet_core::PtrMut;
use facet_path::PathAccessError;

/// Trait for channel operations, implemented by the session driver.
///
/// This abstraction lets the binding functions and macro-generated code bind
/// channels without depending on concrete driver types.
pub trait ChannelBinder: Send + Sync {
    /// Allocate a channel ID and create a sink for sending items.
    ///
    /// `initial_credit` is the const generic `N` from `Tx<T, N>` or `Rx<T, N>`.
    fn create_tx(&self, initial_credit: u32) -> (ChannelId, Arc<dyn ChannelSink>);

    /// Allocate a channel ID, register it for routing, and return a receiver.
    fn create_rx(&self, initial_credit: u32) -> (ChannelId, BoundChannelReceiver);

    /// Create a sink for a known channel ID (callee side).
    ///
    /// The channel ID comes from `Request.channels`.
    /// `initial_credit` is the const generic `N` from `Tx<T, N>`.
    fn bind_tx(&self, channel_id: ChannelId, initial_credit: u32) -> Arc<dyn ChannelSink>;

    /// Register an inbound channel by ID and return the receiver (callee side).
    ///
    /// The channel ID comes from `Request.channels`.
    fn register_rx(&self, channel_id: ChannelId, initial_credit: u32) -> BoundChannelReceiver;

    /// Optional opaque handle that keeps the underlying session/connection alive
    /// for the lifetime of any bound channel handle.
    fn channel_liveness(&self) -> Option<ChannelLivenessHandle> {
        None
    }
}

// r[impl rpc.channel.binding.caller-args]
// r[impl rpc.channel.allocation]
/// Bind channels in args on the **caller** side, returning channel IDs.
///
/// The caller created `(tx, rx)` pairs via `channel()`. Only one handle from
/// each pair is in the args; the other was kept by the caller. This function
/// stores bindings in the shared core so the kept handle can use them.
///
/// # Safety
///
/// `args_ptr` must point to valid, initialized memory for a value whose
/// shape matches `plan.shape`.
#[allow(unsafe_code)]
pub unsafe fn bind_channels_caller_args(
    args_ptr: *mut u8,
    plan: &RpcPlan,
    binder: &dyn ChannelBinder,
) -> Vec<ChannelId> {
    let shape = plan.shape;
    let mut channel_ids = Vec::new();

    for loc in plan.channel_locations {
        // SAFETY: caller guarantees args_ptr is valid and initialized for this shape
        let poke = unsafe { facet::Poke::from_raw_parts(PtrMut::new(args_ptr), shape) };

        match poke.at_path_mut(&loc.path) {
            Ok(channel_poke) => match loc.kind {
                // r[impl rpc.channel.binding.caller-args.rx]
                // Rx in args: handler receives, caller sends.
                // Create a sink and store it in the shared core so the caller's
                // paired Tx can send through it.
                ChannelKind::Rx => {
                    let (channel_id, sink) = binder.create_tx(loc.initial_credit);
                    channel_ids.push(channel_id);
                    let liveness = binder.channel_liveness();
                    if let Ok(mut ps) = channel_poke.into_struct()
                        && let Ok(mut core_field) = ps.field_by_name("core")
                        && let Ok(slot) = core_field.get_mut::<CoreSlot>()
                        && let Some(core) = &slot.inner
                    {
                        core.set_binding(ChannelBinding::Sink(BoundChannelSink { sink, liveness }));
                    }
                }
                // r[impl rpc.channel.binding.caller-args.tx]
                // Tx in args: handler sends, caller receives.
                // Create a receiver and store it in the shared core so the caller's
                // paired Rx can receive from it.
                ChannelKind::Tx => {
                    let (channel_id, bound) = binder.create_rx(loc.initial_credit);
                    channel_ids.push(channel_id);
                    if let Ok(mut ps) = channel_poke.into_struct()
                        && let Ok(mut core_field) = ps.field_by_name("core")
                        && let Ok(slot) = core_field.get_mut::<CoreSlot>()
                        && let Some(core) = &slot.inner
                    {
                        core.bind_retryable_receiver(bound);
                    }
                }
            },
            Err(PathAccessError::OptionIsNone { .. }) => {
                // Option<Tx/Rx> is None — skip
            }
            Err(_) => {}
        }
    }

    channel_ids
}

/// Finalize caller-side channel bindings after a request settles.
///
/// This closes any retryable paired-channel state so callers blocked on a
/// paired `Rx` observe stream termination once the parent request is done.
///
/// # Safety
///
/// `args_ptr` must point to valid, initialized memory for a value whose
/// shape matches `plan.shape`.
#[allow(unsafe_code)]
pub unsafe fn finalize_channels_caller_args(args_ptr: *mut u8, plan: &RpcPlan) {
    let shape = plan.shape;

    for loc in plan.channel_locations {
        let poke = unsafe { facet::Poke::from_raw_parts(PtrMut::new(args_ptr), shape) };

        match poke.at_path_mut(&loc.path) {
            Ok(channel_poke) => {
                if let Ok(mut ps) = channel_poke.into_struct()
                    && let Ok(mut core_field) = ps.field_by_name("core")
                    && let Ok(slot) = core_field.get_mut::<CoreSlot>()
                    && let Some(core) = &slot.inner
                {
                    core.finish_retry_binding();
                }
            }
            Err(PathAccessError::OptionIsNone { .. }) => {}
            Err(_) => {}
        }
    }
}

// r[impl rpc.channel.binding]
// r[impl rpc.channel.binding.callee-args]
/// Bind channels in deserialized args on the **callee** side.
///
/// Handles are standalone (not part of a pair). Bind directly into the
/// handle's local slot using channel IDs from `Request.channels`.
///
/// # Safety
///
/// `args_ptr` must point to valid, initialized memory for a value whose
/// shape matches `plan.shape`.
#[allow(unsafe_code)]
pub unsafe fn bind_channels_callee_args(
    args_ptr: *mut u8,
    plan: &RpcPlan,
    channel_ids: &[ChannelId],
    binder: &dyn ChannelBinder,
) {
    let shape = plan.shape;
    let mut id_idx = 0;

    for loc in plan.channel_locations {
        // SAFETY: caller guarantees args_ptr is valid and initialized for this shape
        let poke = unsafe { facet::Poke::from_raw_parts(PtrMut::new(args_ptr), shape) };

        match poke.at_path_mut(&loc.path) {
            Ok(channel_poke) => {
                if id_idx >= channel_ids.len() {
                    break;
                }
                let channel_id = channel_ids[id_idx];
                id_idx += 1;

                match loc.kind {
                    // r[impl rpc.channel.binding.callee-args.tx]
                    // Tx in args: handler sends. Bind a sink directly.
                    ChannelKind::Tx => {
                        let sink = binder.bind_tx(channel_id, loc.initial_credit);
                        let liveness = binder.channel_liveness();
                        if let Ok(mut ps) = channel_poke.into_struct() {
                            if let Ok(mut sink_field) = ps.field_by_name("sink")
                                && let Ok(slot) = sink_field.get_mut::<SinkSlot>()
                            {
                                slot.inner = Some(sink);
                            }
                            if let Ok(mut liveness_field) = ps.field_by_name("liveness")
                                && let Ok(slot) =
                                    liveness_field.get_mut::<crate::channel::LivenessSlot>()
                            {
                                slot.inner = liveness;
                            }
                        }
                    }
                    // r[impl rpc.channel.binding.callee-args.rx]
                    // Rx in args: handler receives. Register and bind a receiver directly.
                    ChannelKind::Rx => {
                        let bound = binder.register_rx(channel_id, loc.initial_credit);
                        if let Ok(mut ps) = channel_poke.into_struct() {
                            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
                            {
                                slot.inner = Some(bound.receiver);
                            }
                            if let Ok(mut liveness_field) = ps.field_by_name("liveness")
                                && let Ok(slot) =
                                    liveness_field.get_mut::<crate::channel::LivenessSlot>()
                            {
                                slot.inner = bound.liveness;
                            }
                            if let Ok(mut replenisher_field) = ps.field_by_name("replenisher")
                                && let Ok(slot) = replenisher_field.get_mut::<ReplenisherSlot>()
                            {
                                slot.inner = bound.replenisher;
                            }
                        }
                    }
                }
            }
            Err(PathAccessError::OptionIsNone { .. }) => {
                // Option<Tx/Rx> is None — skip this channel location
            }
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use facet::Facet;
    use tokio::sync::mpsc;

    use crate::channel::{
        BoundChannelReceiver, ChannelSink, IncomingChannelMessage, RxError, TxError, channel,
    };
    use crate::{Backing, ChannelClose, ChannelId, Metadata, Payload, RpcPlan, SelfRef, Tx};

    use super::{ChannelBinder, bind_channels_callee_args, bind_channels_caller_args};

    #[derive(Default)]
    struct TestSink;

    impl ChannelSink for TestSink {
        fn send_payload<'payload>(
            &self,
            _payload: Payload<'payload>,
        ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'payload>> {
            Box::pin(async { Ok(()) })
        }

        fn close_channel(
            &self,
            _metadata: Metadata,
        ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'static>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Default)]
    struct TestBinder {
        next_id: Mutex<u64>,
        create_tx_credits: Mutex<Vec<u32>>,
        bind_tx_calls: Mutex<Vec<(ChannelId, u32)>>,
        register_rx_calls: Mutex<Vec<(ChannelId, u32)>>,
        rx_senders: Mutex<HashMap<u64, mpsc::Sender<IncomingChannelMessage>>>,
    }

    impl TestBinder {
        fn new() -> Self {
            Self {
                next_id: Mutex::new(100),
                ..Self::default()
            }
        }

        fn alloc_id(&self) -> ChannelId {
            let mut guard = self.next_id.lock().expect("next-id mutex poisoned");
            let id = *guard;
            *guard += 2;
            ChannelId(id)
        }

        fn sender_for(&self, channel_id: ChannelId) -> mpsc::Sender<IncomingChannelMessage> {
            self.rx_senders
                .lock()
                .expect("sender map mutex poisoned")
                .get(&channel_id.0)
                .cloned()
                .expect("missing sender for channel id")
        }
    }

    impl ChannelBinder for TestBinder {
        fn create_tx(&self, initial_credit: u32) -> (ChannelId, Arc<dyn ChannelSink>) {
            self.create_tx_credits
                .lock()
                .expect("create-tx mutex poisoned")
                .push(initial_credit);
            (self.alloc_id(), Arc::new(TestSink))
        }

        fn create_rx(&self, _initial_credit: u32) -> (ChannelId, BoundChannelReceiver) {
            let channel_id = self.alloc_id();
            let (tx, rx) = mpsc::channel(8);
            self.rx_senders
                .lock()
                .expect("sender map mutex poisoned")
                .insert(channel_id.0, tx);
            (
                channel_id,
                BoundChannelReceiver {
                    receiver: rx,
                    liveness: None,
                    replenisher: None,
                },
            )
        }

        fn bind_tx(&self, channel_id: ChannelId, initial_credit: u32) -> Arc<dyn ChannelSink> {
            self.bind_tx_calls
                .lock()
                .expect("bind-tx mutex poisoned")
                .push((channel_id, initial_credit));
            Arc::new(TestSink)
        }

        fn register_rx(&self, channel_id: ChannelId, initial_credit: u32) -> BoundChannelReceiver {
            self.register_rx_calls
                .lock()
                .expect("register-rx mutex poisoned")
                .push((channel_id, initial_credit));
            let (tx, rx) = mpsc::channel(8);
            self.rx_senders
                .lock()
                .expect("sender map mutex poisoned")
                .insert(channel_id.0, tx);
            BoundChannelReceiver {
                receiver: rx,
                liveness: None,
                replenisher: None,
            }
        }
    }

    #[derive(Facet)]
    struct CallerArgs {
        tx: crate::Tx<u32, 16>,
        rx: crate::Rx<u32, 16>,
        maybe_tx: Option<crate::Tx<u32, 16>>,
        maybe_rx: Option<crate::Rx<u32, 16>>,
    }

    #[derive(Facet)]
    struct CalleeArgs {
        tx: crate::Tx<u32, 16>,
        rx: crate::Rx<u32, 16>,
    }

    #[tokio::test]
    async fn bind_channels_caller_args_binds_paired_handles_and_skips_none_options() {
        let (tx_arg, mut rx_peer) = channel::<u32>();
        let (tx_peer, rx_arg) = channel::<u32>();
        let mut args = CallerArgs {
            tx: tx_arg,
            rx: rx_arg,
            maybe_tx: None,
            maybe_rx: None,
        };

        let plan = RpcPlan::for_type::<CallerArgs>();
        let binder = TestBinder::new();

        let channel_ids = unsafe {
            bind_channels_caller_args((&mut args as *mut CallerArgs).cast::<u8>(), plan, &binder)
        };

        assert_eq!(
            channel_ids.len(),
            2,
            "only present channels should be bound"
        );
        assert_eq!(
            binder
                .create_tx_credits
                .lock()
                .expect("create-tx mutex poisoned")
                .as_slice(),
            &[16],
            "Rx<T, N> in caller args should allocate sink with declared N credit"
        );

        tx_peer
            .send(77)
            .await
            .expect("paired Tx should become bound via create_tx");

        let close_ref = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelClose {
                metadata: Metadata::default(),
            },
        );
        binder
            .sender_for(channel_ids[0])
            .send(IncomingChannelMessage::Close(close_ref))
            .await
            .expect("send close to paired Rx");
        assert!(
            rx_peer.recv().await.expect("recv close").is_none(),
            "paired Rx should become bound via create_rx"
        );
    }

    #[tokio::test]
    async fn bind_channels_callee_args_binds_tx_and_rx_with_supplied_ids() {
        let mut args = CalleeArgs {
            tx: Tx::unbound(),
            rx: crate::Rx::unbound(),
        };
        let plan = RpcPlan::for_type::<CalleeArgs>();
        let binder = TestBinder::new();
        let channel_ids = [ChannelId(41), ChannelId(43)];

        unsafe {
            bind_channels_callee_args(
                (&mut args as *mut CalleeArgs).cast::<u8>(),
                plan,
                &channel_ids,
                &binder,
            )
        };

        args.tx
            .send(5)
            .await
            .expect("callee-side Tx should be bound via bind_tx");

        let close_ref = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelClose {
                metadata: Metadata::default(),
            },
        );
        binder
            .sender_for(ChannelId(43))
            .send(IncomingChannelMessage::Close(close_ref))
            .await
            .expect("send close to bound callee Rx");
        assert!(args.rx.recv().await.expect("recv close").is_none());

        assert_eq!(
            binder
                .bind_tx_calls
                .lock()
                .expect("bind-tx mutex poisoned")
                .as_slice(),
            &[(ChannelId(41), 16)]
        );
        assert_eq!(
            binder
                .register_rx_calls
                .lock()
                .expect("register-rx mutex poisoned")
                .as_slice(),
            &[(ChannelId(43), 16)]
        );
    }

    #[tokio::test]
    async fn bind_channels_callee_args_stops_when_channel_ids_are_exhausted() {
        let mut args = CalleeArgs {
            tx: Tx::unbound(),
            rx: crate::Rx::unbound(),
        };
        let plan = RpcPlan::for_type::<CalleeArgs>();
        let binder = TestBinder::new();
        let only_one_id = [ChannelId(51)];

        unsafe {
            bind_channels_callee_args(
                (&mut args as *mut CalleeArgs).cast::<u8>(),
                plan,
                &only_one_id,
                &binder,
            )
        };

        args.tx
            .send(1)
            .await
            .expect("first channel location should bind");
        let recv = args.rx.recv().await;
        assert!(
            matches!(recv, Err(RxError::Unbound)),
            "second channel location should remain unbound when IDs are exhausted"
        );
    }
}
