use eyre::Result;
#[cfg(not(unix))]
use eyre::eyre;

#[cfg(unix)]
mod unix_demo {
    use std::{
        env,
        io::{Read, Write},
        os::fd::{AsRawFd, IntoRawFd},
        os::unix::net::{UnixListener, UnixStream},
        path::Path,
        process::{Child, Command},
        sync::Arc,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use eyre::{Result, WrapErr, eyre};
    use roam::DriverCaller;
    use roam_shm::bootstrap::{BootstrapStatus, decode_request, encode_request};
    use roam_shm::guest_link_from_raw;
    use roam_shm::varslot::SizeClassConfig;
    use roam_shm::{HostHub, Segment, SegmentConfig, ShmLink};
    use shm_primitives::{FileCleanup, PeerId};

    #[roam::service]
    trait Adder {
        async fn add(&self, a: i32, b: i32) -> i32;
    }

    #[roam::service]
    trait StringReverser {
        async fn reverse(&self, value: String) -> String;
    }

    #[derive(Clone, Copy)]
    struct AdderService;

    impl Adder for AdderService {
        async fn add(&self, a: i32, b: i32) -> i32 {
            a + b
        }
    }

    #[derive(Clone, Copy)]
    struct StringReverserService;

    impl StringReverser for StringReverserService {
        async fn reverse(&self, value: String) -> String {
            value.chars().rev().collect()
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum GuestService {
        Adder,
        StringReverser,
    }

    impl GuestService {
        fn as_arg(self) -> &'static str {
            match self {
                Self::Adder => "adder",
                Self::StringReverser => "reverser",
            }
        }
    }

    impl std::str::FromStr for GuestService {
        type Err = eyre::Report;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "adder" => Ok(Self::Adder),
                "reverser" => Ok(Self::StringReverser),
                _ => Err(eyre!("unknown service `{s}`")),
            }
        }
    }

    enum Role {
        Host,
        Guest(GuestService),
    }

    struct GuestProcess {
        kind: GuestService,
        child: Child,
        _tempdir: tempfile::TempDir,
    }

    struct BootstrappedGuest {
        link: ShmLink,
        process: GuestProcess,
    }

