use std::{future::Future, pin::Pin, sync::Arc};

use vox_rt::sync::mpsc;
use vox_types::{
    Conduit, ConnectionSettings, DEFAULT_INITIAL_CHANNEL_CREDIT, HandshakeResult, Link, MaybeSend,
    MaybeSync, MessageFamily, Metadata, Parity, SplitLink, VoxObserver, VoxObserverHandle,
    metadata_into_owned,
};

use crate::LinkSource;
use crate::{
    BareConduit, IntoConduit, accept_transport, handshake_as_acceptor, handshake_as_initiator,
    initiate_transport,
};

use super::{
    CloseRequest, Connection, ConnectionError, ConnectionHandle, ConnectionKeepaliveConfig,
    LaneAcceptor, OpenRequest,
};

/// Well-known metadata key for service name routing.
pub const VOX_SERVICE_METADATA_KEY: &str = "vox-service";

use crate::FromVoxLane;

/// A pinned, boxed session future. On non-WASM this is `Send + 'static`;
/// on WASM it's `'static` only (no `Send` requirement).
#[cfg(not(target_arch = "wasm32"))]
pub type BoxConnectionFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type BoxConnectionFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

#[cfg(not(target_arch = "wasm32"))]
type SpawnFn = Box<dyn FnOnce(BoxConnectionFuture) + Send + 'static>;
#[cfg(target_arch = "wasm32")]
type SpawnFn = Box<dyn FnOnce(BoxConnectionFuture) + 'static>;

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
) -> ConnectionInitiatorBuilder<I::Conduit> {
    ConnectionInitiatorBuilder::new(into_conduit.into_conduit(), handshake_result)
}

pub fn initiator<S>(source: S) -> ConnectionSourceInitiatorBuilder<S>
where
    S: LinkSource,
{
    ConnectionSourceInitiatorBuilder::new(source)
}

pub fn acceptor_conduit<I: IntoConduit>(
    into_conduit: I,
    handshake_result: HandshakeResult,
) -> ConnectionAcceptorBuilder<I::Conduit> {
    ConnectionAcceptorBuilder::new(into_conduit.into_conduit(), handshake_result)
}

/// Convenience: perform the phon handshake as initiator on a raw link, then return
/// a builder with the conduit ready to go.
pub async fn initiator_on_link<L: Link>(
    link: L,
    settings: ConnectionSettings,
) -> Result<
    ConnectionInitiatorBuilder<BareConduit<MessageFamily, SplitLink<L::Tx, L::Rx>>>,
    ConnectionError,
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
        crate::MessagePlan::from_handshake(&handshake_result).map_err(ConnectionError::Protocol)?;
    Ok(ConnectionInitiatorBuilder::new(
        BareConduit::with_message_plan(SplitLink { tx, rx }, message_plan),
        handshake_result,
    ))
}

/// Convenience: perform the phon handshake as acceptor on a raw link, then return
/// a builder with the conduit ready to go.
pub async fn acceptor_on_link<L: Link>(
    link: L,
    settings: ConnectionSettings,
) -> Result<
    ConnectionAcceptorBuilder<BareConduit<MessageFamily, SplitLink<L::Tx, L::Rx>>>,
    ConnectionError,
>
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
        crate::MessagePlan::from_handshake(&handshake_result).map_err(ConnectionError::Protocol)?;
    Ok(ConnectionAcceptorBuilder::new(
        BareConduit::with_message_plan(SplitLink { tx, rx }, message_plan),
        handshake_result,
    ))
}

pub fn initiator_on<L: Link>(link: L) -> ConnectionTransportInitiatorBuilder<L> {
    ConnectionTransportInitiatorBuilder::new(link)
}

pub fn initiator_transport<L: Link>(link: L) -> ConnectionTransportInitiatorBuilder<L> {
    initiator_on(link)
}

pub fn acceptor_on<L: Link>(link: L) -> ConnectionTransportAcceptorBuilder<L> {
    ConnectionTransportAcceptorBuilder::new(link)
}

pub fn acceptor_transport<L: Link>(link: L) -> ConnectionTransportAcceptorBuilder<L> {
    acceptor_on(link)
}

