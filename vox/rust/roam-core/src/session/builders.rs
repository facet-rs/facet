use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use moire::sync::mpsc;
use roam_types::{
    Conduit, ConduitRx, ConduitTx, ConnectionSettings, Handler, Link, MaybeSend, MaybeSync,
    Message, MessageFamily, MessagePayload, Metadata, Parity, SelfRef, SessionResumeKey, SplitLink,
    metadata_session_resume_key,
};

#[cfg(not(target_arch = "wasm32"))]
use crate::{
    Attachment, LinkSource, StableConduit, prepare_acceptor_attachment, single_attachment_source,
    single_link_source,
};
use crate::{
    BareConduit, IntoConduit, OperationStore, TransportMode, accept_transport, initiate_transport,
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
) -> SessionInitiatorBuilder<'static, I::Conduit> {
    SessionInitiatorBuilder::new(into_conduit.into_conduit())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn initiator<S>(source: S, mode: TransportMode) -> SessionSourceInitiatorBuilder<'static, S>
where
    S: LinkSource,
{
    SessionSourceInitiatorBuilder::new(source, mode)
}

// r[impl session.role]
pub fn acceptor<I: IntoConduit>(into_conduit: I) -> SessionAcceptorBuilder<'static, I::Conduit> {
    SessionAcceptorBuilder::new(into_conduit.into_conduit())
}

pub fn acceptor_conduit<I: IntoConduit>(
    into_conduit: I,
) -> SessionAcceptorBuilder<'static, I::Conduit> {
    acceptor(into_conduit)
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

struct PrefetchedConduitRx<Rx> {
    first: Option<SelfRef<Message<'static>>>,
    inner: Rx,
}

impl<Rx> PrefetchedConduitRx<Rx> {
    fn new(first: SelfRef<Message<'static>>, inner: Rx) -> Self {
        Self {
            first: Some(first),
            inner,
        }
    }
}

impl<Rx> ConduitRx for PrefetchedConduitRx<Rx>
where
    Rx: ConduitRx<Msg = MessageFamily> + MaybeSend,
{
    type Msg = MessageFamily;
    type Error = Rx::Error;

    fn recv(
        &mut self,
    ) -> impl Future<Output = Result<Option<SelfRef<Message<'static>>>, Self::Error>> + MaybeSend + '_
    {
        async move {
            if let Some(first) = self.first.take() {
                return Ok(Some(first));
            }
            self.inner.recv().await
        }
    }
}

fn resume_key_from_first_message(
    first: &SelfRef<Message<'static>>,
) -> Result<Option<SessionResumeKey>, SessionError> {
    match &first.payload {
        MessagePayload::Hello(hello) => Ok(metadata_session_resume_key(&hello.metadata)),
        MessagePayload::ProtocolError(error) => {
            Err(SessionError::Protocol(error.description.to_owned()))
        }
        _ => Err(SessionError::Protocol("expected Hello".into())),
    }
}

