use std::{future::Future, pin::Pin, sync::Arc};

use moire::sync::mpsc;
use vox_types::{
    Conduit, ConnectionSettings, DEFAULT_INITIAL_CHANNEL_CREDIT, HandshakeResult, Link, MaybeSend,
    MaybeSync, MessageFamily, Metadata, MetadataExt, Parity, SplitLink, VoxObserver,
    VoxObserverHandle, metadata_into_owned,
};

use crate::LinkSource;
use crate::{
    BareConduit, IntoConduit, accept_transport, handshake_as_acceptor, handshake_as_initiator,
    initiate_transport,
};

use super::{
    CloseRequest, ConnectionAcceptor, OpenRequest, Session, SessionError, SessionHandle,
    SessionKeepaliveConfig,
};
use crate::FromVoxSession;

/// Well-known metadata key for service name routing.
pub const VOX_SERVICE_METADATA_KEY: &str = "vox-service";

/// Inject `vox-service` metadata from `Client::SERVICE_NAME`.
fn inject_service_metadata<Client: FromVoxSession>(metadata: &mut Metadata) {
    vox_types::meta_set(metadata, VOX_SERVICE_METADATA_KEY, Client::SERVICE_NAME);
}

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
) -> SessionInitiatorBuilder<I::Conduit> {
    SessionInitiatorBuilder::new(into_conduit.into_conduit(), handshake_result)
}

pub fn initiator<S>(source: S) -> SessionSourceInitiatorBuilder<S>
where
    S: LinkSource,
{
    SessionSourceInitiatorBuilder::new(source)
}

pub fn acceptor_conduit<I: IntoConduit>(
    into_conduit: I,
    handshake_result: HandshakeResult,
) -> SessionAcceptorBuilder<I::Conduit> {
    SessionAcceptorBuilder::new(into_conduit.into_conduit(), handshake_result)
}

/// Convenience: perform the phon handshake as initiator on a raw link, then return
/// a builder with the conduit ready to go.
pub async fn initiator_on_link<L: Link>(
    link: L,
    settings: ConnectionSettings,
) -> Result<
    SessionInitiatorBuilder<BareConduit<MessageFamily, SplitLink<L::Tx, L::Rx>>>,
    SessionError,
>
where
    L::Tx: MaybeSend + MaybeSync + 'static,
    L::Rx: MaybeSend + 'static,
{
    let (tx, mut rx) = link.split();
    let handshake_result =
        handshake_as_initiator(&tx, &mut rx, settings, vox_types::Metadata::default())
            .await
            .map_err(session_error_from_handshake)?;
    let message_plan =
        crate::MessagePlan::from_handshake(&handshake_result).map_err(SessionError::Protocol)?;
    Ok(SessionInitiatorBuilder::new(
        BareConduit::with_message_plan(SplitLink { tx, rx }, message_plan),
        handshake_result,
    ))
}

/// Convenience: perform the phon handshake as acceptor on a raw link, then return
/// a builder with the conduit ready to go.
pub async fn acceptor_on_link<L: Link>(
    link: L,
    settings: ConnectionSettings,
) -> Result<SessionAcceptorBuilder<BareConduit<MessageFamily, SplitLink<L::Tx, L::Rx>>>, SessionError>
where
    L::Tx: MaybeSend + MaybeSync + 'static,
    L::Rx: MaybeSend + 'static,
{
    let (tx, mut rx) = link.split();
    let handshake_result =
        handshake_as_acceptor(&tx, &mut rx, settings, vox_types::Metadata::default())
            .await
            .map_err(session_error_from_handshake)?;
    let message_plan =
        crate::MessagePlan::from_handshake(&handshake_result).map_err(SessionError::Protocol)?;
    Ok(SessionAcceptorBuilder::new(
        BareConduit::with_message_plan(SplitLink { tx, rx }, message_plan),
        handshake_result,
    ))
}

