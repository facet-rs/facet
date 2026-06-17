use std::{future::Future, pin::Pin, sync::Arc};

use vox_rt::sync::mpsc;
use vox_types::{
    Conduit, ConnectionRole, ConnectionSettings, DEFAULT_INITIAL_CHANNEL_CREDIT,
    EstablishmentOutcome, EstablishmentPhase, HandshakeResult, Link, LinkRx, LinkTx, MaybeSend,
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
    LaneAcceptor, OpenRequest, observe_establishment_finished, observe_establishment_started,
};

/// Well-known metadata key for service name routing.
pub const VOX_SERVICE_METADATA_KEY: &str = "vox-service";

use crate::FromVoxLane;

/// A pinned, boxed connection future. On non-WASM this is `Send + 'static`;
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

// r[impl rpc.connection-setup]
// r[impl connection.role]
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
        message_plan_from_handshake_observed(&handshake_result, None, ConnectionRole::Initiator)?;
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
        message_plan_from_handshake_observed(&handshake_result, None, ConnectionRole::Acceptor)?;
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

/// Shared configuration for all connection builders.
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

// r[impl rpc.observability.establishment]
fn message_plan_from_handshake_observed(
    handshake_result: &HandshakeResult,
    observer: Option<&VoxObserverHandle>,
    role: ConnectionRole,
) -> Result<crate::MessagePlan, ConnectionError> {
    let started_at =
        observe_establishment_started(observer, role, EstablishmentPhase::SchemaDecodePlan, None);
    match crate::MessagePlan::from_handshake(handshake_result) {
        Ok(plan) => {
            observe_establishment_finished(
                observer,
                role,
                EstablishmentPhase::SchemaDecodePlan,
                None,
                EstablishmentOutcome::Ok,
                started_at,
            );
            Ok(plan)
        }
        Err(error) => {
            observe_establishment_finished(
                observer,
                role,
                EstablishmentPhase::SchemaDecodePlan,
                None,
                EstablishmentOutcome::Error,
                started_at,
            );
            Err(ConnectionError::Protocol(error))
        }
    }
}

// r[impl rpc.observability.establishment]
async fn initiate_transport_observed<L: Link>(
    link: L,
    observer: Option<&VoxObserverHandle>,
) -> Result<SplitLink<L::Tx, L::Rx>, ConnectionError> {
    let started_at = observe_establishment_started(
        observer,
        ConnectionRole::Initiator,
        EstablishmentPhase::VoxTransportPrologue,
        None,
    );
    match initiate_transport(link).await {
        Ok(link) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Initiator,
                EstablishmentPhase::VoxTransportPrologue,
                None,
                EstablishmentOutcome::Ok,
                started_at,
            );
            Ok(link)
        }
        Err(error) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Initiator,
                EstablishmentPhase::VoxTransportPrologue,
                None,
                EstablishmentOutcome::Error,
                started_at,
            );
            Err(session_error_from_transport(error))
        }
    }
}

// r[impl rpc.observability.establishment]
async fn accept_transport_observed<L: Link>(
    link: L,
    observer: Option<&VoxObserverHandle>,
) -> Result<SplitLink<L::Tx, L::Rx>, ConnectionError> {
    let started_at = observe_establishment_started(
        observer,
        ConnectionRole::Acceptor,
        EstablishmentPhase::VoxTransportPrologue,
        None,
    );
    match accept_transport(link).await {
        Ok(link) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Acceptor,
                EstablishmentPhase::VoxTransportPrologue,
                None,
                EstablishmentOutcome::Ok,
                started_at,
            );
            Ok(link)
        }
        Err(error) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Acceptor,
                EstablishmentPhase::VoxTransportPrologue,
                None,
                EstablishmentOutcome::Error,
                started_at,
            );
            Err(session_error_from_transport(error))
        }
    }
}

