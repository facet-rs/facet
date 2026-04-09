use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use moire::sync::mpsc;
use moire::time;
use vox_types::{
    Conduit, ConnectionSettings, HandshakeResult, Link, MaybeSend, MaybeSync, MessageFamily,
    Metadata, Parity, SessionResumeKey, SplitLink, metadata_into_owned,
};

use crate::{Attachment, LinkSource, StableConduit};
use crate::{
    BareConduit, IntoConduit, OperationStore, TransportMode, accept_transport,
    handshake_as_acceptor, handshake_as_initiator, initiate_transport,
};

use super::{
    CloseRequest, ConduitRecoverer, ConnectionAcceptor, OpenRequest, Session, SessionError,
    SessionHandle, SessionKeepaliveConfig,
};
use crate::FromVoxSession;

/// Well-known metadata key for service name routing.
pub const VOX_SERVICE_METADATA_KEY: &str = "vox-service";

/// Inject `vox-service` metadata from `Client::SERVICE_NAME`.
fn inject_service_metadata<Client: FromVoxSession>(metadata: &mut Metadata<'_>) {
    metadata.push(vox_types::MetadataEntry {
        key: VOX_SERVICE_METADATA_KEY.into(),
        value: vox_types::MetadataValue::String(Client::SERVICE_NAME.into()),
        flags: vox_types::MetadataFlags::NONE,
    });
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
) -> SessionInitiatorBuilder<'static, I::Conduit> {
    SessionInitiatorBuilder::new(into_conduit.into_conduit(), handshake_result)
}

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
    let handshake_result = handshake_as_initiator(&tx, &mut rx, settings, true, None, vec![])
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
    let handshake_result = handshake_as_acceptor(&tx, &mut rx, settings, true, false, None, vec![])
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
    Established(Client),
    Resumed,
}

/// Shared configuration for all session builders.
pub struct SessionConfig<'a> {
    pub root_settings: ConnectionSettings,
    pub metadata: Metadata<'a>,
    pub on_connection: Option<Arc<dyn ConnectionAcceptor>>,
    pub keepalive: Option<SessionKeepaliveConfig>,
    pub resumable: bool,
    pub session_registry: Option<SessionRegistry>,
    pub operation_store: Option<Arc<dyn OperationStore>>,
    pub spawn_fn: SpawnFn,
    pub connect_timeout: Option<std::time::Duration>,
    pub recovery_timeout: Option<std::time::Duration>,
}

impl SessionConfig<'_> {
    fn with_settings(root_settings: ConnectionSettings) -> Self {
        Self {
            root_settings,
            metadata: vec![],
            on_connection: None,
            keepalive: None,
            resumable: true,
            session_registry: None,
            operation_store: None,
            spawn_fn: default_spawn_fn(),
            connect_timeout: None,
            recovery_timeout: None,
        }
    }
}

impl Default for SessionConfig<'_> {
    fn default() -> Self {
        Self::with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        })
    }
}

pub struct SessionInitiatorBuilder<'a, C> {
    conduit: C,
    handshake_result: HandshakeResult,
    config: SessionConfig<'a>,
    recoverer: Option<Box<dyn ConduitRecoverer>>,
}

