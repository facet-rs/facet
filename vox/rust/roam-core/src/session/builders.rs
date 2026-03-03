use std::{future::Future, pin::Pin};

use moire::sync::mpsc;
use roam_types::{
    Conduit, ConduitTx, ConnectionSettings, Handler, MaybeSend, MaybeSync, MessageFamily, Metadata,
    Parity,
};

use crate::IntoConduit;

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

pub struct SessionInitiatorBuilder<'a, C> {
    conduit: C,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
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
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let mut session: Session<C> = Session::pre_handshake(
            tx,
            rx,
            self.on_connection,
            open_rx,
            close_rx,
            control_tx.clone(),
            control_rx,
            self.keepalive,
        );
        let handle = session
            .establish_as_initiator(self.root_settings, self.metadata)
            .await?;
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
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

pub struct SessionAcceptorBuilder<'a, C> {
    conduit: C,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
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
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control");
        let mut session: Session<C> = Session::pre_handshake(
            tx,
            rx,
            self.on_connection,
            open_rx,
            close_rx,
            control_tx.clone(),
            control_rx,
            self.keepalive,
        );
        let handle = session
            .establish_as_acceptor(self.root_settings, self.metadata)
            .await?;
        let session_handle = SessionHandle {
            open_tx,
            close_tx,
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
