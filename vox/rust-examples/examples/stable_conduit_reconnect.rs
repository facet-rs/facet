use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use eyre::{Result, WrapErr, eyre};
use vox::{
    Attachment, Backing, ConnectionSettings, DriverCaller, HandshakeResult, Link, LinkRx,
    LinkSource, LinkTx, MemoryLink, MemoryLinkRx, MemoryLinkRxError, MemoryLinkTx, MessageFamily,
    Parity, Rx, SessionRole, StableConduit, Tx, channel, prepare_acceptor_attachment,
};

#[vox::service]
trait StableLab {
    async fn bump(&self) -> u32;
    async fn transform(&self, prefix: String, input: Rx<String>, output: Tx<String>) -> u32;
}

#[derive(Clone)]
struct StableLabService {
    counter: Arc<AtomicU32>,
}

impl StableLabService {
    fn new() -> Self {
        Self {
            counter: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl StableLab for StableLabService {
    async fn bump(&self) -> u32 {
        self.counter.fetch_add(1, Ordering::Relaxed) + 1
    }

    async fn transform(&self, prefix: String, mut input: Rx<String>, output: Tx<String>) -> u32 {
        let mut count = 0;
        while let Ok(Some(item)) = input.recv().await {
            if output
                .send(format!("{prefix}:{}", item.as_str()))
                .await
                .is_err()
            {
                break;
            }
            count += 1;
        }
        let _ = output.close(Default::default()).await;
        count
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
    type Permit = vox::MemoryLinkTxPermit;

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
    let (a, b) = vox::memory_link_pair(buffer);
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
    type Link = vox::SplitLink<L::Tx, L::Rx>;

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
        tokio::spawn(async move { StableConduit::<MessageFamily, _>::new(server_source).await });
    let client_conduit = StableConduit::<MessageFamily, _>::new(client_source)
        .await
        .map_err(|e| eyre!("client StableConduit::new failed: {e}"))?;
    let server_conduit = server_conduit_task
        .await
        .wrap_err("joining server_conduit_task")?
        .map_err(|e| eyre!("server StableConduit::new failed: {e}"))?;

    println!("[demo] establishing vox session over stable conduits");
    let server_task = tokio::spawn(async move {
        let server_settings = ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        };
        let (server_guard, _) = vox::acceptor_conduit(
            server_conduit,
            HandshakeResult {
                role: SessionRole::Acceptor,
                our_settings: server_settings.clone(),
                peer_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                peer_supports_retry: false,
                session_resume_key: None,
                peer_resume_key: None,
                our_schema: vec![],
                peer_schema: vec![],
            },
        )
        .establish::<DriverCaller>(StableLabDispatcher::new(StableLabService::new()))
        .await
        .expect("server establish");
        let _server_guard = server_guard;
        std::future::pending::<()>().await;
    });

    let client_settings = ConnectionSettings {
        parity: Parity::Odd,
        max_concurrent_requests: 64,
    };
    let (client, _) = vox::initiator_conduit(
        client_conduit,
        HandshakeResult {
            role: SessionRole::Initiator,
            our_settings: client_settings.clone(),
            peer_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            peer_supports_retry: false,
            session_resume_key: None,
            peer_resume_key: None,
            our_schema: vec![],
            peer_schema: vec![],
        },
    )
    .establish::<StableLabClient>(())
    .await
    .map_err(|e| eyre!("client establish failed: {e:?}"))?;
    println!("[demo] session established");

    println!("[client] calling bump before cut");
    let first = client
        .bump()
        .await
        .map_err(|e| eyre!("bump #1 failed: {e:?}"))?;
    println!("[client] bump #1 -> {first}");
    assert_eq!(first, 1);

    let (input_tx, input_rx) = channel::<String>();
    let (output_tx, mut output_rx) = channel::<String>();
    let cut_switch = kill_switch_1.clone();

    let send_task = tokio::spawn(async move {
        for (idx, word) in ["one", "two", "three"].iter().enumerate() {
            println!("[client/send] -> {word}");
            input_tx
                .send((*word).to_string())
                .await
                .expect("send input");
            if idx == 1 {
                println!("[demo] intentionally cutting physical link #1 mid-channel");
                cut_switch.trip();
            }
        }
        println!("[client/send] closing input");
        input_tx
            .close(Default::default())
            .await
            .expect("close input");
    });

    println!("[client] calling transform (channel state should survive reconnect)");
    let transformed_count = client
        .transform("item".to_string(), input_rx, output_tx)
        .await
        .map_err(|e| eyre!("transform failed: {e:?}"))?;
    println!("[client] transform returned count={transformed_count}");
    assert_eq!(transformed_count, 3);
    send_task.await.wrap_err("joining send_task")?;

    let mut got = Vec::new();
    while let Some(item) = output_rx
        .recv()
        .await
        .wrap_err("receiving from output_rx")?
    {
        println!("[client/recv] <- {}", item.as_str());
        got.push(item.to_string());
    }
    assert_eq!(got, vec!["item:one", "item:two", "item:three"]);
    println!("[client] output stream complete: {got:?}");

    println!("[client] calling bump after reconnect");
    let second = client
        .bump()
        .await
        .map_err(|e| eyre!("bump #2 failed: {e:?}"))?;
    println!("[client] bump #2 -> {second}");
    assert_eq!(second, 2);

    server_task.abort();
    println!("[demo] stable_conduit_reconnect: complete");
    Ok(())
}