impl<'a, C> SessionInitiatorBuilder<'a, C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        let mut config = SessionConfig::with_settings(root_settings);
        // Conduit builders default to non-resumable — callers opt in with .resumable()
        config.resumable = false;
        Self {
            conduit,
            handshake_result,
            config,
            recoverer: None,
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

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    pub fn recovery_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.recovery_timeout = Some(timeout);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.config.resumable = true;
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.config.operation_store = Some(operation_store);
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
            recoverer,
        } = self;
        validate_negotiated_root_settings(&config.root_settings, &handshake_result)?;
        let mut peer_metadata = std::mem::take(&mut handshake_result.peer_metadata);
        let (tx, rx) = conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (resume_tx, resume_rx) = mpsc::channel::<super::ResumeRequest>("session.resume", 1);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let acceptor: Arc<dyn ConnectionAcceptor> =
            config.on_connection.unwrap_or_else(|| Arc::new(()));
        let mut session = Session::pre_handshake(
            tx,
            rx,
            Some(acceptor.clone()),
            open_rx,
            close_rx,
            resume_rx,
            control_tx.clone(),
            control_rx,
            config.keepalive,
            config.resumable,
            recoverer,
            config.recovery_timeout,
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
        // Route the root connection through the acceptor.
        let caller_slot = Arc::new(std::sync::Mutex::new(None::<crate::Caller>));
        let pending = super::PendingConnection::with_caller_slot(
            handle,
            caller_slot.clone(),
            config.operation_store,
        );
        peer_metadata.push(vox_types::MetadataEntry::str(
            VOX_SERVICE_METADATA_KEY,
            Client::SERVICE_NAME,
        ));
        let request = super::ConnectionRequest::new(&peer_metadata)?;
        acceptor
            .accept(&request, pending)
            .map_err(SessionError::Rejected)?;
        let caller =
            caller_slot.lock().unwrap().take().expect(
                "root connection acceptor must call handle_with (not into_handle or proxy_to)",
            );
        let client = Client::from_vox_session(caller, Some(session_handle));
        (config.spawn_fn)(Box::pin(async move { session.run().await }));
        Ok(client)
    }
}

pub struct SessionSourceInitiatorBuilder<'a, S> {
    source: S,
    mode: TransportMode,
    config: SessionConfig<'a>,
}

impl<'a, S> SessionSourceInitiatorBuilder<'a, S> {
    fn new(source: S, mode: TransportMode) -> Self {
        let config = SessionConfig {
            resumable: false,
            ..SessionConfig::default()
        };
        Self {
            source,
            mode,
            config,
        }
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

    pub fn metadata(mut self, metadata: Metadata<'a>) -> Self {
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

    pub fn recovery_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.recovery_timeout = Some(timeout);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.config.resumable = true;
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.config.operation_store = Some(operation_store);
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
            Some(timeout) => time::timeout(timeout, fut)
                .await
                .map_err(|_| SessionError::ConnectTimeout)?,
            None => fut.await,
        }
    }

    async fn establish_inner<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        S: LinkSource,
        S::Link: Link + MaybeSend + 'static,
        <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
        <S::Link as Link>::Rx: MaybeSend + 'static,
    {
        let Self {
            mut source,
            mode,
            mut config,
        } = self;
        inject_service_metadata::<Client>(&mut config.metadata);

        match mode {
            TransportMode::Bare => {
                let attachment = source.next_link().await.map_err(SessionError::Io)?;
                let mut link = initiate_transport(attachment.into_link(), TransportMode::Bare)
                    .await
                    .map_err(session_error_from_transport)?;
                let handshake_result = handshake_as_initiator(
                    &link.tx,
                    &mut link.rx,
                    config.root_settings.clone(),
                    true,
                    None,
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
                let recoverer = Box::new(BareSourceRecoverer {
                    source,
                    settings: config.root_settings.clone(),
                    connect_timeout: config.connect_timeout,
                    metadata: metadata_into_owned(config.metadata.clone()),
                });
                SessionTransportInitiatorBuilder::<S::Link>::apply_common_parts(
                    builder,
                    config,
                    Some(recoverer),
                )
                .establish()
                .await
            }
            TransportMode::Stable => {
                let attachment = source.next_link().await.map_err(SessionError::Io)?;
                let mut link = initiate_transport(attachment.into_link(), TransportMode::Stable)
                    .await
                    .map_err(session_error_from_transport)?;
                let handshake_result = handshake_as_initiator(
                    &link.tx,
                    &mut link.rx,
                    config.root_settings.clone(),
                    true,
                    None,
                    metadata_into_owned(config.metadata.clone()),
                )
                .await
                .map_err(session_error_from_handshake)?;
                let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
                    .map_err(SessionError::Protocol)?;
                let conduit = StableConduit::<MessageFamily, _>::with_first_link(
                    link.tx,
                    link.rx,
                    None,
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
                    builder, config, None,
                )
                .establish()
                .await
            }
        }
    }
}

pub struct SessionTransportInitiatorBuilder<'a, L> {
    link: L,
    mode: TransportMode,
    config: SessionConfig<'a>,
}

