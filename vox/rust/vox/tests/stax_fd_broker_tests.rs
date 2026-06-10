//! Stax-shaped `vox::Fd` brokering through a real `#[vox::service]`.
//!
//! This is the Linux staxd shape: ordinary config/status/error DTOs travel in
//! the payload, while the perf-event descriptors travel as out-of-band `Fd`
//! capabilities and only work on fd-capable local transports.
#![cfg(unix)]

use std::io::{Read, Seek, Write};
use std::os::fd::OwnedFd;

use facet::Facet;
use vox::Fd;
use vox::transport::local::FdStreamLink;
use vox::transport::tcp::StreamLink;

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct PerfSessionConfig {
    target_pid: u32,
    frequency_hz: u32,
    kernel_stacks: bool,
    request_waking: bool,
    request_pmu: bool,
    request_dwarf_unwind: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct WakingFieldOffsets {
    wakee_pid_offset: u32,
    wakee_pid_size: u32,
}

#[derive(Debug, Facet)]
pub struct PerfSessionFds {
    sampling: Vec<Fd>,
    switch: Vec<Fd>,
    waking: Vec<Fd>,
    waking_field_offsets: Option<WakingFieldOffsets>,
    pmu: Vec<Fd>,
    pmu_ids: Vec<u64>,
    pmu_per_cpu: u32,
    cpu_count: u32,
    page_size: u32,
    data_pages: u32,
    target_pid: u32,
    frequency_hz: u32,
    kernel_stacks: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PerfSessionError {
    NotPrivileged {
        detail: String,
    },
    PerfEventOpen {
        cpu: u32,
        errno: i32,
        detail: String,
    },
    NoSuchTarget(u32),
    NotAuthorized {
        caller_uid: u32,
        target_uid: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct DaemonStatus {
    version: String,
    host_arch: String,
    privileged: bool,
    perf_event_paranoid: i32,
}

fn sample_config() -> PerfSessionConfig {
    PerfSessionConfig {
        target_pid: 42_424,
        frequency_hz: 997,
        kernel_stacks: true,
        request_waking: true,
        request_pmu: true,
        request_dwarf_unwind: false,
    }
}

fn temp_blob(seed: &[u8]) -> std::fs::File {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("vox-stax-fd-broker-{}-{nanos}", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    let _ = std::fs::remove_file(&path);
    f.write_all(seed).unwrap();
    f.rewind().unwrap();
    f
}

fn fd_blob(label: impl Into<String>) -> Fd {
    Fd::new(OwnedFd::from(temp_blob(label.into().as_bytes())))
}

fn read_fd(fd: Fd) -> String {
    let mut f = std::fs::File::from(fd.into_owned_fd().expect("owned fd"));
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    s
}

fn read_fds(fds: Vec<Fd>) -> Vec<String> {
    fds.into_iter().map(read_fd).collect()
}

#[vox::service]
trait StaxdLinuxFixture {
    async fn open_perf_session(
        &self,
        config: PerfSessionConfig,
    ) -> Result<PerfSessionFds, PerfSessionError>;

    async fn status(&self) -> DaemonStatus;
}

#[derive(Clone)]
struct StaxdFixture;

impl StaxdLinuxFixture for StaxdFixture {
    async fn open_perf_session(
        &self,
        config: PerfSessionConfig,
    ) -> Result<PerfSessionFds, PerfSessionError> {
        if config.target_pid == 0 {
            return Err(PerfSessionError::NoSuchTarget(config.target_pid));
        }

        Ok(PerfSessionFds {
            sampling: vec![
                fd_blob(format!("sampling-cpu0-pid{}", config.target_pid)),
                fd_blob(format!("sampling-cpu1-pid{}", config.target_pid)),
            ],
            switch: vec![fd_blob("switch-cpu0"), fd_blob("switch-cpu1")],
            waking: vec![fd_blob("waking-cpu0"), fd_blob("waking-cpu1")],
            waking_field_offsets: Some(WakingFieldOffsets {
                wakee_pid_offset: 16,
                wakee_pid_size: 4,
            }),
            pmu: vec![
                fd_blob("pmu-cycles-cpu0"),
                fd_blob("pmu-instructions-cpu0"),
                fd_blob("pmu-cycles-cpu1"),
                fd_blob("pmu-instructions-cpu1"),
            ],
            pmu_ids: vec![10, 11, 20, 21],
            pmu_per_cpu: 2,
            cpu_count: 2,
            page_size: 16_384,
            data_pages: 64,
            target_pid: config.target_pid,
            frequency_hz: config.frequency_hz,
            kernel_stacks: config.kernel_stacks,
        })
    }

    async fn status(&self) -> DaemonStatus {
        DaemonStatus {
            version: "1.0.0-dev".to_string(),
            host_arch: "x86_64".to_string(),
            privileged: true,
            perf_event_paranoid: 1,
        }
    }
}

async fn stax_pair() -> (StaxdLinuxFixtureClient, vox::NoopClient) {
    let (client_link, server_link) = FdStreamLink::pair().unwrap();
    let server = tokio::spawn(async move {
        vox::acceptor_on(server_link)
            .on_connection(StaxdLinuxFixtureDispatcher::new(StaxdFixture))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish")
    });
    let client = vox::initiator_on(client_link)
        .establish::<StaxdLinuxFixtureClient>()
        .await
        .expect("client establish");
    let server_guard = server.await.expect("server task");
    (client, server_guard)
}

#[tokio::test]
// r[verify transport.stream.local]
async fn stax_fd_broker_bundle_round_trips_over_fd_capable_link() {
    let (client, _server) = stax_pair().await;

    let status = client.status().await.expect("status call");
    assert_eq!(
        status,
        DaemonStatus {
            version: "1.0.0-dev".to_string(),
            host_arch: "x86_64".to_string(),
            privileged: true,
            perf_event_paranoid: 1,
        }
    );

    let fds = client
        .open_perf_session(sample_config())
        .await
        .expect("open_perf_session call");
    assert_eq!(fds.cpu_count, 2);
    assert_eq!(fds.page_size, 16_384);
    assert_eq!(fds.data_pages, 64);
    assert_eq!(fds.target_pid, 42_424);
    assert_eq!(fds.frequency_hz, 997);
    assert!(fds.kernel_stacks);
    assert_eq!(
        fds.waking_field_offsets,
        Some(WakingFieldOffsets {
            wakee_pid_offset: 16,
            wakee_pid_size: 4,
        })
    );
    assert_eq!(fds.pmu_ids, vec![10, 11, 20, 21]);
    assert_eq!(fds.pmu_per_cpu, 2);

    assert_eq!(
        read_fds(fds.sampling),
        vec!["sampling-cpu0-pid42424", "sampling-cpu1-pid42424"]
    );
    assert_eq!(read_fds(fds.switch), vec!["switch-cpu0", "switch-cpu1"]);
    assert_eq!(read_fds(fds.waking), vec!["waking-cpu0", "waking-cpu1"]);
    assert_eq!(
        read_fds(fds.pmu),
        vec![
            "pmu-cycles-cpu0",
            "pmu-instructions-cpu0",
            "pmu-cycles-cpu1",
            "pmu-instructions-cpu1",
        ]
    );
}

#[tokio::test]
// r[verify transport.stream]
// r[verify transport.fd.capability]
async fn stax_fd_broker_bundle_is_refused_by_non_fd_transport() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        vox::acceptor_on(StreamLink::tcp(sock))
            .on_connection(StaxdLinuxFixtureDispatcher::new(StaxdFixture))
            .establish::<vox::NoopClient>()
            .await
            .expect("server establish")
    });

    let client_sock = tokio::net::TcpStream::connect(addr).await.unwrap();
    let client = vox::initiator_on(StreamLink::tcp(client_sock))
        .establish::<StaxdLinuxFixtureClient>()
        .await
        .expect("client establish");
    let _server = server.await.expect("server task");

    let result = client.open_perf_session(sample_config()).await;
    assert!(
        result.is_err(),
        "TCP transport must refuse a Stax fd bundle, got {result:?}"
    );
}
