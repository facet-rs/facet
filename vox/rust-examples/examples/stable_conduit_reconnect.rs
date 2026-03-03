use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use eyre::{Result, WrapErr, eyre};
use roam::facet::Facet;
use roam::{
    Attachment, Backing, Conduit, ConduitRx, ConduitTx, ConduitTxPermit, Link, LinkRx, LinkSource,
    LinkTx, MemoryLink, MemoryLinkRx, MemoryLinkRxError, MemoryLinkTx, MsgFamily, StableConduit,
    prepare_acceptor_attachment,
};

struct StringFamily;

impl MsgFamily for StringFamily {
    type Msg<'a> = String;

    fn shape() -> &'static roam::facet::Shape {
        String::SHAPE
    }
}

#[derive(Clone)]
struct LinkKillSwitch {
    tripped: Arc<AtomicBool>,
}

impl LinkKillSwitch {
    fn new() -> Self {
        Self {
            tripped: Arc::new(AtomicBool::new(false)),
        }
    }

    fn trip(&self) {
        self.tripped.store(true, Ordering::Relaxed);
    }

    fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Relaxed)
    }
}

struct KillableMemoryLink {
    inner: MemoryLink,
    kill_switch: LinkKillSwitch,
}

#[derive(Clone)]
struct KillableMemoryLinkTx {
    inner: MemoryLinkTx,
    kill_switch: LinkKillSwitch,
}

struct KillableMemoryLinkRx {
    inner: MemoryLinkRx,
    kill_switch: LinkKillSwitch,
}

impl Link for KillableMemoryLink {
    type Tx = KillableMemoryLinkTx;
    type Rx = KillableMemoryLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx, rx) = self.inner.split();
        (
            KillableMemoryLinkTx {
                inner: tx,
                kill_switch: self.kill_switch.clone(),
            },
            KillableMemoryLinkRx {
                inner: rx,
                kill_switch: self.kill_switch,
            },
        )
    }
}

impl LinkTx for KillableMemoryLinkTx {
    type Permit = roam::MemoryLinkTxPermit;

    async fn reserve(&self) -> std::io::Result<Self::Permit> {
        if self.kill_switch.is_tripped() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "link intentionally cut",
            ));
        }
        self.inner.reserve().await
    }

    async fn close(self) -> std::io::Result<()> {
        self.inner.close().await
    }
}

impl LinkRx for KillableMemoryLinkRx {
    type Error = MemoryLinkRxError;

    async fn recv(&mut self) -> std::result::Result<Option<Backing>, Self::Error> {
        if self.kill_switch.is_tripped() {
            return Ok(None);
        }
        self.inner.recv().await
    }
}

fn killable_memory_link_pair(
    buffer: usize,
) -> (KillableMemoryLink, KillableMemoryLink, LinkKillSwitch) {
    let (a, b) = roam::memory_link_pair(buffer);
    let kill_switch = LinkKillSwitch::new();
    (
        KillableMemoryLink {
            inner: a,
            kill_switch: kill_switch.clone(),
        },
        KillableMemoryLink {
            inner: b,
            kill_switch: kill_switch.clone(),
        },
        kill_switch,
    )
}

struct QueuedInitiatorLinkSource<L> {
    links: VecDeque<Attachment<L>>,
}

impl<L> QueuedInitiatorLinkSource<L> {
    fn new(links: Vec<Attachment<L>>) -> Self {
        Self {
            links: links.into(),
        }
    }
}

impl<L> LinkSource for QueuedInitiatorLinkSource<L>
where
    L: Link + Send + 'static,
{
    type Link = L;

    async fn next_link(&mut self) -> std::io::Result<Attachment<Self::Link>> {
        self.links.pop_front().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "no more links available for stable reconnect",
            )
        })
    }
}

struct QueuedAcceptorLinkSource<L> {
    links: VecDeque<L>,
}

impl<L> QueuedAcceptorLinkSource<L> {
    fn new(links: Vec<L>) -> Self {
        Self {
            links: links.into(),
        }
    }
}