impl<'a, L> SessionTransportInitiatorBuilder<'a, L> {
    fn new(link: L, mode: TransportMode) -> Self {
        let config = SessionConfig {
            resumable: false,
            ..SessionConfig::default()
        };
        Self { link, mode, config }
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

    pub fn metadata(mut self, metadata: Metadata<'a>) -> Self {
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

    pub fn recovery_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.recovery_timeout = Some(timeout);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.config.resumable = true;
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.config.operation_store = Some(operation_store);
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
            Some(timeout) => tokio::time::timeout(timeout, fut)
                .await
                .map_err(|_| SessionError::ConnectTimeout)?,
            None => fut.await,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn establish_inner<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            mode,
            mut config,
        } = self;
        inject_service_metadata::<Client>(&mut config.metadata);
        match mode {
            TransportMode::Bare => {
                let link = initiate_transport(link, TransportMode::Bare)
                    .await
                    .map_err(session_error_from_transport)?;
                Self::finish_with_bare_parts(link, config).await
            }
            TransportMode::Stable => {
                let link = initiate_transport(link, TransportMode::Stable)
                    .await
                    .map_err(session_error_from_transport)?;
                Self::finish_with_stable_parts(link, config).await
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn establish<Client: FromVoxSession>(self) -> Result<Client, SessionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self {
            link,
            mode,
            mut config,
        } = self;
        inject_service_metadata::<Client>(&mut config.metadata);
        match mode {
            TransportMode::Bare => {
                let link = initiate_transport(link, TransportMode::Bare)
                    .await
                    .map_err(session_error_from_transport)?;
                Self::finish_with_bare_parts(link, config).await
            }
            TransportMode::Stable => Err(SessionError::Protocol(
                "stable conduit transport selection is unsupported on wasm".into(),
            )),
        }
    }

    async fn finish_with_bare_parts<Client: FromVoxSession>(
        mut link: SplitLink<L::Tx, L::Rx>,
        config: SessionConfig<'a>,
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
            true,
            None,
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
        Self::apply_common_parts(builder, config, None)
            .establish()
            .await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn finish_with_stable_parts<Client: FromVoxSession>(
        mut link: SplitLink<L::Tx, L::Rx>,
        config: SessionConfig<'a>,
    ) -> Result<Client, SessionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + Send + 'static,
        L::Rx: MaybeSend + Send + 'static,
    {
        let handshake_result = handshake_as_initiator(
            &link.tx,
            &mut link.rx,
            config.root_settings.clone(),
            true,
            None,
            metadata_into_owned(config.metadata.clone()),
        )
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
        Self::apply_common_parts(builder, config, None)
            .establish()
            .await
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_common_parts<C>(
        mut builder: SessionInitiatorBuilder<'a, C>,
        config: SessionConfig<'a>,
        recoverer: Option<Box<dyn ConduitRecoverer>>,
    ) -> SessionInitiatorBuilder<'a, C> {
        builder.config = config;
        builder.recoverer = recoverer;
        builder
    }
}

struct BareSourceRecoverer<S> {
    source: S,
    settings: ConnectionSettings,
    connect_timeout: Option<Duration>,
    metadata: Metadata<'static>,
}

const SOURCE_RECOVERY_BACKOFF_MIN: Duration = Duration::from_millis(100);
const SOURCE_RECOVERY_BACKOFF_MAX: Duration = Duration::from_secs(5);

impl<S> ConduitRecoverer for BareSourceRecoverer<S>
where
    S: LinkSource,
    S::Link: Link + MaybeSend + 'static,
    <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
    <S::Link as Link>::Rx: MaybeSend + 'static,
{
    fn next_conduit<'a>(
        &'a mut self,
        resume_key: Option<&'a SessionResumeKey>,
    ) -> vox_types::BoxFut<'a, Result<super::RecoveredConduit, SessionError>> {
        Box::pin(async move {
            let mut backoff = SOURCE_RECOVERY_BACKOFF_MIN;
            let mut use_resume_key = resume_key.is_some();

            loop {
                let selected_resume_key = if use_resume_key { resume_key } else { None };

                let attempt = async {
                    let attachment = self.source.next_link().await.map_err(SessionError::Io)?;
                    let mut link = initiate_transport(attachment.into_link(), TransportMode::Bare)
                        .await
                        .map_err(session_error_from_transport)?;
                    let handshake_result = handshake_as_initiator(
                        &link.tx,
                        &mut link.rx,
                        self.settings.clone(),
                        true,
                        selected_resume_key,
                        metadata_into_owned(self.metadata.clone()),
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
                };

                let result = match self.connect_timeout {
                    Some(timeout) => match time::timeout(timeout, attempt).await {
                        Ok(r) => r,
                        Err(_) => Err(SessionError::ConnectTimeout),
                    },
                    None => attempt.await,
                };

                match result {
                    Ok(conduit) => return Ok(conduit),
                    Err(e) if !e.is_retryable() => return Err(e),
                    Err(_) => {}
                }

                if use_resume_key {
                    // If a resumption attempt is rejected once, continue trying without
                    // a resume key so restart scenarios can establish a fresh session.
                    use_resume_key = false;
                }

                time::sleep(backoff).await;
                backoff = backoff.saturating_mul(2).min(SOURCE_RECOVERY_BACKOFF_MAX);
            }
        })
    }
}

struct TransportedLinkSource<S> {
    source: S,
    mode: TransportMode,
}

impl<S> LinkSource for TransportedLinkSource<S>
where
    S: LinkSource,
    S::Link: Link + MaybeSend + 'static,
    <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
    <S::Link as Link>::Rx: MaybeSend + 'static,
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
    config: SessionConfig<'a>,
}

impl<'a, C> SessionAcceptorBuilder<'a, C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        let mut config = SessionConfig::with_settings(root_settings);
        // Conduit builders default to non-resumable — callers opt in with .resumable()
        config.resumable = false;
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

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    pub fn recovery_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.recovery_timeout = Some(timeout);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.config.resumable = true;
        self
    }

    pub fn session_registry(mut self, session_registry: SessionRegistry) -> Self {
        self.config.session_registry = Some(session_registry);
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.config.operation_store = Some(operation_store);
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
        let (resume_tx, resume_rx) = mpsc::channel::<super::ResumeRequest>("session.resume", 1);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let acceptor: Arc<dyn ConnectionAcceptor> =
            config.on_connection.unwrap_or_else(|| Arc::new(()));
        let mut session = Session::pre_handshake(
            tx,
            rx,
            Some(acceptor.clone()),
            open_rx,
            close_rx,
            resume_rx,
            control_tx.clone(),
            control_rx,
            config.keepalive,
            config.resumable,
            None,
            config.recovery_timeout,
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
        if let (Some(registry), Some(key)) = (&config.session_registry, resume_key) {
            registry.insert(key, session_handle.clone());
            session.registered_in_registry = true;
        }
        // Route the root connection through the acceptor.
        let caller_slot = Arc::new(std::sync::Mutex::new(None::<crate::Caller>));
        let pending = super::PendingConnection::with_caller_slot(
            handle,
            caller_slot.clone(),
            config.operation_store,
        );
        peer_metadata.push(vox_types::MetadataEntry::str(
            VOX_SERVICE_METADATA_KEY,
            Client::SERVICE_NAME,
        ));
        let request = super::ConnectionRequest::new(&peer_metadata)?;
        acceptor
            .accept(&request, pending)
            .map_err(SessionError::Rejected)?;
        let caller =
            caller_slot.lock().unwrap().take().expect(
                "root connection acceptor must call handle_with (not into_handle or proxy_to)",
            );
        let client = Client::from_vox_session(caller, Some(session_handle));
        (config.spawn_fn)(Box::pin(async move { session.run().await }));
        Ok(client)
    }

    #[moire::instrument]
    pub async fn establish_or_resume<Client: FromVoxSession>(
        self,
    ) -> Result<SessionAcceptOutcome<Client>, SessionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        C::Rx: MaybeSend + 'static,
    {
        // With the CBOR handshake, resume detection happens at the link level
        // before the conduit is created. If the peer sent a resume key in the Hello
        // that matches a known session, we resume. Otherwise, we establish.
        if let (Some(registry), Some(resume_key)) = (
            &self.config.session_registry,
            self.handshake_result.peer_resume_key,
        ) && let Some(handle) = registry.get(&resume_key)
        {
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

        let client = self.establish().await?;
        Ok(SessionAcceptOutcome::Established(client))
    }
}

pub struct SessionTransportAcceptorBuilder<'a, L: Link> {
    link: L,
    config: SessionConfig<'a>,
}