pub struct SessionInitiatorBuilder<'a, C> {
    conduit: C,
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
    fn new(conduit: C) -> Self {
        Self {
            conduit,
            root_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: false,
            recoverer: None,
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
        let (tx, rx) = self.conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (resume_tx, resume_rx) = mpsc::channel::<super::ResumeRequest>("session.resume", 1);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let mut session = Session::pre_handshake(
            tx,
            rx,
            self.on_connection,
            open_rx,
            close_rx,
            resume_rx,
            control_tx.clone(),
            control_rx,
            self.keepalive,
            self.resumable,
            self.recoverer,
        );
        let handle = session
            .establish_as_initiator(self.root_settings, self.metadata)
            .await?;
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
            resume_tx,
            control_tx,
        };
        let mut driver = match self.operation_store {
            Some(operation_store) => Driver::with_operation_store(handle, handler, operation_store),
            None => Driver::new(handle, handler),
        };
        let client = Client::from(driver.caller());
        (self.spawn_fn)(Box::pin(async move { session.run().await }));
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
                let link = initiate_transport(attachment.into_link(), TransportMode::Bare)
                    .await
                    .map_err(session_error_from_transport)?;
                let builder =
                    SessionInitiatorBuilder::new(BareConduit::<MessageFamily, _>::new(link));
                let recoverer = Box::new(BareSourceRecoverer { source });
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
                let conduit = StableConduit::<MessageFamily, _>::new(TransportedLinkSource {
                    source,
                    mode: TransportMode::Stable,
                })
                .await
                .map_err(|error| {
                    SessionError::Protocol(format!("stable conduit setup failed: {error}"))
                })?;
                let builder = SessionInitiatorBuilder::new(conduit);
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

    async fn finish_with_bare_parts<Client: From<DriverCaller>>(
        link: SplitLink<L::Tx, L::Rx>,
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
        let builder = SessionInitiatorBuilder::new(BareConduit::<MessageFamily, _>::new(link));
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
    async fn finish_with_stable_parts<Client: From<DriverCaller>>(
        link: L,
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
        let link = initiate_transport(link, TransportMode::Stable)
            .await
            .map_err(session_error_from_transport)?;
        let conduit = StableConduit::<MessageFamily, _>::new(single_link_source(link))
            .await
            .map_err(|error| {
                SessionError::Protocol(format!("stable conduit setup failed: {error}"))
            })?;
        let builder = SessionInitiatorBuilder::new(conduit);
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
    fn next_conduit<'b>(
        &'b mut self,
    ) -> super::BoxFuture<
        'b,
        Result<
            (
                std::sync::Arc<dyn crate::DynConduitTx>,
                Box<dyn crate::DynConduitRx>,
            ),
            SessionError,
        >,
    > {
        Box::pin(async move {
            let attachment = self.source.next_link().await.map_err(SessionError::Io)?;
            let link = initiate_transport(attachment.into_link(), TransportMode::Bare)
                .await
                .map_err(session_error_from_transport)?;
            let conduit = BareConduit::<MessageFamily, _>::new(link);
            let (tx, rx) = conduit.split();
            Ok((
                std::sync::Arc::new(tx) as std::sync::Arc<dyn crate::DynConduitTx>,
                Box::new(rx) as Box<dyn crate::DynConduitRx>,
            ))
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
    fn new(conduit: C) -> Self {
        Self {
            conduit,
            root_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: false,
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
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry: _session_registry,
            operation_store,
            spawn_fn,
        } = self;
        Self::establish_from_parts(
            conduit,
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
        let Self {
            conduit,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            session_registry,
            operation_store,
            spawn_fn,
        } = self;

        let Some(session_registry) = session_registry else {
            let (client, handle) = Self::establish_from_parts(
                conduit,
                root_settings,
                metadata,
                on_connection,
                keepalive,
                resumable,
                operation_store,
                spawn_fn,
                handler,
            )
            .await?;
            return Ok(SessionAcceptOutcome::Established(client, handle));
        };

        let (tx, mut rx) = conduit.split();
        let Some(first) = rx
            .recv()
            .await
            .map_err(|error| SessionError::Protocol(error.to_string()))?
        else {
            return Err(SessionError::Protocol(
                "peer closed during handshake".into(),
            ));
        };

        if let Some(resume_key) = resume_key_from_first_message(&first)? {
            if let Some(handle) = session_registry.get(&resume_key) {
                if let Err(error) = handle
                    .resume_parts(Arc::new(tx), Box::new(PrefetchedConduitRx::new(first, rx)))
                    .await
                {
                    session_registry.remove(&resume_key);
                    return Err(error);
                }
                return Ok(SessionAcceptOutcome::Resumed);
            }
            return Err(SessionError::Protocol("unknown session resume key".into()));
        }

        let (client, handle, resume_key) = Self::establish_from_parts_with_prefetched_hello(
            tx,
            PrefetchedConduitRx::new(first, rx),
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            operation_store,
            spawn_fn,
            handler,
        )
        .await?;
        if let Some(resume_key) = resume_key {
            session_registry.insert(resume_key, handle.clone());
        }
        Ok(SessionAcceptOutcome::Established(client, handle))
    }

    async fn establish_from_parts<Client: From<DriverCaller>, Tx, Rx>(
        conduit: impl Conduit<Msg = MessageFamily, Tx = Tx, Rx = Rx> + 'static,
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
        Tx: ConduitTx<Msg = MessageFamily> + MaybeSend + MaybeSync + 'static,
        for<'p> <Tx as ConduitTx>::Permit<'p>: MaybeSend,
        Rx: ConduitRx<Msg = MessageFamily> + MaybeSend + 'static,
    {
        let (tx, rx) = conduit.split();
        let (client, handle, _resume_key) = Self::establish_from_parts_with_prefetched_hello(
            tx,
            rx,
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
            operation_store,
            spawn_fn,
            handler,
        )
        .await?;
        Ok((client, handle))
    }

    async fn establish_from_parts_with_prefetched_hello<Client: From<DriverCaller>, Tx, Rx>(
        tx: Tx,
        rx: Rx,
        root_settings: ConnectionSettings,
        metadata: Metadata<'a>,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        operation_store: Option<Arc<dyn OperationStore>>,
        spawn_fn: SpawnFn,
        handler: impl Handler<DriverReplySink> + 'static,
    ) -> Result<(Client, SessionHandle, Option<SessionResumeKey>), SessionError>
    where
        Tx: ConduitTx<Msg = MessageFamily> + MaybeSend + MaybeSync + 'static,
        for<'p> <Tx as ConduitTx>::Permit<'p>: MaybeSend,
        Rx: ConduitRx<Msg = MessageFamily> + MaybeSend + 'static,
    {
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
        let handle = session
            .establish_as_acceptor(root_settings, metadata)
            .await?;
        let resume_key = session.resume_key();
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
            resume_tx,
            control_tx,
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
        Ok((client, session_handle, resume_key))
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
        let (mode, link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let builder =
                    SessionAcceptorBuilder::new(BareConduit::<MessageFamily, _>::new(link));
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
        let (mode, link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let builder =
                    SessionAcceptorBuilder::new(BareConduit::<MessageFamily, _>::new(link));
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
        let (mode, link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let builder =
                    SessionAcceptorBuilder::new(BareConduit::<MessageFamily, _>::new(link));
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
        let (mode, link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let builder =
                    SessionAcceptorBuilder::new(BareConduit::<MessageFamily, _>::new(link));
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
    async fn finish_with_stable_parts<Client: From<DriverCaller>>(
        link: SplitLink<L::Tx, L::Rx>,
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
        let attachment = prepare_acceptor_attachment(link).await.map_err(|error| {
            SessionError::Protocol(format!("stable acceptor attachment failed: {error}"))
        })?;
        let conduit = StableConduit::<MessageFamily, _>::new(single_attachment_source(attachment))
            .await
            .map_err(|error| {
                SessionError::Protocol(format!("stable conduit setup failed: {error}"))
            })?;
        let builder = SessionAcceptorBuilder::new(conduit);
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