/// Shared configuration for all session builders.
pub struct ConnectionConfig {
    pub root_settings: ConnectionSettings,
    pub metadata: Metadata,
    pub on_connection: Option<Arc<dyn LaneAcceptor>>,
    pub keepalive: Option<ConnectionKeepaliveConfig>,
    pub spawn_fn: SpawnFn,
    pub connect_timeout: Option<std::time::Duration>,
    pub observer: Option<VoxObserverHandle>,
}

impl ConnectionConfig {
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

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self::with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: DEFAULT_INITIAL_CHANNEL_CREDIT,
        })
    }
}

pub struct ConnectionInitiatorBuilder<C> {
    conduit: C,
    handshake_result: HandshakeResult,
    config: ConnectionConfig,
}

impl<C> ConnectionInitiatorBuilder<C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        let config = ConnectionConfig::with_settings(root_settings);
        Self {
            conduit,
            handshake_result,
            config,
        }
    }

    pub fn on_connection(mut self, acceptor: impl LaneAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: ConnectionKeepaliveConfig) -> Self {
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
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Establish the Vox connection and start its driven runtime.
    ///
    /// The root/control lane is private and transitional; user services are
    /// reached by opening lanes on the returned [`ConnectionHandle`].
    pub async fn establish_connection(self) -> Result<ConnectionHandle, ConnectionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        C::Rx: MaybeSend + 'static,
    {
        let Self {
            conduit,
            handshake_result,
            config,
        } = self;
        validate_negotiated_root_settings(&config.root_settings, &handshake_result)?;
        let (tx, rx) = conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let mut connection = Connection::pre_handshake(
            tx,
            rx,
            config.on_connection,
            open_rx,
            close_rx,
            control_tx.clone(),
            control_rx,
            config.keepalive,
            config.observer.clone(),
        );
        let root_lane = connection.establish_from_handshake(handshake_result)?;
        let mut root_driver = crate::Driver::new(root_lane, ());
        let control_caller = crate::Caller::new(root_driver.caller());
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { root_driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { root_driver.run().await });

        let connection_handle = ConnectionHandle {
            open_tx,
            close_tx,
            control_tx,
            _control_caller: Some(control_caller),
        };
        (config.spawn_fn)(Box::pin(async move { connection.run().await }));
        Ok(connection_handle)
    }

    /// Establish a connection and open the requested service lane.
    pub async fn establish<Client: FromVoxLane>(self) -> Result<Client, ConnectionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        C::Rx: MaybeSend + 'static,
    {
        self.establish_connection()
            .await?
            .open_lane::<Client>()
            .await
    }
}

pub struct ConnectionSourceInitiatorBuilder<S> {
    source: S,
    config: ConnectionConfig,
}