pub fn initiator_on<L: Link>(link: L) -> SessionTransportInitiatorBuilder<L> {
    SessionTransportInitiatorBuilder::new(link)
}

pub fn initiator_transport<L: Link>(link: L) -> SessionTransportInitiatorBuilder<L> {
    initiator_on(link)
}

pub fn acceptor_on<L: Link>(link: L) -> SessionTransportAcceptorBuilder<L> {
    SessionTransportAcceptorBuilder::new(link)
}

pub fn acceptor_transport<L: Link>(link: L) -> SessionTransportAcceptorBuilder<L> {
    acceptor_on(link)
}

/// Shared configuration for all session builders.
pub struct SessionConfig {
    pub root_settings: ConnectionSettings,
    pub metadata: Metadata,
    pub on_connection: Option<Arc<dyn ConnectionAcceptor>>,
    pub keepalive: Option<SessionKeepaliveConfig>,
    pub spawn_fn: SpawnFn,
    pub connect_timeout: Option<std::time::Duration>,
    pub observer: Option<VoxObserverHandle>,
}

impl SessionConfig {
    fn with_settings(root_settings: ConnectionSettings) -> Self {
        Self {
            root_settings,
            metadata: vox_types::Metadata::default(),
            on_connection: None,
            keepalive: None,
            spawn_fn: default_spawn_fn(),
            connect_timeout: None,
            observer: None,
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self::with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: DEFAULT_INITIAL_CHANNEL_CREDIT,
        })
    }
}

pub struct SessionInitiatorBuilder<C> {
    conduit: C,
    handshake_result: HandshakeResult,
    config: SessionConfig,
}