    pub fn main_unix() -> Result<()> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .wrap_err("building Tokio runtime")?;
        rt.block_on(async {
            match parse_role()? {
                Role::Host => run_host().await,
                Role::Guest(service) => run_guest(service).await,
            }
        })
    }

    fn parse_role() -> Result<Role> {
        let mut args = env::args().skip(1);
        let Some(flag) = args.next() else {
            return Ok(Role::Host);
        };

        if flag != "--role" {
            return Err(eyre!("expected `--role`, got `{flag}`"));
        }
        let role = args.next().ok_or_else(|| eyre!("missing role value"))?;

        match role.as_str() {
            "host" => Ok(Role::Host),
            "guest" => {
                let service_flag = args
                    .next()
                    .ok_or_else(|| eyre!("guest mode requires `--service <adder|reverser>`"))?;
                if service_flag != "--service" {
                    return Err(eyre!(
                        "guest mode expected `--service`, got `{service_flag}`"
                    ));
                }
                let service = args
                    .next()
                    .ok_or_else(|| eyre!("missing service value"))?
                    .parse::<GuestService>()?;
                Ok(Role::Guest(service))
            }
            _ => Err(eyre!("unknown role `{role}`")),
        }
    }

    async fn run_host() -> Result<()> {
        println!("[host:{}] starting SHM 3-process demo", std::process::id());
        println!("[host] launching guest: Adder");
        let mut adder_guest = spawn_guest(GuestService::Adder)?;
        println!("[host] guest Adder pid={}", adder_guest.process.child.id());
        let (adder, _) = roam::acceptor(adder_guest.link)
            .establish::<AdderClient>(())
            .await
            .map_err(|e| eyre!("host<->adder handshake failed: {e:?}"))?;

        println!("[host] launching guest: StringReverser");
        let mut reverser_guest = spawn_guest(GuestService::StringReverser)?;
        println!(
            "[host] guest StringReverser pid={}",
            reverser_guest.process.child.id()
        );
        let (reverser, _) = roam::acceptor(reverser_guest.link)
            .establish::<StringReverserClient>(())
            .await
            .map_err(|e| eyre!("host<->reverser handshake failed: {e:?}"))?;

        let sum = adder
            .add(40, 2)
            .await
            .map_err(|e| eyre!("adder.add failed: {e:?}"))?;
        println!("[host] Adder.add(40, 2) -> {sum}");
        assert_eq!(sum, 42);

        let reversed = reverser
            .reverse("desserts".to_string())
            .await
            .map_err(|e| eyre!("reverser.reverse failed: {e:?}"))?;
        println!("[host] StringReverser.reverse(\"desserts\") -> {reversed}");
        assert_eq!(reversed, "stressed");

        stop_guest(&mut adder_guest.process);
        stop_guest(&mut reverser_guest.process);
        println!("[host] demo complete");
        Ok(())
    }

    async fn run_guest(service: GuestService) -> Result<()> {
        let pid = std::process::id();
        println!("[guest:{pid}] bootstrapping as {:?}", service);
        let link = connect_guest_link_from_env()?;

        match service {
            GuestService::Adder => {
                let (guard, _) = roam::initiator_conduit(link)
                    .establish::<DriverCaller>(AdderDispatcher::new(AdderService))
                    .await
                    .map_err(|e| eyre!("guest adder handshake failed: {e:?}"))?;
                println!("[guest:{pid}] serving Adder");
                let _guard = guard;
                std::future::pending::<()>().await;
            }
            GuestService::StringReverser => {
                let (guard, _) = roam::initiator_conduit(link)
                    .establish::<DriverCaller>(StringReverserDispatcher::new(StringReverserService))
                    .await
                    .map_err(|e| eyre!("guest reverser handshake failed: {e:?}"))?;
                println!("[guest:{pid}] serving StringReverser");
                let _guard = guard;
                std::future::pending::<()>().await;
            }
        }
        Ok(())
    }

    fn spawn_guest(kind: GuestService) -> Result<BootstrappedGuest> {
        let tempdir = tempfile::tempdir().wrap_err("creating tempdir for guest shm assets")?;
        let sid = sid_hex_32();
        let control_sock_path = tempdir.path().join("bootstrap.sock");
        let shm_path = tempdir.path().join("guest.shm");

        let listener = UnixListener::bind(&control_sock_path).map_err(|e| {
            eyre!(
                "binding bootstrap socket {}: {e}",
                control_sock_path.display()
            )
        })?;
        listener
            .set_nonblocking(true)
            .wrap_err("setting bootstrap listener non-blocking")?;

        let size_classes = [SizeClassConfig {
            slot_size: 4096,
            slot_count: 8,
        }];
        let segment = Arc::new(
            Segment::create(
                &shm_path,
                SegmentConfig {
                    max_guests: 1,
                    bipbuf_capacity: 64 * 1024,
                    max_payload_size: 1024 * 1024,
                    inline_threshold: 256,
                    heartbeat_interval: 0,
                    size_classes: &size_classes,
                },
                FileCleanup::Manual,
            )
            .map_err(|e| eyre!("creating segment {}: {e}", shm_path.display()))?,
        );
        let hub = HostHub::new(Arc::clone(&segment));

        let hub_path = shm_path
            .to_str()
            .ok_or_else(|| eyre!("invalid UTF-8 shm path: {}", shm_path.display()))?;
        let prepared = hub
            .prepare_bootstrap_success(hub_path.as_bytes())
            .map_err(|e| eyre!("prepare bootstrap success: {e}"))?;

        let control_sock = control_sock_path
            .to_str()
            .ok_or_else(|| eyre!("invalid UTF-8 socket path: {}", control_sock_path.display()))?;
        let mut child = Command::new(env::current_exe().wrap_err("reading current executable")?)
            .arg("--role")
            .arg("guest")
            .arg("--service")
            .arg(kind.as_arg())
            .env("SHM_CONTROL_SOCK", control_sock)
            .env("SHM_SESSION_ID", &sid)
            .env("SHM_MMAP_TX_FD", prepared.guest_ticket.mmap_tx_arg())
            .spawn()
            .map_err(|e| eyre!("spawning guest process for {:?}: {e}", kind))?;

        let mut stream = wait_for_bootstrap_connection(&listener, &mut child, &control_sock_path)?;
        stream
            .set_nonblocking(false)
            .wrap_err("setting bootstrap stream blocking")?;

        let mut request_buf = [0_u8; 2048];
        let n = stream
            .read(&mut request_buf)
            .wrap_err("reading bootstrap request")?;
        if n == 0 {
            return Err(eyre!("bootstrap request EOF"));
        }
        let request = decode_request(&request_buf[..n])
            .map_err(|e| eyre!("decoding bootstrap request: {e}"))?;
        if request.sid != sid.as_bytes() {
            return Err(eyre!(
                "bootstrap sid mismatch (expected `{sid}`, got `{}`)",
                String::from_utf8_lossy(request.sid)
            ));
        }

        prepared
            .send_success_unix(stream.as_raw_fd(), &segment)
            .map_err(|e| eyre!("sending bootstrap success: {e}"))?;

        let link = prepared
            .host_peer
            .into_link()
            .map_err(|e| eyre!("converting host peer to link: {e}"))?;

        Ok(BootstrappedGuest {
            link,
            process: GuestProcess {
                kind,
                child,
                _tempdir: tempdir,
            },
        })
    }

    fn wait_for_bootstrap_connection(
        listener: &UnixListener,
        child: &mut Child,
        control_sock_path: &Path,
    ) -> Result<UnixStream> {
        let started = Instant::now();
        let timeout = Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _addr)) => return Ok(stream),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = child
                        .try_wait()
                        .map_err(|err| eyre!("checking guest status: {err}"))?
                    {
                        return Err(eyre!(
                            "guest exited before bootstrap on {}: {}",
                            control_sock_path.display(),
                            status
                        ));
                    }
                    if started.elapsed() > timeout {
                        return Err(eyre!(
                            "timed out waiting for guest bootstrap on {}",
                            control_sock_path.display()
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    return Err(eyre!(
                        "accepting bootstrap connection on {}: {e}",
                        control_sock_path.display()
                    ));
                }
            }
        }
    }

    fn connect_guest_link_from_env() -> Result<ShmLink> {
        let control_sock =
            env::var("SHM_CONTROL_SOCK").map_err(|_| eyre!("SHM_CONTROL_SOCK env var not set"))?;
        let sid =
            env::var("SHM_SESSION_ID").map_err(|_| eyre!("SHM_SESSION_ID env var not set"))?;
        let mmap_tx_fd: i32 = env::var("SHM_MMAP_TX_FD")
            .map_err(|_| eyre!("SHM_MMAP_TX_FD env var not set"))?
            .parse()
            .map_err(|e| eyre!("invalid SHM_MMAP_TX_FD: {e}"))?;
        connect_guest_link(&control_sock, &sid, mmap_tx_fd)
    }

    fn connect_guest_link(control_sock: &str, sid: &str, mmap_tx_fd: i32) -> Result<ShmLink> {
        let request = encode_request(sid.as_bytes()).map_err(|e| eyre!("encode request: {e}"))?;

        let mut stream = UnixStream::connect(control_sock)
            .map_err(|e| eyre!("connecting bootstrap socket {control_sock}: {e}"))?;
        stream
            .write_all(&request)
            .wrap_err("sending bootstrap request")?;

        let received = shm_primitives::bootstrap::recv_response_unix(stream.as_raw_fd())
            .map_err(|e| eyre!("receiving bootstrap response: {e}"))?;
        if received.response.status != BootstrapStatus::Success {
            return Err(eyre!(
                "bootstrap failed: status={:?}, payload={}",
                received.response.status,
                String::from_utf8_lossy(&received.response.payload)
            ));
        }

        let fds = received
            .fds
            .ok_or_else(|| eyre!("missing bootstrap success fds"))?;
        let hub_path = std::str::from_utf8(&received.response.payload)
            .map_err(|e| eyre!("bootstrap payload is not utf-8 path: {e}"))?;
        let segment = Arc::new(
            Segment::attach(Path::new(hub_path))
                .map_err(|e| eyre!("attach segment at {hub_path}: {e}"))?,
        );

        let peer_id = PeerId::new(received.response.peer_id as u8)
            .ok_or_else(|| eyre!("invalid peer id {}", received.response.peer_id))?;

        let doorbell_fd = fds.doorbell_fd.into_raw_fd();
        let mmap_rx_fd = fds.mmap_control_fd.into_raw_fd();

        unsafe { guest_link_from_raw(segment, peer_id, doorbell_fd, mmap_rx_fd, mmap_tx_fd, true) }
            .map_err(|e| eyre!("guest_link_from_raw: {e}"))
    }

    fn stop_guest(guest: &mut GuestProcess) {
        match guest.child.try_wait() {
            Ok(Some(status)) => {
                println!(
                    "[host] guest {:?} already exited with status {}",
                    guest.kind, status
                );
            }
            Ok(None) => {
                let _ = guest.child.kill();
                let _ = guest.child.wait();
                println!("[host] guest {:?} terminated", guest.kind);
            }
            Err(err) => {
                eprintln!("[host] failed to inspect guest {:?}: {err}", guest.kind);
            }
        }
    }

    fn sid_hex_32() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let pid = u128::from(std::process::id());
        format!("{:032x}", nanos ^ (pid << 64))
    }
}

#[cfg(unix)]
fn main() -> Result<()> {
    unix_demo::main_unix()
}

#[cfg(not(unix))]
fn main() -> Result<()> {
    Err(eyre!(
        "shm_host_two_guests example currently supports Unix only"
    ))
}