// r[impl rpc.observability.establishment]
async fn handshake_as_initiator_observed<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: Metadata,
    observer: Option<&VoxObserverHandle>,
) -> Result<HandshakeResult, ConnectionError> {
    let started_at = observe_establishment_started(
        observer,
        ConnectionRole::Initiator,
        EstablishmentPhase::ConnectionHandshake,
        None,
    );
    match handshake_as_initiator(tx, rx, settings, metadata).await {
        Ok(handshake_result) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Initiator,
                EstablishmentPhase::ConnectionHandshake,
                None,
                EstablishmentOutcome::Ok,
                started_at,
            );
            Ok(handshake_result)
        }
        Err(error) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Initiator,
                EstablishmentPhase::ConnectionHandshake,
                None,
                EstablishmentOutcome::Error,
                started_at,
            );
            Err(session_error_from_handshake(error))
        }
    }
}

// r[impl rpc.observability.establishment]
async fn handshake_as_acceptor_observed<Tx: LinkTx, Rx: LinkRx>(
    tx: &Tx,
    rx: &mut Rx,
    settings: ConnectionSettings,
    metadata: Metadata,
    observer: Option<&VoxObserverHandle>,
) -> Result<HandshakeResult, ConnectionError> {
    let started_at = observe_establishment_started(
        observer,
        ConnectionRole::Acceptor,
        EstablishmentPhase::ConnectionHandshake,
        None,
    );
    match handshake_as_acceptor(tx, rx, settings, metadata).await {
        Ok(handshake_result) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Acceptor,
                EstablishmentPhase::ConnectionHandshake,
                None,
                EstablishmentOutcome::Ok,
                started_at,
            );
            Ok(handshake_result)
        }
        Err(error) => {
            observe_establishment_finished(
                observer,
                ConnectionRole::Acceptor,
                EstablishmentPhase::ConnectionHandshake,
                None,
                EstablishmentOutcome::Error,
                started_at,
            );
            Err(session_error_from_handshake(error))
        }
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

    /// Override the function used to spawn the connection background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the connection background task.
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
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("connection.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("connection.close", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("connection.control");
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
                let link =
                    initiate_transport_observed(attachment.into_link(), config.observer.as_ref())
                        .await?;
                ConnectionTransportInitiatorBuilder::<S::Link>::finish_with_bare_parts(link, config)
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
        let link = initiate_transport_observed(link, config.observer.as_ref()).await?;
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
        let link = initiate_transport_observed(link, config.observer.as_ref()).await?;
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
        let handshake_result = handshake_as_initiator_observed(
            &link.tx,
            &mut link.rx,
            config.root_settings.clone(),
            metadata_into_owned(config.metadata.clone()),
            config.observer.as_ref(),
        )
        .await?;
        let message_plan = message_plan_from_handshake_observed(
            &handshake_result,
            config.observer.as_ref(),
            ConnectionRole::Initiator,
        )?;
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

    /// Override the function used to spawn the connection background task.
    /// Defaults to `tokio::spawn` on non-WASM and `wasm_bindgen_futures::spawn_local` on WASM.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_fn(mut self, f: impl FnOnce(BoxConnectionFuture) + Send + 'static) -> Self {
        self.config.spawn_fn = Box::new(f);
        self
    }

    /// Override the function used to spawn the connection background task.
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
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("connection.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("connection.close", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("connection.control");
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
        let mut link = accept_transport_observed(link, config.observer.as_ref()).await?;
        let handshake_result = handshake_as_acceptor_observed(
            &link.tx,
            &mut link.rx,
            config.root_settings.clone(),
            metadata_into_owned(config.metadata.clone()),
            config.observer.as_ref(),
        )
        .await?;
        let message_plan = message_plan_from_handshake_observed(
            &handshake_result,
            config.observer.as_ref(),
            ConnectionRole::Acceptor,
        )?;
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
    use std::sync::{Arc, Mutex};

    use vox_types::{EstablishmentContext, EstablishmentEvent, LaneId};

    use super::*;

    #[derive(Default)]
    struct RecordingEstablishmentObserver {
        events: Arc<Mutex<Vec<EstablishmentEvent>>>,
    }

    impl VoxObserver for RecordingEstablishmentObserver {
        fn establishment_event(&self, event: EstablishmentEvent) {
            self.events
                .lock()
                .expect("establishment events mutex poisoned")
                .push(event);
        }
    }

    // r[verify rpc.flow-control.max-concurrent-requests.default]
    #[test]
    fn session_config_default_advertises_request_limit() {
        let config = ConnectionConfig::default();
        assert_eq!(config.root_settings.max_concurrent_requests, 64);
    }

    // r[verify rpc.observability.establishment]
    #[tokio::test]
    async fn memory_transport_reports_vox_establishment_phases_only() {
        let (client_link, server_link) = crate::memory_link_pair(16);
        let events = Arc::new(Mutex::new(Vec::new()));
        let observer: VoxObserverHandle = Arc::new(RecordingEstablishmentObserver {
            events: Arc::clone(&events),
        });

        let server_observer = Arc::clone(&observer);
        let server = tokio::spawn(async move {
            acceptor_on(server_link)
                .observer_handle(server_observer)
                .on_connection(crate::lane_acceptor_fn(
                    |request: &crate::LaneRequest, lane: crate::PendingLane| {
                        if request.service() == "Noop" {
                            lane.handle_with(());
                            Ok(())
                        } else {
                            Err(crate::LaneRejection::new(
                                crate::LaneRejectReason::UnknownService,
                            ))
                        }
                    },
                ))
                .establish_connection()
                .await
                .expect("server establish")
        });

        let client = initiator_on(client_link)
            .observer_handle(Arc::clone(&observer))
            .establish_connection()
            .await
            .expect("client establish");
        let server = server.await.expect("server task");

        let accepted = client
            .open_lane_handle(
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                    initial_channel_credit: DEFAULT_INITIAL_CHANNEL_CREDIT,
                },
                vox_types::metadata().str("vox-service", "Noop").build(),
            )
            .await
            .expect("accepted service lane");
        client
            .close_lane(accepted.connection_id(), Metadata::default())
            .await
            .expect("close accepted service lane");

        let rejected = client
            .open_lane_handle(
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                    initial_channel_credit: DEFAULT_INITIAL_CHANNEL_CREDIT,
                },
                vox_types::metadata().str("vox-service", "Missing").build(),
            )
            .await;
        assert!(
            matches!(rejected, Err(ConnectionError::Rejected(_))),
            "expected rejected service lane, got: {rejected:?}"
        );

        let _ = client.shutdown();
        let _ = server.shutdown();

        let events = events
            .lock()
            .expect("establishment events mutex poisoned")
            .clone();
        let contexts: Vec<EstablishmentContext> = events
            .iter()
            .map(|event| match *event {
                EstablishmentEvent::Started { context }
                | EstablishmentEvent::Finished { context, .. } => context,
            })
            .collect();

        for phase in [
            EstablishmentPhase::VoxTransportPrologue,
            EstablishmentPhase::ConnectionHandshake,
            EstablishmentPhase::SchemaDecodePlan,
            EstablishmentPhase::ServiceLaneOpen,
        ] {
            assert!(
                contexts.iter().any(|context| context.phase == phase),
                "missing establishment phase {phase:?}; events: {events:?}"
            );
        }

        for absent_phase in [
            EstablishmentPhase::TcpConnection,
            EstablishmentPhase::TlsHandshake,
            EstablishmentPhase::WebSocketUpgrade,
        ] {
            assert!(
                contexts.iter().all(|context| context.phase != absent_phase),
                "memory transport must not invent {absent_phase:?}; events: {events:?}"
            );
        }

        assert!(events.iter().any(|event| matches!(
            event,
            EstablishmentEvent::Finished {
                context,
                outcome: EstablishmentOutcome::Ok,
                ..
            } if context.phase == EstablishmentPhase::ServiceLaneOpen
                && context.lane_id == Some(LaneId(1))
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            EstablishmentEvent::Finished {
                context,
                outcome: EstablishmentOutcome::Rejected,
                ..
            } if context.phase == EstablishmentPhase::ServiceLaneOpen
        )));
    }
}
