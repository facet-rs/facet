use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use moire::sync::mpsc;
use roam_types::{
    Conduit, ConduitTx, ConnectionSettings, Handler, HandshakeResult, Link, MaybeSend, MaybeSync,
    MessageFamily, Metadata, Parity, SessionResumeKey, SplitLink,
};

#[cfg(not(target_arch = "wasm32"))]
use crate::{Attachment, LinkSource, StableConduit};
use crate::{
    BareConduit, IntoConduit, OperationStore, TransportMode, accept_transport,
    handshake_as_acceptor, handshake_as_initiator, initiate_transport,
};

use super::{
    CloseRequest, ConduitRecoverer, ConnectionAcceptor, OpenRequest, Session, SessionError,
    SessionHandle, SessionKeepaliveConfig,
};
use crate::{Driver, DriverCaller, DriverReplySink};

/// A pinned, boxed session future. On non-WASM this is `Send + 'static`;
/// on WASM it's `'static` only (no `Send` requirement).
#[cfg(not(target_arch = "wasm32"))]
pub type BoxSessionFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type BoxSessionFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

#[cfg(not(target_arch = "wasm32"))]
type SpawnFn = Box<dyn FnOnce(BoxSessionFuture) + Send + 'static>;
#[cfg(target_arch = "wasm32")]
type SpawnFn = Box<dyn FnOnce(BoxSessionFuture) + 'static>;

#[cfg(not(target_arch = "wasm32"))]
fn default_spawn_fn() -> SpawnFn {
    Box::new(|fut| {
        tokio::spawn(fut);
    })
}

#[cfg(target_arch = "wasm32")]
fn default_spawn_fn() -> SpawnFn {
    Box::new(|fut| {
        wasm_bindgen_futures::spawn_local(fut);
    })
}

// r[impl rpc.session-setup]
// r[impl session.role]
pub fn initiator_conduit<I: IntoConduit>(
    into_conduit: I,
    handshake_result: HandshakeResult,
) -> SessionInitiatorBuilder<'static, I::Conduit> {
    SessionInitiatorBuilder::new(into_conduit.into_conduit(), handshake_result)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn initiator<S>(source: S, mode: TransportMode) -> SessionSourceInitiatorBuilder<'static, S>
where
    S: LinkSource,
{
    SessionSourceInitiatorBuilder::new(source, mode)
}

pub fn acceptor_conduit<I: IntoConduit>(
    into_conduit: I,
    handshake_result: HandshakeResult,
) -> SessionAcceptorBuilder<'static, I::Conduit> {
    SessionAcceptorBuilder::new(into_conduit.into_conduit(), handshake_result)
}

/// Convenience: perform CBOR handshake as initiator on a raw link, then return
/// a builder with the conduit ready to go.
pub async fn initiator_on_link<L: Link>(
    link: L,
    settings: ConnectionSettings,
) -> Result<
    SessionInitiatorBuilder<'static, BareConduit<MessageFamily, SplitLink<L::Tx, L::Rx>>>,
    SessionError,
>
where
    L::Tx: MaybeSend + MaybeSync + 'static,
    L::Rx: MaybeSend + 'static,
{
    let (tx, mut rx) = link.split();
    let handshake_result = handshake_as_initiator(&tx, &mut rx, settings, true, None)
        .await
        .map_err(session_error_from_handshake)?;
    let message_plan =
        crate::MessagePlan::from_handshake(&handshake_result).map_err(SessionError::Protocol)?;
    Ok(SessionInitiatorBuilder::new(
        BareConduit::with_message_plan(SplitLink { tx, rx }, message_plan),
        handshake_result,
    ))
}

/// Convenience: perform CBOR handshake as acceptor on a raw link, then return
/// a builder with the conduit ready to go.
pub async fn acceptor_on_link<L: Link>(
    link: L,
    settings: ConnectionSettings,
) -> Result<
    SessionAcceptorBuilder<'static, BareConduit<MessageFamily, SplitLink<L::Tx, L::Rx>>>,
    SessionError,
>
where
    L::Tx: MaybeSend + MaybeSync + 'static,
    L::Rx: MaybeSend + 'static,
{
    let (tx, mut rx) = link.split();
    let handshake_result = handshake_as_acceptor(&tx, &mut rx, settings, true, false, None)
        .await
        .map_err(session_error_from_handshake)?;
    let message_plan =
        crate::MessagePlan::from_handshake(&handshake_result).map_err(SessionError::Protocol)?;
    Ok(SessionAcceptorBuilder::new(
        BareConduit::with_message_plan(SplitLink { tx, rx }, message_plan),
        handshake_result,
    ))
}