impl<C> SessionInitiatorBuilder<C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        let config = SessionConfig::with_settings(root_settings);
        Self {
            conduit,
            handshake_result,
            config,
        }
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn channel_capacity(mut self, channel_capacity: u32) -> Self {
        self.config.root_settings.initial_channel_credit = channel_capacity;
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer(mut self, observer: impl VoxObserver) -> Self {
        self.config.observer = Some(Arc::new(observer));
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer_handle(mut self, observer: VoxObserverHandle) -> Self {
        self.config.observer = Some(observer);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Establish a session using the given settings, on the given link source, etc,
    ///
    ///   - requiring (as an arg) a handler for the service the local peer will serve
    ///   - returning a caller for the service we expect the remote peer to serve
    pub async fn establish<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        C::Rx: MaybeSend + 'static,
    {
        let Self {
            conduit,
            mut handshake_result,
            config,
        } = self;
        validate_negotiated_root_settings(&config.root_settings, &handshake_result)?;
        let mut peer_metadata = std::mem::take(&mut handshake_result.peer_metadata);
        let (tx, rx) = conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let acceptor: Arc<dyn ConnectionAcceptor> =
            config.on_connection.unwrap_or_else(|| Arc::new(()));
        let mut session = Session::pre_handshake(
            tx,
            rx,
            Some(acceptor.clone()),
            open_rx,
            close_rx,
            control_tx.clone(),
            control_rx,
            config.keepalive,
            config.observer.clone(),
        );
        let handle = session.establish_from_handshake(handshake_result)?;
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
            control_tx,
        };
        // Route the root connection through the acceptor.
        let caller_slot = Arc::new(std::sync::Mutex::new(None::<crate::Caller>));
        let pending = super::PendingConnection::with_caller_slot(handle, caller_slot.clone());
        vox_types::meta_set(
            &mut peer_metadata,
            VOX_SERVICE_METADATA_KEY,
            Client::SERVICE_NAME,
        );
        let request = super::ConnectionRequest::new(&peer_metadata)?;
        tracing::debug!(
            service = Client::SERVICE_NAME,
            "vox root connection routing to acceptor"
        );
        match acceptor.accept(&request, pending) {
            Ok(()) => tracing::debug!(
                service = Client::SERVICE_NAME,
                "vox root connection accepted"
            ),
            Err(metadata) => {
                tracing::debug!(
                    service = Client::SERVICE_NAME,
                    metadata_len = metadata.meta_len(),
                    "vox root connection rejected"
                );
                return Err(SessionError::Rejected(metadata));
            }
        }
        let caller =
            caller_slot.lock().unwrap().take().expect(
                "root connection acceptor must call handle_with (not into_handle or proxy_to)",
            );
        let client = Client::from_vox_session(caller, Some(session_handle));
        (config.spawn_fn)(Box::pin(async move { session.run().await }));
        Ok(client)
    }
}

pub struct SessionSourceInitiatorBuilder<S> {
    source: S,
    config: SessionConfig,
}

impl<S> SessionSourceInitiatorBuilder<S> {
    fn new(source: S) -> Self {
        let config = SessionConfig::default();
        Self { source, config }
    }

    pub fn parity(mut self, parity: Parity) -> Self {
        self.config.root_settings.parity = parity;
        self
    }

    pub fn root_settings(mut self, settings: ConnectionSettings) -> Self {
        self.config.root_settings = settings;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: u32) -> Self {
        self.config.root_settings.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn channel_capacity(mut self, channel_capacity: u32) -> Self {
        self.config.root_settings.initial_channel_credit = channel_capacity;
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer(mut self, observer: impl VoxObserver) -> Self {
        self.config.observer = Some(Arc::new(observer));
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer_handle(mut self, observer: VoxObserverHandle) -> Self {
        self.config.observer = Some(observer);
        self
    }

    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.config.metadata = metadata;
        self
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    pub async fn establish<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        S: LinkSource,
        S::Link: Link + MaybeSend + 'static,
        <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
        <S::Link as Link>::Rx: MaybeSend + 'static,
    {
        let connect_timeout = self.config.connect_timeout;
        let fut = self.establish_inner::<Client>();
        match connect_timeout {
            Some(timeout) => vox_types::time::tokio::timeout(timeout, fut)
                .await
                .map_err(|_| SessionError::ConnectTimeout)?,
            None => fut.await,
        }
    }

    // r[impl transport.prologue.first-payload]
    // r[impl transport.prologue.post-accept]
    async fn establish_inner<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        S: LinkSource,
        S::Link: Link + MaybeSend + 'static,
        <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
        <S::Link as Link>::Rx: MaybeSend + 'static,
    {
        let Self {
            mut source,
            mut config,
        } = self;
        inject_service_metadata::<Client>(&mut config.metadata);

        {
            {
                let attachment = source.next_link().await.map_err(SessionError::Io)?;
                let mut link = initiate_transport(attachment.into_link())
                    .await
                    .map_err(session_error_from_transport)?;
                let handshake_result = handshake_as_initiator(
                    &link.tx,
                    &mut link.rx,
                    config.root_settings.clone(),
                    metadata_into_owned(config.metadata.clone()),
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let builder = SessionInitiatorBuilder::new(
                    BareConduit::with_message_plan(link, message_plan),
                    handshake_result,
                );
                SessionTransportInitiatorBuilder::<S::Link>::apply_common_parts(builder, config)
                    .establish()
                    .await
            }
        }
    }
}

pub struct SessionTransportInitiatorBuilder<L> {
    link: L,
    config: SessionConfig,
}

impl<L> SessionTransportInitiatorBuilder<L> {
    fn new(link: L) -> Self {
        let config = SessionConfig::default();
        Self { link, config }
    }

    pub fn parity(mut self, parity: Parity) -> Self {
        self.config.root_settings.parity = parity;
        self
    }

    pub fn root_settings(mut self, settings: ConnectionSettings) -> Self {
        self.config.root_settings = settings;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: u32) -> Self {
        self.config.root_settings.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn channel_capacity(mut self, channel_capacity: u32) -> Self {
        self.config.root_settings.initial_channel_credit = channel_capacity;
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer(mut self, observer: impl VoxObserver) -> Self {
        self.config.observer = Some(Arc::new(observer));
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer_handle(mut self, observer: VoxObserverHandle) -> Self {
        self.config.observer = Some(observer);
        self
    }

    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.config.metadata = metadata;
        self
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn establish<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let connect_timeout = self.config.connect_timeout;
        let fut = self.establish_inner::<Client>();
        match connect_timeout {
            Some(timeout) => vox_types::time::tokio::timeout(timeout, fut)
                .await
                .map_err(|_| SessionError::ConnectTimeout)?,
            None => fut.await,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    // r[impl transport.prologue.first-payload]
    // r[impl transport.prologue.post-accept]
    async fn establish_inner<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self { link, mut config } = self;
        inject_service_metadata::<Client>(&mut config.metadata);
        let link = initiate_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        Self::finish_with_bare_parts(link, config).await
    }

    #[cfg(target_arch = "wasm32")]
    // r[impl transport.prologue.first-payload]
    // r[impl transport.prologue.post-accept]
    pub async fn establish<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self { link, mut config } = self;
        inject_service_metadata::<Client>(&mut config.metadata);
        let link = initiate_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        Self::finish_with_bare_parts(link, config).await
    }

    async fn finish_with_bare_parts<Client: FromVoxSession>(
        mut link: SplitLink<L::Tx, L::Rx>,
        config: SessionConfig,
    ) -> Result<Client, SessionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let handshake_result = handshake_as_initiator(
            &link.tx,
            &mut link.rx,
            config.root_settings.clone(),
            metadata_into_owned(config.metadata.clone()),
        )
        .await
        .map_err(session_error_from_handshake)?;
        let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
            .map_err(SessionError::Protocol)?;
        let builder = SessionInitiatorBuilder::new(
            BareConduit::with_message_plan(link, message_plan),
            handshake_result,
        );
        Self::apply_common_parts(builder, config).establish().await
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_common_parts<C>(
        mut builder: SessionInitiatorBuilder<C>,
        config: SessionConfig,
    ) -> SessionInitiatorBuilder<C> {
        builder.config = config;
        builder
    }
}

pub struct SessionAcceptorBuilder<C> {
    conduit: C,
    handshake_result: HandshakeResult,
    config: SessionConfig,
}

impl<C> SessionAcceptorBuilder<C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        let config = SessionConfig::with_settings(root_settings);
        Self {
            conduit,
            handshake_result,
            config,
        }
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn channel_capacity(mut self, channel_capacity: u32) -> Self {
        self.config.root_settings.initial_channel_credit = channel_capacity;
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer(mut self, observer: impl VoxObserver) -> Self {
        self.config.observer = Some(Arc::new(observer));
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer_handle(mut self, observer: VoxObserverHandle) -> Self {
        self.config.observer = Some(observer);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[moire::instrument]
    pub async fn establish<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        C::Rx: MaybeSend + 'static,
    {
        let Self {
            conduit,
            mut handshake_result,
            config,
        } = self;
        validate_negotiated_root_settings(&config.root_settings, &handshake_result)?;
        let mut peer_metadata = std::mem::take(&mut handshake_result.peer_metadata);
        let (tx, rx) = conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let acceptor: Arc<dyn ConnectionAcceptor> =
            config.on_connection.unwrap_or_else(|| Arc::new(()));
        let mut session = Session::pre_handshake(
            tx,
            rx,
            Some(acceptor.clone()),
            open_rx,
            close_rx,
            control_tx.clone(),
            control_rx,
            config.keepalive,
            config.observer.clone(),
        );
        let handle = session.establish_from_handshake(handshake_result)?;
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
            control_tx,
        };
        // Route the root connection through the acceptor.
        let caller_slot = Arc::new(std::sync::Mutex::new(None::<crate::Caller>));
        let pending = super::PendingConnection::with_caller_slot(handle, caller_slot.clone());
        vox_types::meta_set(
            &mut peer_metadata,
            VOX_SERVICE_METADATA_KEY,
            Client::SERVICE_NAME,
        );
        let request = super::ConnectionRequest::new(&peer_metadata)?;
        tracing::debug!(
            service = Client::SERVICE_NAME,
            "vox root connection routing to acceptor"
        );
        match acceptor.accept(&request, pending) {
            Ok(()) => tracing::debug!(
                service = Client::SERVICE_NAME,
                "vox root connection accepted"
            ),
            Err(metadata) => {
                tracing::debug!(
                    service = Client::SERVICE_NAME,
                    metadata_len = metadata.meta_len(),
                    "vox root connection rejected"
                );
                return Err(SessionError::Rejected(metadata));
            }
        }
        let caller =
            caller_slot.lock().unwrap().take().expect(
                "root connection acceptor must call handle_with (not into_handle or proxy_to)",
            );
        let client = Client::from_vox_session(caller, Some(session_handle));
        (config.spawn_fn)(Box::pin(async move { session.run().await }));
        Ok(client)
    }
}

pub struct SessionTransportAcceptorBuilder<L: Link> {
    link: L,
    config: SessionConfig,
}

impl<L: Link> SessionTransportAcceptorBuilder<L> {
    fn new(link: L) -> Self {
        Self {
            link,
            config: SessionConfig::with_settings(ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
                initial_channel_credit: DEFAULT_INITIAL_CHANNEL_CREDIT,
            }),
        }
    }

    pub fn root_settings(mut self, settings: ConnectionSettings) -> Self {
        self.config.root_settings = settings;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: u32) -> Self {
        self.config.root_settings.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn channel_capacity(mut self, channel_capacity: u32) -> Self {
        self.config.root_settings.initial_channel_credit = channel_capacity;
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer(mut self, observer: impl VoxObserver) -> Self {
        self.config.observer = Some(Arc::new(observer));
        self
    }

    // r[impl rpc.observability.runtime]
    pub fn observer_handle(mut self, observer: VoxObserverHandle) -> Self {
        self.config.observer = Some(observer);
        self
    }

    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.config.metadata = metadata;
        self
    }

    pub fn on_connection(mut self, acceptor: impl ConnectionAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: SessionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxSessionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[moire::instrument]
    pub async fn establish<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        L: Link + MaybeSend + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self { link, mut config } = self;
        inject_service_metadata::<Client>(&mut config.metadata);
        let mut link = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        let handshake_result = handshake_as_acceptor(
            &link.tx,
            &mut link.rx,
            config.root_settings.clone(),
            metadata_into_owned(config.metadata.clone()),
        )
        .await
        .map_err(session_error_from_handshake)?;
        let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
            .map_err(SessionError::Protocol)?;
        let builder = SessionAcceptorBuilder::new(
            BareConduit::with_message_plan(link, message_plan),
            handshake_result,
        );
        Self::apply_common_parts(builder, config).establish().await
    }

    fn apply_common_parts<C>(
        mut builder: SessionAcceptorBuilder<C>,
        config: SessionConfig,
    ) -> SessionAcceptorBuilder<C> {
        builder.config = config;
        builder
    }
}

fn validate_negotiated_root_settings(
    expected_root_settings: &ConnectionSettings,
    handshake_result: &HandshakeResult,
) -> Result<(), SessionError> {
    if expected_root_settings.initial_channel_credit == 0
        || handshake_result.peer_settings.initial_channel_credit == 0
    {
        return Err(SessionError::Protocol(
            "initial_channel_credit must be greater than zero".into(),
        ));
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify rpc.flow-control.max-concurrent-requests.default]
    #[test]
    fn session_config_default_advertises_request_limit() {
        let config = SessionConfig::default();
        assert_eq!(config.root_settings.max_concurrent_requests, 64);
    }
}