impl<S> ConnectionSourceInitiatorBuilder<S> {
    fn new(source: S) -> Self {
        let config = ConnectionConfig::default();
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

    pub fn on_connection(mut self, acceptor: impl LaneAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: ConnectionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    pub async fn establish_connection(self) -> Result<ConnectionHandle, ConnectionError>
    where
        S: LinkSource,
        S::Link: Link + MaybeSend + 'static,
        <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
        <S::Link as Link>::Rx: MaybeSend + 'static,
    {
        let connect_timeout = self.config.connect_timeout;
        let fut = self.establish_connection_inner();
        match connect_timeout {
            Some(timeout) => vox_types::time::tokio::timeout(timeout, fut)
                .await
                .map_err(|_| ConnectionError::ConnectTimeout)?,
            None => fut.await,
        }
    }

    pub async fn establish<Client: FromVoxLane>(self) -> Result<Client, ConnectionError>
    where
        S: LinkSource,
        S::Link: Link + MaybeSend + 'static,
        <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
        <S::Link as Link>::Rx: MaybeSend + 'static,
    {
        self.establish_connection()
            .await?
            .open_lane::<Client>()
            .await
    }

    // r[impl transport.prologue.first-payload]
    // r[impl transport.prologue.post-accept]
    async fn establish_connection_inner(self) -> Result<ConnectionHandle, ConnectionError>
    where
        S: LinkSource,
        S::Link: Link + MaybeSend + 'static,
        <S::Link as Link>::Tx: MaybeSend + MaybeSync + 'static,
        <S::Link as Link>::Rx: MaybeSend + 'static,
    {
        let Self { mut source, config } = self;

        {
            {
                let attachment = source.next_link().await.map_err(ConnectionError::Io)?;
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
                    .map_err(ConnectionError::Protocol)?;
                let builder = ConnectionInitiatorBuilder::new(
                    BareConduit::with_message_plan(link, message_plan),
                    handshake_result,
                );
                ConnectionTransportInitiatorBuilder::<S::Link>::apply_common_parts(builder, config)
                    .establish_connection()
                    .await
            }
        }
    }
}

pub struct ConnectionTransportInitiatorBuilder<L> {
    link: L,
    config: ConnectionConfig,
}

impl<L> ConnectionTransportInitiatorBuilder<L> {
    fn new(link: L) -> Self {
        let config = ConnectionConfig::default();
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

    pub fn on_connection(mut self, acceptor: impl LaneAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: ConnectionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn establish_connection(self) -> Result<ConnectionHandle, ConnectionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let connect_timeout = self.config.connect_timeout;
        let fut = self.establish_connection_inner();
        match connect_timeout {
            Some(timeout) => vox_types::time::tokio::timeout(timeout, fut)
                .await
                .map_err(|_| ConnectionError::ConnectTimeout)?,
            None => fut.await,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn establish<Client: FromVoxLane>(self) -> Result<Client, ConnectionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        self.establish_connection()
            .await?
            .open_lane::<Client>()
            .await
    }

    #[cfg(not(target_arch = "wasm32"))]
    // r[impl transport.prologue.first-payload]
    // r[impl transport.prologue.post-accept]
    async fn establish_connection_inner(self) -> Result<ConnectionHandle, ConnectionError>
    where
        L: Link + Send + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self { link, config } = self;
        let link = initiate_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        Self::finish_with_bare_parts(link, config).await
    }

    #[cfg(target_arch = "wasm32")]
    // r[impl transport.prologue.first-payload]
    // r[impl transport.prologue.post-accept]
    pub async fn establish_connection(self) -> Result<ConnectionHandle, ConnectionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self { link, config } = self;
        let link = initiate_transport(link)
            .await
            .map_err(session_error_from_transport)?;
        Self::finish_with_bare_parts(link, config).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn establish<Client: FromVoxLane>(self) -> Result<Client, ConnectionError>
    where
        L: Link + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        self.establish_connection()
            .await?
            .open_lane::<Client>()
            .await
    }

    async fn finish_with_bare_parts(
        mut link: SplitLink<L::Tx, L::Rx>,
        config: ConnectionConfig,
    ) -> Result<ConnectionHandle, ConnectionError>
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
            .map_err(ConnectionError::Protocol)?;
        let builder = ConnectionInitiatorBuilder::new(
            BareConduit::with_message_plan(link, message_plan),
            handshake_result,
        );
        Self::apply_common_parts(builder, config)
            .establish_connection()
            .await
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_common_parts<C>(
        mut builder: ConnectionInitiatorBuilder<C>,
        config: ConnectionConfig,
    ) -> ConnectionInitiatorBuilder<C> {
        builder.config = config;
        builder
    }
}

pub struct ConnectionAcceptorBuilder<C> {
    conduit: C,
    handshake_result: HandshakeResult,
    config: ConnectionConfig,
}

impl<C> ConnectionAcceptorBuilder<C> {
    fn new(conduit: C, handshake_result: HandshakeResult) -> Self {
        let root_settings = handshake_result.our_settings.clone();
        let config = ConnectionConfig::with_settings(root_settings);
        Self {
            conduit,
            handshake_result,
            config,
        }
    }

    pub fn on_connection(mut self, acceptor: impl LaneAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: ConnectionKeepaliveConfig) -> Self {
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
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the session background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[vox_rt::instrument]
    pub async fn establish_connection(self) -> Result<ConnectionHandle, ConnectionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        C::Rx: MaybeSend + 'static,
    {
        let Self {
            conduit,
            handshake_result,
            config,
        } = self;
        validate_negotiated_root_settings(&config.root_settings, &handshake_result)?;
        let (tx, rx) = conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let mut connection = Connection::pre_handshake(
            tx,
            rx,
            config.on_connection,
            open_rx,
            close_rx,
            control_tx.clone(),
            control_rx,
            config.keepalive,
            config.observer.clone(),
        );
        let root_lane = connection.establish_from_handshake(handshake_result)?;
        let mut root_driver = crate::Driver::new(root_lane, ());
        let control_caller = crate::Caller::new(root_driver.caller());
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { root_driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { root_driver.run().await });

        let connection_handle = ConnectionHandle {
            open_tx,
            close_tx,
            control_tx,
            _control_caller: Some(control_caller),
        };
        (config.spawn_fn)(Box::pin(async move { connection.run().await }));
        Ok(connection_handle)
    }

    pub async fn establish<Client: FromVoxLane>(self) -> Result<Client, ConnectionError>
    where
        C: Conduit<Msg = MessageFamily> + 'static,
        C::Tx: MaybeSend + MaybeSync + 'static,
        C::Rx: MaybeSend + 'static,
    {
        self.establish_connection()
            .await?
            .open_lane::<Client>()
            .await
    }
}

pub struct ConnectionTransportAcceptorBuilder<L: Link> {
    link: L,
    config: ConnectionConfig,
}

impl<L: Link> ConnectionTransportAcceptorBuilder<L> {
    fn new(link: L) -> Self {
        Self {
            link,
            config: ConnectionConfig::with_settings(ConnectionSettings {
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

    pub fn on_connection(mut self, acceptor: impl LaneAcceptor) -> Self {
        self.config.on_connection = Some(Arc::new(acceptor));
        self
    }

    pub fn keepalive(mut self, keepalive: ConnectionKeepaliveConfig) -> Self {
        self.config.keepalive = Some(keepalive);
        self
    }

    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.connect_timeout = Some(timeout);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    #[vox_rt::instrument]
    pub async fn establish_connection(self) -> Result<ConnectionHandle, ConnectionError>
    where
        L: Link + MaybeSend + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        let Self { link, config } = self;
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
            .map_err(ConnectionError::Protocol)?;
        let builder = ConnectionAcceptorBuilder::new(
            BareConduit::with_message_plan(link, message_plan),
            handshake_result,
        );
        Self::apply_common_parts(builder, config)
            .establish_connection()
            .await
    }

    #[vox_rt::instrument]
    pub async fn establish<Client: FromVoxLane>(self) -> Result<Client, ConnectionError>
    where
        L: Link + MaybeSend + 'static,
        L::Tx: MaybeSend + MaybeSync + 'static,
        L::Rx: MaybeSend + 'static,
    {
        self.establish_connection()
            .await?
            .open_lane::<Client>()
            .await
    }

    fn apply_common_parts<C>(
        mut builder: ConnectionAcceptorBuilder<C>,
        config: ConnectionConfig,
    ) -> ConnectionAcceptorBuilder<C> {
        builder.config = config;
        builder
    }
}

fn validate_negotiated_root_settings(
    expected_root_settings: &ConnectionSettings,
    handshake_result: &HandshakeResult,
) -> Result<(), ConnectionError> {
    if expected_root_settings.initial_channel_credit == 0
        || handshake_result.peer_settings.initial_channel_credit == 0
    {
        return Err(ConnectionError::Protocol(
            "initial_channel_credit must be greater than zero".into(),
        ));
    }

    if handshake_result.our_settings != *expected_root_settings {
        return Err(ConnectionError::Protocol(
            "negotiated root settings do not match builder settings".into(),
        ));
    }
    Ok(())
}

fn session_error_from_handshake(error: crate::HandshakeError) -> ConnectionError {
    match error {
        crate::HandshakeError::Io(io) => ConnectionError::Io(io),
        crate::HandshakeError::PeerClosed => {
            ConnectionError::Protocol("peer closed during handshake".into())
        }
        other => ConnectionError::Protocol(other.to_string()),
    }
}

fn session_error_from_transport(error: crate::TransportPrologueError) -> ConnectionError {
    match error {
        crate::TransportPrologueError::Io(io) => ConnectionError::Io(io),
        crate::TransportPrologueError::LinkDead => {
            ConnectionError::Protocol("link closed during transport prologue".into())
        }
        crate::TransportPrologueError::Protocol(message) => ConnectionError::Protocol(message),
        crate::TransportPrologueError::Rejected(reason) => {
            ConnectionError::Protocol(format!("transport rejected: {reason}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify rpc.flow-control.max-concurrent-requests.default]
    #[test]
    fn session_config_default_advertises_request_limit() {
        let config = ConnectionConfig::default();
        assert_eq!(config.root_settings.max_concurrent_requests, 64);
    }
}