pub fn initiator_on<L: Link>(
    link: L,
    mode: TransportMode,
) -> SessionTransportInitiatorBuilder<'static, L> {
    SessionTransportInitiatorBuilder::new(link, mode)
}

pub fn initiator_transport<L: Link>(
    link: L,
    mode: TransportMode,
) -> SessionTransportInitiatorBuilder<'static, L> {
    initiator_on(link, mode)
}

pub fn acceptor_on<L: Link>(link: L) -> SessionTransportAcceptorBuilder<'static, L> {
    SessionTransportAcceptorBuilder::new(link)
}

pub fn acceptor_transport<L: Link>(link: L) -> SessionTransportAcceptorBuilder<'static, L> {
    acceptor_on(link)
}

#[derive(Clone, Default)]
pub struct SessionRegistry {
    inner: Arc<Mutex<HashMap<SessionResumeKey, SessionHandle>>>,
}

impl SessionRegistry {
    fn get(&self, key: &SessionResumeKey) -> Option<SessionHandle> {
        self.inner
            .lock()
            .expect("session registry poisoned")
            .get(key)
            .cloned()
    }

    fn insert(&self, key: SessionResumeKey, handle: SessionHandle) {
        self.inner
            .lock()
            .expect("session registry poisoned")
            .insert(key, handle);
    }

    fn remove(&self, key: &SessionResumeKey) {
        self.inner
            .lock()
            .expect("session registry poisoned")
            .remove(key);
    }
}

pub enum SessionAcceptOutcome<Client> {
    Established(Client, SessionHandle),
    Resumed,
}

pub struct SessionInitiatorBuilder<'a, C> {
    conduit: C,
    handshake_result: HandshakeResult,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
    recoverer: Option<Box<dyn ConduitRecoverer>>,
    operation_store: Option<Arc<dyn OperationStore>>,
    spawn_fn: SpawnFn,
}

