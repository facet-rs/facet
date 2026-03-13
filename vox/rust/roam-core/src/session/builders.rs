use std::{future::Future, pin::Pin};

use moire::sync::mpsc;
use roam_types::{
    Conduit, ConduitTx, ConnectionSettings, Handler, Link, MaybeSend, MaybeSync, MessageFamily,
    Metadata, Parity, SplitLink,
};

use crate::{BareConduit, IntoConduit, TransportMode, accept_transport, initiate_transport};
#[cfg(not(target_arch = "wasm32"))]
use crate::{
    StableConduit, prepare_acceptor_attachment, single_attachment_source, single_link_source,
};

use super::{
    CloseRequest, ConnectionAcceptor, OpenRequest, Session, SessionError, SessionHandle,
    SessionKeepaliveConfig,
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
pub fn initiator<I: IntoConduit>(into_conduit: I) -> SessionInitiatorBuilder<'static, I::Conduit> {
    SessionInitiatorBuilder::new(into_conduit.into_conduit())
}

// r[impl session.role]
pub fn acceptor<I: IntoConduit>(into_conduit: I) -> SessionAcceptorBuilder<'static, I::Conduit> {
    SessionAcceptorBuilder::new(into_conduit.into_conduit())
}

pub fn initiator_transport<L: Link>(
    link: L,
    mode: TransportMode,
) -> SessionTransportInitiatorBuilder<'static, L> {
    SessionTransportInitiatorBuilder::new(link, mode)
}

pub fn acceptor_transport<L: Link>(link: L) -> SessionTransportAcceptorBuilder<'static, L> {
    SessionTransportAcceptorBuilder::new(link)
}

pub struct SessionInitiatorBuilder<'a, C> {
    conduit: C,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
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
        let mut driver = Driver::new(handle, handler);
        let client = Client::from(driver.caller());
        (self.spawn_fn)(Box::pin(async move { session.run().await }));
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        Ok((client, session_handle))
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
        spawn_fn: SpawnFn,
    ) -> SessionInitiatorBuilder<'a, C> {
        builder.root_settings = root_settings;
        builder.metadata = metadata;
        builder.on_connection = on_connection;
        builder.keepalive = keepalive;
        builder.resumable = resumable;
        builder.spawn_fn = spawn_fn;
        builder
    }
}

pub struct SessionAcceptorBuilder<'a, C> {
    conduit: C,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
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
        );
        let handle = session
            .establish_as_acceptor(self.root_settings, self.metadata)
            .await?;
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
            resume_tx,
            control_tx,
        };
        let mut driver = Driver::new(handle, handler);
        let client = Client::from(driver.caller());
        (self.spawn_fn)(Box::pin(async move { session.run().await }));
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        Ok((client, session_handle))
    }
}

pub struct SessionTransportAcceptorBuilder<'a, L: Link> {
    link: L,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
    resumable: bool,
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
            resumable: false,
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
            root_settings,
            metadata,
            on_connection,
            keepalive,
            resumable,
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

    #[cfg(not(target_arch = "wasm32"))]
    async fn finish_with_stable_parts<Client: From<DriverCaller>>(
        link: SplitLink<L::Tx, L::Rx>,
        root_settings: ConnectionSettings,
        metadata: Metadata<'a>,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
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
        spawn_fn: SpawnFn,
    ) -> SessionAcceptorBuilder<'a, C> {
        builder.root_settings = root_settings;
        builder.metadata = metadata;
        builder.on_connection = on_connection;
        builder.keepalive = keepalive;
        builder.resumable = resumable;
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