impl<L> LinkSource for QueuedAcceptorLinkSource<L>
where
    L: Link + Send + 'static,
{
    type Link = roam::SplitLink<L::Tx, L::Rx>;

    async fn next_link(&mut self) -> std::io::Result<Attachment<Self::Link>> {
        let link = self.links.pop_front().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "no more inbound links available for stable reconnect",
            )
        })?;
        prepare_acceptor_attachment(link)
            .await
            .map_err(|e| std::io::Error::other(format!("prepare_acceptor_attachment failed: {e}")))
    }
}

fn main() -> Result<()> {
    println!("[demo] stable_conduit_reconnect: starting runtime");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("building Tokio runtime")?;
    rt.block_on(run_demo())
}

async fn run_demo() -> Result<()> {
    let (client_link_1, server_link_1, kill_switch_1) = killable_memory_link_pair(64);
    let (client_link_2, server_link_2, _kill_switch_2) = killable_memory_link_pair(64);

    let client_source = QueuedInitiatorLinkSource::new(vec![
        Attachment::initiator(client_link_1),
        Attachment::initiator(client_link_2),
    ]);
    let server_source = QueuedAcceptorLinkSource::new(vec![server_link_1, server_link_2]);

    println!("[demo] building client/server stable conduits");
    let server_conduit_task =
        tokio::spawn(async move { StableConduit::<StringFamily, _>::new(server_source).await });
    let client_conduit = StableConduit::<StringFamily, _>::new(client_source)
        .await
        .map_err(|e| eyre!("client StableConduit::new failed: {e}"))?;
    let server_conduit = server_conduit_task
        .await
        .wrap_err("joining server_conduit_task")?
        .map_err(|e| eyre!("server StableConduit::new failed: {e}"))?;

    let (client_tx, mut client_rx) = client_conduit.split();
    let (server_tx, mut server_rx) = server_conduit.split();

    let server_task = tokio::spawn(async move {
        println!("[server] waiting for first message");
        let first = server_rx
            .recv()
            .await
            .expect("server recv #1")
            .expect("server eof #1");
        println!("[server] <- {}", first.as_str());
        server_tx
            .reserve()
            .await
            .expect("server reserve #1")
            .send(format!("echo:{}", first.as_str()))
            .expect("server send #1");
        println!("[server] -> echo:{}", first.as_str());

        println!("[server] intentionally cutting physical link #1");
        kill_switch_1.trip();

        println!("[server] waiting for second message (after reconnect)");
        let second = server_rx
            .recv()
            .await
            .expect("server recv #2")
            .expect("server eof #2");
        println!("[server] <- {}", second.as_str());
        server_tx
            .reserve()
            .await
            .expect("server reserve #2")
            .send(format!("echo:{}", second.as_str()))
            .expect("server send #2");
        println!("[server] -> echo:{}", second.as_str());
    });

    println!("[client] sending first message on link #1");
    client_tx
        .reserve()
        .await
        .map_err(|e| eyre!("client reserve #1 failed: {e}"))?
        .send("alpha".to_string())
        .map_err(|e| eyre!("client send #1 failed: {e}"))?;
    let first_reply = client_rx
        .recv()
        .await
        .map_err(|e| eyre!("client recv #1 failed: {e}"))?
        .ok_or_else(|| eyre!("client recv #1: unexpected eof"))?;
    println!("[client] <- {}", first_reply.as_str());
    assert_eq!(first_reply.as_str(), "echo:alpha");

    println!("[client] sending second message (will force reconnect to link #2)");
    client_tx
        .reserve()
        .await
        .map_err(|e| eyre!("client reserve #2 failed: {e}"))?
        .send("beta".to_string())
        .map_err(|e| eyre!("client send #2 failed: {e}"))?;
    let second_reply = client_rx
        .recv()
        .await
        .map_err(|e| eyre!("client recv #2 failed: {e}"))?
        .ok_or_else(|| eyre!("client recv #2: unexpected eof"))?;
    println!("[client] <- {}", second_reply.as_str());
    assert_eq!(second_reply.as_str(), "echo:beta");

    server_task.await.wrap_err("joining server_task")?;
    println!("[demo] stable_conduit_reconnect: complete");
    Ok(())
}