impl<'a, C> SessionInitiatorBuilder<'a, C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        Self {
            conduit,
            handshake_result,
            root_settings,
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: false,
            recoverer: None,
            operation_store: None,
            spawn_fn: default_spawn_fn(),
        }
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.on_connection = Some(Box::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.keepalive = Some(keepalive);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.resumable = true;
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.operation_store = Some(operation_store);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    pub async fn establish<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        for<'p> <C::Tx as ConduitTx>::Permit<'p>: MaybeSend,
        C::Rx: MaybeSend + 'static,
    {
        let Self {
            conduit,
            handshake_result,
            root_settings,
            metadata: _metadata,
            on_connection,
            keepalive,
            resumable,
            recoverer,
            operation_store,
            spawn_fn,
        } = self;
        validate_negotiated_root_settings(&root_settings, &handshake_result)?;
        let (tx, rx) = conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (resume_tx, resume_rx) = mpsc::channel::<super::ResumeRequest>("session.resume", 1);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let mut session = Session::pre_handshake(
            tx,
            rx,
            on_connection,
            open_rx,
            close_rx,
            resume_rx,
            control_tx.clone(),
            control_rx,
            keepalive,
            resumable,
            recoverer,
        );
        let handle = session.establish_from_handshake(handshake_result)?;
        let resume_key = session.resume_key();
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
            resume_tx,
            control_tx,
            resume_key,
        };
        let mut driver = match operation_store {
            Some(operation_store) => Driver::with_operation_store(handle, handler, operation_store),
            None => Driver::new(handle, handler),
        };
        let client = Client::from(driver.caller());
        (spawn_fn)(Box::pin(async move { session.run().await }));
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        Ok((client, session_handle))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct SessionSourceInitiatorBuilder<'a, S> {
    source: S,
    mode: TransportMode,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
    operation_store: Option<Arc<dyn OperationStore>>,
    spawn_fn: SpawnFn,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a, S> SessionSourceInitiatorBuilder<'a, S> {
    fn new(source: S, mode: TransportMode) -> Self {
        Self {
            source,
            mode,
            root_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: true,
            operation_store: None,
            spawn_fn: default_spawn_fn(),
        }
    }

    pub fn parity(mut self, parity: Parity) -> Self {
        self.root_settings.parity = parity;
        self
    }

    pub fn root_settings(mut self, settings: ConnectionSettings) -> Self {
        self.root_settings = settings;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: u32) -> Self {
        self.root_settings.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn metadata(mut self, metadata: Metadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.on_connection = Some(Box::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.keepalive = Some(keepalive);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.resumable = true;
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.operation_store = Some(operation_store);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    pub async fn establish<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        S: LinkSource,
        S::Link: Link + Send + 'static,
        <S::Link as Link>::Tx: MaybeSend + MaybeSync + Send + 'static,
        <<S::Link as Link>::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        <S::Link as Link>::Rx: MaybeSend + Send + 'static,
    {
        let Self {
            mut source,
            mode,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            operation_store,
            spawn_fn,
        } = self;

        match mode {
            TransportMode::Bare => {
                let attachment = source.next_link().await.map_err(SessionError::Io)?;
                let mut link = initiate_transport(attachment.into_link(), TransportMode::Bare)
                    .await
                    .map_err(session_error_from_transport)?;
                let handshake_result = handshake_as_initiator(
                    &link.tx,
                    &mut link.rx,
                    root_settings.clone(),
                    true,
                    None,
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let builder = SessionInitiatorBuilder::new(
                    BareConduit::with_message_plan(link, message_plan),
                    handshake_result,
                );
                let recoverer = Box::new(BareSourceRecoverer {
                    source,
                    settings: root_settings.clone(),
                });
                SessionTransportInitiatorBuilder::<S::Link>::apply_common_parts(
                    builder,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    Some(recoverer),
                    operation_store,
                    spawn_fn,
                )
                .establish(handler)
                .await
            }
            TransportMode::Stable => {
                // Get first link and do transport + CBOR handshake before
                // handing to the stable conduit.
                let attachment = source.next_link().await.map_err(SessionError::Io)?;
                let mut link = initiate_transport(attachment.into_link(), TransportMode::Stable)
                    .await
                    .map_err(session_error_from_transport)?;
                let handshake_result = handshake_as_initiator(
                    &link.tx,
                    &mut link.rx,
                    root_settings.clone(),
                    true,
                    None,
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let conduit = StableConduit::<MessageFamily, _>::with_first_link(
                    link.tx,
                    link.rx,
                    None, // initiator side — no ClientHello
                    TransportedLinkSource {
                        source,
                        mode: TransportMode::Stable,
                    },
                )
                .await
                .map_err(|error| {
                    SessionError::Protocol(format!("stable conduit setup failed: {error}"))
                })?
                .with_message_plan(message_plan);
                let builder = SessionInitiatorBuilder::new(conduit, handshake_result);
                SessionTransportInitiatorBuilder::<S::Link>::apply_common_parts(
                    builder,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    None,
                    operation_store,
                    spawn_fn,
                )
                .establish(handler)
                .await
            }
        }
    }
}

pub struct SessionTransportInitiatorBuilder<'a, L> {
    link: L,
    mode: TransportMode,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
    operation_store: Option<Arc<dyn OperationStore>>,
    spawn_fn: SpawnFn,
}

impl<'a, L> SessionTransportInitiatorBuilder<'a, L> {
    fn new(link: L, mode: TransportMode) -> Self {
        Self {
            link,
            mode,
            root_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: false,
            operation_store: None,
            spawn_fn: default_spawn_fn(),
        }
    }

    pub fn parity(mut self, parity: Parity) -> Self {
        self.root_settings.parity = parity;
        self
    }

    pub fn root_settings(mut self, settings: ConnectionSettings) -> Self {
        self.root_settings = settings;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: u32) -> Self {
        self.root_settings.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn metadata(mut self, metadata: Metadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.on_connection = Some(Box::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.keepalive = Some(keepalive);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.resumable = true;
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.operation_store = Some(operation_store);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn establish<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            mode,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            operation_store,
            spawn_fn,
        } = self;
        match mode {
            TransportMode::Bare => {
                let link = initiate_transport(link, TransportMode::Bare)
                    .await
                    .map_err(session_error_from_transport)?;
                Self::finish_with_bare_parts(
                    link,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    operation_store,
                    spawn_fn,
                    handler,
                )
                .await
            }
            TransportMode::Stable => {
                let link = initiate_transport(link, TransportMode::Stable)
                    .await
                    .map_err(session_error_from_transport)?;
                Self::finish_with_stable_parts(
                    link,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    operation_store,
                    spawn_fn,
                    handler,
                )
                .await
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn establish<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            mode,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            operation_store,
            spawn_fn,
        } = self;
        match mode {
            TransportMode::Bare => {
                let link = initiate_transport(link, TransportMode::Bare)
                    .await
                    .map_err(session_error_from_transport)?;
                Self::finish_with_bare_parts(
                    link,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    operation_store,
                    spawn_fn,
                    handler,
                )
                .await
            }
            TransportMode::Stable => Err(SessionError::Protocol(
                "stable conduit transport selection is unsupported on wasm".into(),
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_with_bare_parts<Client: From<DriverCaller>>(
        mut link: SplitLink<L::Tx, L::Rx>,
        root_settings: ConnectionSettings,
        metadata: Metadata<'a>,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        operation_store: Option<Arc<dyn OperationStore>>,
        spawn_fn: SpawnFn,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + 'static,
    {
        let handshake_result =
            handshake_as_initiator(&link.tx, &mut link.rx, root_settings.clone(), true, None)
                .await
                .map_err(session_error_from_handshake)?;
        let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
            .map_err(SessionError::Protocol)?;
        let builder = SessionInitiatorBuilder::new(
            BareConduit::with_message_plan(link, message_plan),
            handshake_result,
        );
        Self::apply_common_parts(
            builder,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            None,
            operation_store,
            spawn_fn,
        )
        .establish(handler)
        .await
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    async fn finish_with_stable_parts<Client: From<DriverCaller>>(
        mut link: SplitLink<L::Tx, L::Rx>,
        root_settings: ConnectionSettings,
        metadata: Metadata<'a>,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        operation_store: Option<Arc<dyn OperationStore>>,
        spawn_fn: SpawnFn,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + Send + 'static,
        for<'p> <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + Send + 'static,
    {
        let handshake_result =
            handshake_as_initiator(&link.tx, &mut link.rx, root_settings.clone(), true, None)
                .await
                .map_err(session_error_from_handshake)?;
        let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
            .map_err(SessionError::Protocol)?;
        let conduit = StableConduit::<MessageFamily, _>::with_first_link(
            link.tx,
            link.rx,
            None,
            crate::stable_conduit::exhausted_source::<SplitLink<L::Tx, L::Rx>>(),
        )
        .await
        .map_err(|e| SessionError::Protocol(format!("stable conduit setup failed: {e}")))?
        .with_message_plan(message_plan);
        let builder = SessionInitiatorBuilder::new(conduit, handshake_result);
        Self::apply_common_parts(
            builder,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            None,
            operation_store,
            spawn_fn,
        )
        .establish(handler)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn apply_common_parts<C>(
        mut builder: SessionInitiatorBuilder<'a, C>,
        root_settings: ConnectionSettings,
        metadata: Metadata<'a>,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        recoverer: Option<Box<dyn ConduitRecoverer>>,
        operation_store: Option<Arc<dyn OperationStore>>,
        spawn_fn: SpawnFn,
    ) -> SessionInitiatorBuilder<'a, C> {
        builder.root_settings = root_settings;
        builder.metadata = metadata;
        builder.on_connection = on_connection;
        builder.keepalive = keepalive;
        builder.resumable = resumable;
        builder.recoverer = recoverer;
        builder.operation_store = operation_store;
        builder.spawn_fn = spawn_fn;
        builder
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct BareSourceRecoverer<S> {
    source: S,
    settings: ConnectionSettings,
}

#[cfg(not(target_arch = "wasm32"))]
impl<S> ConduitRecoverer for BareSourceRecoverer<S>
where
    S: LinkSource,
    S::Link: Link + Send + 'static,
    <S::Link as Link>::Tx: MaybeSend + MaybeSync + Send + 'static,
    <<S::Link as Link>::Tx as roam_types::LinkTx>::Permit: MaybeSend,
    <S::Link as Link>::Rx: MaybeSend + Send + 'static,
{
    fn next_conduit<'a>(
        &'a mut self,
        resume_key: Option<&'a SessionResumeKey>,
    ) -> roam_types::BoxFut<'a, Result<super::RecoveredConduit, SessionError>> {
        Box::pin(async move {
            let attachment = self.source.next_link().await.map_err(SessionError::Io)?;
            let mut link = initiate_transport(attachment.into_link(), TransportMode::Bare)
                .await
                .map_err(session_error_from_transport)?;
            let handshake_result = handshake_as_initiator(
                &link.tx,
                &mut link.rx,
                self.settings.clone(),
                true,
                resume_key,
            )
            .await
            .map_err(session_error_from_handshake)?;
            let conduit = BareConduit::<MessageFamily, _>::new(link);
            let (tx, rx) = conduit.split();
            Ok(super::RecoveredConduit {
                tx: Arc::new(tx) as Arc<dyn crate::DynConduitTx>,
                rx: Box::new(rx) as Box<dyn crate::DynConduitRx>,
                handshake: handshake_result,
            })
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct TransportedLinkSource<S> {
    source: S,
    mode: TransportMode,
}

#[cfg(not(target_arch = "wasm32"))]
impl<S> LinkSource for TransportedLinkSource<S>
where
    S: LinkSource,
    S::Link: Link + Send + 'static,
    <S::Link as Link>::Tx: MaybeSend + MaybeSync + Send + 'static,
    <<S::Link as Link>::Tx as roam_types::LinkTx>::Permit: MaybeSend,
    <S::Link as Link>::Rx: MaybeSend + Send + 'static,
{
    type Link = SplitLink<<S::Link as Link>::Tx, <S::Link as Link>::Rx>;

    async fn next_link(&mut self) -> std::io::Result<Attachment<Self::Link>> {
        let attachment = self.source.next_link().await?;
        let link = initiate_transport(attachment.into_link(), self.mode)
            .await
            .map_err(std::io::Error::other)?;
        Ok(Attachment::initiator(link))
    }
}

pub struct SessionAcceptorBuilder<'a, C> {
    conduit: C,
    handshake_result: HandshakeResult,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
    session_registry: Option<SessionRegistry>,
    operation_store: Option<Arc<dyn OperationStore>>,
    spawn_fn: SpawnFn,
}

impl<'a, C> SessionAcceptorBuilder<'a, C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        Self {
            conduit,
            handshake_result,
            root_settings,
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: false,
            session_registry: None,
            operation_store: None,
            spawn_fn: default_spawn_fn(),
        }
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.on_connection = Some(Box::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.keepalive = Some(keepalive);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.resumable = true;
        self
    }

    pub fn session_registry(mut self, session_registry: SessionRegistry) -> Self {
        self.session_registry = Some(session_registry);
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.operation_store = Some(operation_store);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    #[moire::instrument]
    pub async fn establish<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        for<'p> <C::Tx as ConduitTx>::Permit<'p>: MaybeSend,
        C::Rx: MaybeSend + 'static,
    {
        let Self {
            conduit,
            handshake_result,
            root_settings,
            metadata: _metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry,
            operation_store,
            spawn_fn,
        } = self;
        validate_negotiated_root_settings(&root_settings, &handshake_result)?;
        let (tx, rx) = conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (resume_tx, resume_rx) = mpsc::channel::<super::ResumeRequest>("session.resume", 1);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let mut session = Session::pre_handshake(
            tx,
            rx,
            on_connection,
            open_rx,
            close_rx,
            resume_rx,
            control_tx.clone(),
            control_rx,
            keepalive,
            resumable,
            None,
        );
        let handle = session.establish_from_handshake(handshake_result)?;
        let resume_key = session.resume_key();
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
            resume_tx,
            control_tx,
            resume_key,
        };
        if let (Some(registry), Some(key)) = (&session_registry, resume_key) {
            registry.insert(key, session_handle.clone());
        }
        let mut driver = match operation_store {
            Some(operation_store) => Driver::with_operation_store(handle, handler, operation_store),
            None => Driver::new(handle, handler),
        };
        let client = Client::from(driver.caller());
        (spawn_fn)(Box::pin(async move { session.run().await }));
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        Ok((client, session_handle))
    }

    #[moire::instrument]
    pub async fn establish_or_resume<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<SessionAcceptOutcome<Client>, SessionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        for<'p> <C::Tx as ConduitTx>::Permit<'p>: MaybeSend,
        C::Rx: MaybeSend + 'static,
    {
        // With the CBOR handshake, resume detection happens at the link level
        // before the conduit is created. If the peer sent a resume key in the Hello
        // that matches a known session, we resume. Otherwise, we establish.
        if let (Some(registry), Some(resume_key)) = (
            &self.session_registry,
            self.handshake_result.peer_resume_key,
        ) {
            if let Some(handle) = registry.get(&resume_key) {
                let (tx, rx) = self.conduit.split();
                if let Err(error) = handle
                    .resume_parts(Arc::new(tx), Box::new(rx), self.handshake_result)
                    .await
                {
                    registry.remove(&resume_key);
                    return Err(error);
                }
                return Ok(SessionAcceptOutcome::Resumed);
            }
            return Err(SessionError::Protocol("unknown session resume key".into()));
        }

        let (client, session_handle) = self.establish(handler).await?;
        Ok(SessionAcceptOutcome::Established(client, session_handle))
    }
}

pub struct SessionTransportAcceptorBuilder<'a, L: Link> {
    link: L,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
    session_registry: Option<SessionRegistry>,
    operation_store: Option<Arc<dyn OperationStore>>,
    spawn_fn: SpawnFn,
}

impl<'a, L: Link> SessionTransportAcceptorBuilder<'a, L> {
    fn new(link: L) -> Self {
        Self {
            link,
            root_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: true,
            session_registry: None,
            operation_store: None,
            spawn_fn: default_spawn_fn(),
        }
    }

    pub fn root_settings(mut self, settings: ConnectionSettings) -> Self {
        self.root_settings = settings;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: u32) -> Self {
        self.root_settings.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn metadata(mut self, metadata: Metadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.on_connection = Some(Box::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.keepalive = Some(keepalive);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.resumable = true;
        self
    }

    pub fn session_registry(mut self, session_registry: SessionRegistry) -> Self {
        self.session_registry = Some(session_registry);
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.operation_store = Some(operation_store);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.spawn_fn = Box::new(f);
        self
    }

    #[moire::instrument]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn establish<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry,
            operation_store,
            spawn_fn,
        } = self;
        let (mode, mut link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let handshake_result = handshake_as_acceptor(
                    &link.tx,
                    &mut link.rx,
                    root_settings.clone(),
                    true,
                    resumable,
                    None,
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let builder = SessionAcceptorBuilder::new(
                    BareConduit::with_message_plan(link, message_plan),
                    handshake_result,
                );
                Self::apply_common_parts(
                    builder,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    session_registry,
                    operation_store,
                    spawn_fn,
                )
                .establish(handler)
                .await
            }
            TransportMode::Stable => {
                Self::finish_with_stable_parts(
                    link,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    session_registry,
                    operation_store,
                    spawn_fn,
                    handler,
                )
                .await
            }
        }
    }

    #[moire::instrument]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn establish_or_resume<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<SessionAcceptOutcome<Client>, SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry,
            operation_store,
            spawn_fn,
        } = self;
        let (mode, mut link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let handshake_result = handshake_as_acceptor(
                    &link.tx,
                    &mut link.rx,
                    root_settings.clone(),
                    true,
                    resumable,
                    None,
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let builder = SessionAcceptorBuilder::new(
                    BareConduit::with_message_plan(link, message_plan),
                    handshake_result,
                );
                Self::apply_common_parts(
                    builder,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    session_registry,
                    operation_store,
                    spawn_fn,
                )
                .establish_or_resume(handler)
                .await
            }
            TransportMode::Stable => Self::finish_with_stable_parts(
                link,
                root_settings,
                metadata,
                on_connection,
                keepalive,
                resumable,
                session_registry,
                operation_store,
                spawn_fn,
                handler,
            )
            .await
            .map(|(client, handle)| SessionAcceptOutcome::Established(client, handle)),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn establish<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry,
            operation_store,
            spawn_fn,
        } = self;
        let (mode, mut link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let handshake_result = handshake_as_acceptor(
                    &link.tx,
                    &mut link.rx,
                    root_settings.clone(),
                    true,
                    resumable,
                    None,
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let builder = SessionAcceptorBuilder::new(
                    BareConduit::with_message_plan(link, message_plan),
                    handshake_result,
                );
                Self::apply_common_parts(
                    builder,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    session_registry,
                    operation_store,
                    spawn_fn,
                )
                .establish(handler)
                .await
            }
            TransportMode::Stable => Err(SessionError::Protocol(
                "stable conduit transport selection is unsupported on wasm".into(),
            )),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn establish_or_resume<Client: From<DriverCaller>>(
        self,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<SessionAcceptOutcome<Client>, SessionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry,
            operation_store,
            spawn_fn,
        } = self;
        let (mode, mut link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let handshake_result = handshake_as_acceptor(
                    &link.tx,
                    &mut link.rx,
                    root_settings.clone(),
                    true,
                    resumable,
                    None,
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let builder = SessionAcceptorBuilder::new(
                    BareConduit::with_message_plan(link, message_plan),
                    handshake_result,
                );
                Self::apply_common_parts(
                    builder,
                    root_settings,
                    metadata,
                    on_connection,
                    keepalive,
                    resumable,
                    session_registry,
                    operation_store,
                    spawn_fn,
                )
                .establish_or_resume(handler)
                .await
            }
            TransportMode::Stable => Err(SessionError::Protocol(
                "stable conduit transport selection is unsupported on wasm".into(),
            )),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    async fn finish_with_stable_parts<Client: From<DriverCaller>>(
        mut link: SplitLink<L::Tx, L::Rx>,
        root_settings: ConnectionSettings,
        metadata: Metadata<'a>,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        session_registry: Option<SessionRegistry>,
        operation_store: Option<Arc<dyn OperationStore>>,
        spawn_fn: SpawnFn,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle), SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + Send + 'static,
        <L::Tx as roam_types::LinkTx>::Permit: MaybeSend,
        L::Rx: MaybeSend + Send + 'static,
    {
        let handshake_result = handshake_as_acceptor(
            &link.tx,
            &mut link.rx,
            root_settings.clone(),
            true,
            resumable,
            None,
        )
        .await
        .map_err(session_error_from_handshake)?;
        let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
            .map_err(SessionError::Protocol)?;
        // Read the stable conduit's ClientHello — the initiator sends it
        // after the CBOR session handshake completes.
        let client_hello = crate::stable_conduit::recv_client_hello(&mut link.rx)
            .await
            .map_err(|e| SessionError::Protocol(format!("stable conduit setup failed: {e}")))?;
        let conduit = StableConduit::<MessageFamily, _>::with_first_link(
            link.tx,
            link.rx,
            Some(client_hello),
            crate::stable_conduit::exhausted_source::<SplitLink<L::Tx, L::Rx>>(),
        )
        .await
        .map_err(|e| SessionError::Protocol(format!("stable conduit setup failed: {e}")))?
        .with_message_plan(message_plan);
        let builder = SessionAcceptorBuilder::new(conduit, handshake_result);
        Self::apply_common_parts(
            builder,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry,
            operation_store,
            spawn_fn,
        )
        .establish(handler)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn apply_common_parts<C>(
        mut builder: SessionAcceptorBuilder<'a, C>,
        root_settings: ConnectionSettings,
        metadata: Metadata<'a>,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        session_registry: Option<SessionRegistry>,
        operation_store: Option<Arc<dyn OperationStore>>,
        spawn_fn: SpawnFn,
    ) -> SessionAcceptorBuilder<'a, C> {
        builder.root_settings = root_settings;
        builder.metadata = metadata;
        builder.on_connection = on_connection;
        builder.keepalive = keepalive;
        builder.resumable = resumable;
        builder.session_registry = session_registry;
        builder.operation_store = operation_store;
        builder.spawn_fn = spawn_fn;
        builder
    }
}

fn validate_negotiated_root_settings(
    expected_root_settings: &ConnectionSettings,
    handshake_result: &HandshakeResult,
) -> Result<(), SessionError> {
    if handshake_result.our_settings != *expected_root_settings {
        return Err(SessionError::Protocol(
            "negotiated root settings do not match builder settings".into(),
        ));
    }
    Ok(())
}

fn session_error_from_handshake(error: crate::HandshakeError) -> SessionError {
    match error {
        crate::HandshakeError::Io(io) => SessionError::Io(io),
        crate::HandshakeError::PeerClosed => {
            SessionError::Protocol("peer closed during handshake".into())
        }
        crate::HandshakeError::NotResumable => SessionError::NotResumable,
        other => SessionError::Protocol(other.to_string()),
    }
}

fn session_error_from_transport(error: crate::TransportPrologueError) -> SessionError {
    match error {
        crate::TransportPrologueError::Io(io) => SessionError::Io(io),
        crate::TransportPrologueError::LinkDead => {
            SessionError::Protocol("link closed during transport prologue".into())
        }
        crate::TransportPrologueError::Protocol(message) => SessionError::Protocol(message),
        crate::TransportPrologueError::Rejected(reason) => {
            SessionError::Protocol(format!("transport rejected: {reason}"))
        }
    }
}