impl<'a, L: Link> SessionTransportAcceptorBuilder<'a, L> {
    fn new(link: L) -> Self {
        Self {
            link,
            config: SessionConfig::with_settings(ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
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

    pub fn metadata(mut self, metadata: Metadata<'a>) -> Self {
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

    pub fn recovery_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.recovery_timeout = Some(timeout);
        self
    }

    pub fn resumable(mut self) -> Self {
        self.config.resumable = true;
        self
    }

    pub fn session_registry(mut self, session_registry: SessionRegistry) -> Self {
        self.config.session_registry = Some(session_registry);
        self
    }

    pub fn operation_store(mut self, operation_store: Arc<dyn OperationStore>) -> Self {
        self.config.operation_store = Some(operation_store);
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
        let (mode, mut link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let handshake_result = handshake_as_acceptor(
                    &link.tx,
                    &mut link.rx,
                    config.root_settings.clone(),
                    true,
                    config.resumable,
                    None,
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
            TransportMode::Stable => Self::finish_with_stable_parts(link, config).await,
        }
    }

    #[moire::instrument]
    pub async fn establish_or_resume<Client: FromVoxSession>(
        self,
    ) -> Result<SessionAcceptOutcome<Client>, SessionError>
    where
        L: Link + MaybeSend + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self { link, config } = self;
        let (mode, mut link) = accept_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        match mode {
            TransportMode::Bare => {
                let handshake_result = handshake_as_acceptor(
                    &link.tx,
                    &mut link.rx,
                    config.root_settings.clone(),
                    true,
                    config.resumable,
                    None,
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
                Self::apply_common_parts(builder, config)
                    .establish_or_resume()
                    .await
            }
            TransportMode::Stable => Self::finish_with_stable_parts(link, config)
                .await
                .map(SessionAcceptOutcome::Established),
        }
    }

    async fn finish_with_stable_parts<Client: FromVoxSession>(
        mut link: SplitLink<L::Tx, L::Rx>,
        config: SessionConfig<'a>,
    ) -> Result<Client, SessionError>
    where
        L: Link + MaybeSend + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let handshake_result = handshake_as_acceptor(
            &link.tx,
            &mut link.rx,
            config.root_settings.clone(),
            true,
            config.resumable,
            None,
            metadata_into_owned(config.metadata.clone()),
        )
        .await
        .map_err(session_error_from_handshake)?;
        let message_plan = crate::MessagePlan::from_handshake(&handshake_result)
            .map_err(SessionError::Protocol)?;
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
        Self::apply_common_parts(builder, config).establish().await
    }

    fn apply_common_parts<C>(
        mut builder: SessionAcceptorBuilder<'a, C>,
        config: SessionConfig<'a>,
    ) -> SessionAcceptorBuilder<'a, C> {
        builder.config = config;
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
