use moire::sync::mpsc;
use roam_types::{
    Conduit, ConduitTx, ConnectionSettings, MaybeSend, MaybeSync, MessageFamily, Metadata, Parity,
};

use super::{
    CloseRequest, ConnectionAcceptor, ConnectionHandle, OpenRequest, Session, SessionError,
    SessionHandle, SessionKeepaliveConfig,
};

// r[impl rpc.session-setup]
// r[impl session.role]
pub fn initiator<C>(conduit: C) -> SessionInitiatorBuilder<'static, C> {
    SessionInitiatorBuilder::new(conduit)
}

// r[impl session.role]
pub fn acceptor<C>(conduit: C) -> SessionAcceptorBuilder<'static, C> {
    SessionAcceptorBuilder::new(conduit)
}

pub struct SessionInitiatorBuilder<'a, C> {
    conduit: C,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
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

    pub async fn establish(
        self,
    ) -> Result<(Session<C>, ConnectionHandle, SessionHandle), SessionError>
    where
        C: Conduit<Msg = MessageFamily>,
        C::Tx: MaybeSend + MaybeSync + 'static,
        for<'p> <C::Tx as ConduitTx>::Permit<'p>: MaybeSend,
        C::Rx: MaybeSend,
    {
        let (tx, rx) = self.conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let mut session = Session::pre_handshake(
            tx,
            rx,
            self.on_connection,
            open_rx,
            close_rx,
            self.keepalive,
        );
        let handle = session
            .establish_as_initiator(self.root_settings, self.metadata)
            .await?;
        let session_handle = SessionHandle { open_tx, close_tx };
        Ok((session, handle, session_handle))
    }
}

pub struct SessionAcceptorBuilder<'a, C> {
    conduit: C,
    root_settings: ConnectionSettings,
    metadata: Metadata<'a>,
    on_connection: Option<Box<dyn ConnectionAcceptor>>,
    keepalive: Option<SessionKeepaliveConfig>,
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

    #[moire::instrument]
    pub async fn establish(
        self,
    ) -> Result<(Session<C>, ConnectionHandle, SessionHandle), SessionError>
    where
        C: Conduit<Msg = MessageFamily>,
        C::Tx: MaybeSend + MaybeSync + 'static,
        for<'p> <C::Tx as ConduitTx>::Permit<'p>: MaybeSend,
        C::Rx: MaybeSend,
    {
        let (tx, rx) = self.conduit.split();
        let (open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open", 4);
        let (close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close", 4);
        let mut session = Session::pre_handshake(
            tx,
            rx,
            self.on_connection,
            open_rx,
            close_rx,
            self.keepalive,
        );
        let handle = session
            .establish_as_acceptor(self.root_settings, self.metadata)
            .await?;
        let session_handle = SessionHandle { open_tx, close_tx };
        Ok((session, handle, session_handle))
    }
}
