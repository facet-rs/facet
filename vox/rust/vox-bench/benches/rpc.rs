use std::sync::LazyLock;

use tokio::runtime::Runtime;

static TOKIO: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
});

fn main() {
    divan::main();
}

const PAYLOAD_SIZES: &[usize] = &[64, 256, 1024, 4096, 65536];
const STREAM_COUNTS: &[usize] = &[10, 100, 1000];
const ZEROCOPY_PAYLOAD_SIZES: &[usize] = &[
    64, 256, 1024, 4096, 65536, 262144, 1048576, 4194304, 16777216, 67108864, 134217728,
];

// ============================================================================
// vox zerocopy-vs-owned
// ============================================================================

mod vox_zerocopy_bench {
    use moire::task::FutureExt;
    use vox_core::{NoopClient, TransportMode, acceptor_on, initiator_on, memory_link_pair};
    use vox_shm::varslot::SizeClassConfig;
    use vox_shm::{Segment, SegmentConfig, create_test_link_pair};
    use vox_stream::StreamLink;

    use shm_primitives::FileCleanup;
    use tokio::net::TcpListener;

    #[vox::service]
    pub trait Zerocopy {
        async fn owned_len(&self, data: String) -> usize;
        async fn borrowed_len(&self, data: &str) -> usize;
    }

    #[derive(Clone)]
    pub struct Handler;

    impl Zerocopy for Handler {
        async fn owned_len(&self, data: String) -> usize {
            data.len()
        }

        async fn borrowed_len(&self, data: &str) -> usize {
            data.len()
        }
    }

    pub async fn setup_mem() -> ZerocopyClient {
        let (a, b) = memory_link_pair(64);

        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let _server_task = moire::task::spawn(
            async move {
                let _caller = acceptor_on(b)
                    .on_connection(ZerocopyDispatcher::new(Handler).establish::<NoopClient>())
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let client = initiator_on(a, TransportMode::Bare)
            .establish::<ZerocopyClient>()
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }

    pub async fn setup_shm() -> ZerocopyClient {
        let classes = [
            SizeClassConfig {
                slot_size: 4096,
                slot_count: 16,
            },
            SizeClassConfig {
                slot_size: 65536 + 256,
                slot_count: 8,
            },
            SizeClassConfig {
                slot_size: 262144 + 256,
                slot_count: 4,
            },
            SizeClassConfig {
                slot_size: 1048576 + 256,
                slot_count: 2,
            },
        ];
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("zerocopy-bench.shm");
        let segment = std::sync::Arc::new(
            Segment::create(
                &path,
                SegmentConfig {
                    max_guests: 1,
                    bipbuf_capacity: 1 << 17,
                    max_payload_size: 1 << 30,
                    inline_threshold: 256,
                    heartbeat_interval: 0,
                    size_classes: &classes,
                },
                FileCleanup::Manual,
            )
            .expect("create segment"),
        );
        let (a, b) = create_test_link_pair(segment)
            .await
            .expect("create_test_link_pair");
        std::mem::forget(dir);

        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let _server_task = moire::task::spawn(
            async move {
                let _caller = acceptor_on(b)
                    .on_connection(ZerocopyDispatcher::new(Handler).establish::<NoopClient>())
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let client = initiator_on(a, TransportMode::Bare)
            .establish::<ZerocopyClient>()
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }

    pub async fn setup_tcp() -> ZerocopyClient {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let _server_task = moire::task::spawn(
            async move {
                let (stream, _) = listener.accept().await.unwrap();
                stream.set_nodelay(true).unwrap();
                let _caller = acceptor_on(StreamLink::tcp(stream))
                    .on_connection(ZerocopyDispatcher::new(Handler).establish::<NoopClient>())
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.set_nodelay(true).unwrap();

        let client = initiator_on(StreamLink::tcp(stream), TransportMode::Bare)
            .establish::<ZerocopyClient>()
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }
}

macro_rules! define_zerocopy_transport_benches {
    ($owned_name:ident, $borrowed_name:ident, $setup:path, $label:literal) => {
        #[divan::bench(args = ZEROCOPY_PAYLOAD_SIZES)]
        fn $owned_name(bencher: divan::Bencher, n: usize) {
            let client = TOKIO.block_on($setup());
            let payload = "x".repeat(n);
            bencher.bench_local(|| {
                TOKIO.block_on(async {
                    let resp = client
                        .owned_len(payload.clone())
                        .await
                        .expect(concat!($label, " owned_len failed"));
                    divan::black_box(resp);
                })
            });
        }

        #[divan::bench(args = ZEROCOPY_PAYLOAD_SIZES)]
        fn $borrowed_name(bencher: divan::Bencher, n: usize) {
            let client = TOKIO.block_on($setup());
            let payload = "x".repeat(n);
            bencher.bench_local(|| {
                TOKIO.block_on(async {
                    let resp = client
                        .borrowed_len(&payload)
                        .await
                        .expect(concat!($label, " borrowed_len failed"));
                    divan::black_box(resp);
                })
            });
        }
    };
}

define_zerocopy_transport_benches!(
    vox_mem_zerocopy_owned_len,
    vox_mem_zerocopy_borrowed_len,
    vox_zerocopy_bench::setup_mem,
    "vox-mem"
);
define_zerocopy_transport_benches!(
    vox_tcp_zerocopy_owned_len,
    vox_tcp_zerocopy_borrowed_len,
    vox_zerocopy_bench::setup_tcp,
    "vox-tcp"
);
define_zerocopy_transport_benches!(
    vox_shm_zerocopy_owned_len,
    vox_shm_zerocopy_borrowed_len,
    vox_zerocopy_bench::setup_shm,
    "vox-shm"
);

// ============================================================================
// vox
// ============================================================================

mod vox_bench {
    use moire::task::FutureExt;
    use vox_core::{NoopClient, TransportMode, acceptor_on, initiator_on, memory_link_pair};

    #[vox::service]
    pub trait Bench {
        async fn add(&self, a: i32, b: i32) -> i32;
        async fn echo(&self, data: Vec<u8>) -> Vec<u8>;
        async fn generate(&self, count: u32, output: vox::Tx<i32>);
    }

    #[derive(Clone)]
    pub struct Handler;

    impl Bench for Handler {
        async fn add(&self, a: i32, b: i32) -> i32 {
            a + b
        }

        async fn echo(&self, data: Vec<u8>) -> Vec<u8> {
            data
        }

        async fn generate(&self, count: u32, output: vox::Tx<i32>) {
            for i in 0..count as i32 {
                output.send(i).await.unwrap();
            }
            output.close(Default::default()).await.unwrap();
        }
    }

    pub async fn setup() -> BenchClient {
        let (a, b) = memory_link_pair(64);

        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let _server_task = moire::task::spawn(
            async move {
                let _caller = acceptor_on(b)
                    .on_connection(BenchDispatcher::new(Handler).establish::<NoopClient>())
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let client = initiator_on(a, TransportMode::Bare)
            .establish::<BenchClient>()
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }
}

#[divan::bench]
fn vox_add(bencher: divan::Bencher) {
    let client = TOKIO.block_on(vox_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client.add(3, 5).await.expect("vox add failed");
            divan::black_box(resp);
        })
    });
}

#[divan::bench(args = PAYLOAD_SIZES)]
fn vox_echo(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(vox_bench::setup());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client.echo(payload.clone()).await.expect("vox echo failed");
            divan::black_box(resp.len());
        })
    });
}

#[divan::bench(args = STREAM_COUNTS)]
fn vox_stream(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(vox_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let (tx, mut rx) = vox::channel::<i32>();
            let call = client.generate(n as u32, tx);
            let recv_task = tokio::spawn(async move {
                let mut count = 0u32;
                while let Ok(Some(_item)) = rx.recv().await {
                    count += 1;
                }
                count
            });
            call.await.expect("vox generate failed");
            let count = recv_task.await.unwrap();
            divan::black_box(count);
        })
    });
}

// ============================================================================
// vox-shm (shared memory transport)
// ============================================================================

mod vox_shm_bench {
    use std::sync::Arc;

    use moire::task::FutureExt;
    use vox_core::{NoopClient, TransportMode, acceptor_on, initiator_on};
    use vox_shm::varslot::SizeClassConfig;
    use vox_shm::{Segment, SegmentConfig, create_test_link_pair};

    use shm_primitives::FileCleanup;

    use super::vox_bench::{BenchClient, BenchDispatcher, Handler};

    pub async fn setup() -> BenchClient {
        let classes = [
            SizeClassConfig {
                slot_size: 4096,
                slot_count: 16,
            },
            SizeClassConfig {
                slot_size: 65536 + 256,
                slot_count: 4,
            },
        ];
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bench.shm");
        let segment = Arc::new(
            Segment::create(
                &path,
                SegmentConfig {
                    max_guests: 1,
                    bipbuf_capacity: 1 << 16,
                    max_payload_size: 1 << 20,
                    inline_threshold: 256,
                    heartbeat_interval: 0,
                    size_classes: &classes,
                },
                FileCleanup::Manual,
            )
            .expect("create segment"),
        );
        let (a, b) = create_test_link_pair(segment)
            .await
            .expect("create_test_link_pair");
        // Leak the tempdir so the segment file lives for the entire benchmark run.
        std::mem::forget(dir);

        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let _server_task = moire::task::spawn(
            async move {
                let _caller = acceptor_on(b)
                    .on_connection(BenchDispatcher::new(Handler).establish::<NoopClient>())
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let client = initiator_on(a, TransportMode::Bare)
            .establish::<BenchClient>()
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }
}

#[divan::bench]
fn vox_shm_add(bencher: divan::Bencher) {
    let client = TOKIO.block_on(vox_shm_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client.add(3, 5).await.expect("vox-shm add failed");
            divan::black_box(resp);
        })
    });
}

#[divan::bench(args = PAYLOAD_SIZES)]
fn vox_shm_echo(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(vox_shm_bench::setup());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(payload.clone())
                .await
                .expect("vox-shm echo failed");
            divan::black_box(resp.len());
        })
    });
}

#[divan::bench(args = STREAM_COUNTS)]
fn vox_shm_stream(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(vox_shm_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let (tx, mut rx) = vox::channel::<i32>();
            let call = client.generate(n as u32, tx);
            let recv_task = tokio::spawn(async move {
                let mut count = 0u32;
                while let Ok(Some(_item)) = rx.recv().await {
                    count += 1;
                }
                count
            });
            call.await.expect("vox-shm generate failed");
            let count = recv_task.await.unwrap();
            divan::black_box(count);
        })
    });
}

// ============================================================================
// vox-tcp (TCP loopback transport)
// ============================================================================

mod vox_tcp_bench {
    use moire::task::FutureExt;
    use vox_core::{NoopClient, TransportMode, acceptor_on, initiator_on};
    use vox_stream::StreamLink;

    use tokio::net::TcpListener;

    use super::vox_bench::{BenchClient, BenchDispatcher, Handler};

    pub async fn setup() -> BenchClient {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let _server_task = moire::task::spawn(
            async move {
                let (stream, _) = listener.accept().await.unwrap();
                stream.set_nodelay(true).unwrap();
                let _caller = acceptor_on(StreamLink::tcp(stream))
                    .on_connection(BenchDispatcher::new(Handler).establish::<NoopClient>())
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.set_nodelay(true).unwrap();

        let client = initiator_on(StreamLink::tcp(stream), TransportMode::Bare)
            .establish::<BenchClient>()
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }
}

#[divan::bench]
fn vox_tcp_add(bencher: divan::Bencher) {
    let client = TOKIO.block_on(vox_tcp_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client.add(3, 5).await.expect("vox-tcp add failed");
            divan::black_box(resp);
        })
    });
}

#[divan::bench(args = PAYLOAD_SIZES)]
fn vox_tcp_echo(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(vox_tcp_bench::setup());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(payload.clone())
                .await
                .expect("vox-tcp echo failed");
            divan::black_box(resp.len());
        })
    });
}

#[divan::bench(args = STREAM_COUNTS)]
fn vox_tcp_stream(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(vox_tcp_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let (tx, mut rx) = vox::channel::<i32>();
            let call = client.generate(n as u32, tx);
            let recv_task = tokio::spawn(async move {
                let mut count = 0u32;
                while let Ok(Some(_item)) = rx.recv().await {
                    count += 1;
                }
                count
            });
            call.await.expect("vox-tcp generate failed");
            let count = recv_task.await.unwrap();
            divan::black_box(count);
        })
    });
}

// ============================================================================
// serialization microbenchmark: scatter plan vs direct to_vec
// ============================================================================

mod serialize_bench {
    use std::sync::LazyLock;

    use facet::{Facet, Peek, PtrConst};
    use vox_postcard::{
        PlanInput, SchemaSet, TranslationPlan, build_plan, from_slice_borrowed_with_plan,
        peek_to_scatter_plan, to_vec,
    };
    use vox_types::{
        CborPayload, ConnectionId, Message, MessageFamily, MessagePayload, MetadataEntry,
        MetadataFlags, MetadataValue, MethodId, MsgFamily, Payload, RequestBody, RequestCall,
        RequestId, RequestMessage, SchemaRegistry, extract_schemas,
    };

    #[derive(Facet)]
    pub struct UserProfile<'a> {
        pub id: u64,
        pub username: &'a str,
        pub email: &'a str,
        pub bio: &'a str,
        pub avatar_url: &'a str,
        pub follower_count: u32,
        pub following_count: u32,
        pub is_verified: bool,
        pub tags: Vec<&'a str>,
        pub scores: Vec<f64>,
    }

    pub fn make_profile(n_tags: usize) -> UserProfile<'static> {
        static TAGS: &[&str] = &[
            "christmas",
            "gifts",
            "reindeer",
            "sleigh",
            "cookies",
            "chimney",
            "naughty",
            "nice",
            "elves",
            "workshop",
            "northpole",
            "rudolph",
            "snowflakes",
            "jinglebells",
            "mistletoe",
            "eggnog",
        ];
        UserProfile {
            id: 24121800,
            username: "santa_claus",
            email: "santa@northpole.int",
            bio: "Jolly old elf residing at the North Pole. Maintains a \
                  globally distributed naughty/nice list with eventual \
                  consistency. Expert in chimney-based ingress protocols.",
            avatar_url: "https://northpole.int/avatars/big-red-suit.jpg",
            follower_count: 4200000000,
            following_count: 0,
            is_verified: true,
            tags: TAGS.iter().copied().cycle().take(n_tags).collect(),
            scores: (0..n_tags).map(|i| i as f64 * 1.5).collect(),
        }
    }

    pub fn make_message<'a>(args: &'a (&'a UserProfile<'a>,)) -> Message<'a> {
        Message {
            connection_id: vox_types::ConnectionId(1),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: vox_types::RequestId(42),
                body: RequestBody::Call(RequestCall {
                    method_id: vox_types::MethodId(7),
                    args: Payload::outgoing(args),
                    metadata: vec![
                        MetadataEntry {
                            key: "authorization".into(),
                            value: MetadataValue::String(
                                "Bearer eyJhbGciOiJIUzI1NiJ9.e30.ZRrHA1JJJW8opB1Qfp7QDm".into(),
                            ),
                            flags: MetadataFlags::SENSITIVE,
                        },
                        MetadataEntry {
                            key: "request-id".into(),
                            value: MetadataValue::String(
                                "550e8400-e29b-41d4-a716-446655440000".into(),
                            ),
                            flags: MetadataFlags::NONE,
                        },
                    ],
                    schemas: Default::default(),
                }),
            }),
        }
    }

    pub fn bench_to_vec(msg: &Message<'_>) -> usize {
        let bytes = to_vec(msg).expect("serialize");
        bytes.len()
    }

    pub async fn bench_scatter_writev(
        msg: &Message<'_>,
        stream: &mut tokio::net::TcpStream,
    ) -> usize {
        use tokio::io::AsyncWriteExt;
        let shape = MessageFamily::shape();
        #[allow(unsafe_code)]
        let peek = unsafe {
            Peek::unchecked_new(
                PtrConst::new((msg as *const Message<'_>).cast::<u8>()),
                shape,
            )
        };
        let plan = peek_to_scatter_plan(peek).expect("scatter plan");
        let io_slices = plan.to_io_slices();
        let bufs: Vec<std::io::IoSlice<'_>> = io_slices;
        stream.write_vectored(&bufs).await.expect("writev")
    }

    pub async fn bench_to_vec_write(
        msg: &Message<'_>,
        stream: &mut tokio::net::TcpStream,
    ) -> usize {
        use tokio::io::AsyncWriteExt;
        let bytes = to_vec(msg).expect("serialize");
        stream.write_all(&bytes).await.expect("write");
        bytes.len()
    }

    /// Create a TCP loopback pair. Returns the write side; the read side is
    /// drained by a spawned task.
    pub async fn tcp_sink() -> tokio::net::TcpStream {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let writer = tokio::net::TcpStream::connect(addr).await.unwrap();
        writer.set_nodelay(true).unwrap();
        let (reader, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut reader = reader;
            let mut buf = vec![0u8; 65536];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
        writer
    }

    pub fn bench_scatter(msg: &Message<'_>) -> usize {
        let shape = MessageFamily::shape();
        #[allow(unsafe_code)]
        let peek = unsafe {
            Peek::unchecked_new(
                PtrConst::new((msg as *const Message<'_>).cast::<u8>()),
                shape,
            )
        };
        let plan = peek_to_scatter_plan(peek).expect("scatter plan");
        let mut buf = vec![0u8; plan.total_size()];
        plan.write_into(&mut buf);
        buf.len()
    }

    pub struct PlanResult {
        pub plan: TranslationPlan,
        pub registry: SchemaRegistry,
    }

    pub static MESSAGE_PLAN: LazyLock<PlanResult> = LazyLock::new(|| {
        let remote_extracted =
            extract_schemas(<MessageFamily as MsgFamily>::shape()).expect("schema extraction");
        let remote =
            SchemaSet::from_root_and_schemas(remote_extracted.root, remote_extracted.schemas);
        let local_extracted =
            extract_schemas(<MessageFamily as MsgFamily>::shape()).expect("schema extraction");
        let local = SchemaSet::from_root_and_schemas(local_extracted.root, local_extracted.schemas);
        let plan = build_plan(&PlanInput {
            remote: &remote,
            local: &local,
        })
        .expect("identity translation plan");
        PlanResult {
            plan,
            registry: remote.registry,
        }
    });

    pub struct ExactCallFixture {
        pub args: Vec<u8>,
        pub schemas: Vec<u8>,
    }

    impl ExactCallFixture {
        pub fn message(&self) -> Message<'_> {
            Message {
                connection_id: ConnectionId(1),
                payload: MessagePayload::RequestMessage(RequestMessage {
                    id: RequestId(42),
                    body: RequestBody::Call(RequestCall {
                        method_id: MethodId(7),
                        metadata: vec![
                            MetadataEntry {
                                key: "authorization".into(),
                                value: MetadataValue::String(
                                    "Bearer eyJhbGciOiJIUzI1NiJ9.e30.ZRrHA1JJJW8opB1Qfp7QDm".into(),
                                ),
                                flags: MetadataFlags::SENSITIVE,
                            },
                            MetadataEntry {
                                key: "attempt".into(),
                                value: MetadataValue::U64(3),
                                flags: MetadataFlags::NONE,
                            },
                        ],
                        args: Payload::PostcardBytes(&self.args),
                        schemas: CborPayload(self.schemas.clone()),
                    }),
                }),
            }
        }
    }

    pub fn make_exact_call_fixture(n: usize) -> ExactCallFixture {
        let args = vec![0xA5; n];
        let schemas = Vec::new();
        ExactCallFixture { args, schemas }
    }

    #[derive(Facet)]
    pub struct FastPathPayload<'a> {
        pub inode: u64,
        pub parent_inode: u64,
        pub size: u64,
        pub mode: u32,
        pub uid: u32,
        pub gid: u32,
        pub link_count: u32,
        pub mtime_ns: u64,
        pub ctime_ns: u64,
        pub name: &'a str,
        pub path: &'a str,
        pub etag: &'a [u8],
        pub checksum: &'a [u8],
        pub inline_data: &'a [u8],
    }

    type FastPathArgs<'a> = (FastPathPayload<'a>,);

    pub static PAYLOAD_PLAN: LazyLock<PlanResult> = LazyLock::new(|| {
        let remote_extracted = extract_schemas(<FastPathArgs<'static> as Facet<'static>>::SHAPE)
            .expect("schema extraction");
        let remote =
            SchemaSet::from_root_and_schemas(remote_extracted.root, remote_extracted.schemas);
        let local_extracted = extract_schemas(<FastPathArgs<'static> as Facet<'static>>::SHAPE)
            .expect("schema extraction");
        let local = SchemaSet::from_root_and_schemas(local_extracted.root, local_extracted.schemas);
        let plan = build_plan(&PlanInput {
            remote: &remote,
            local: &local,
        })
        .expect("identity translation plan");
        PlanResult {
            plan,
            registry: remote.registry,
        }
    });

    pub struct FastPathPayloadFixture {
        pub name: String,
        pub path: String,
        pub etag: Vec<u8>,
        pub checksum: Vec<u8>,
        pub inline_data: Vec<u8>,
    }

    impl FastPathPayloadFixture {
        pub fn payload(&self) -> FastPathPayload<'_> {
            FastPathPayload {
                inode: 0x10_20_30_40,
                parent_inode: 0x01_02_03_04,
                size: self.inline_data.len() as u64,
                mode: 0o100644,
                uid: 501,
                gid: 20,
                link_count: 1,
                mtime_ns: 1_735_689_123_456_789,
                ctime_ns: 1_735_689_123_400_000,
                name: &self.name,
                path: &self.path,
                etag: &self.etag,
                checksum: &self.checksum,
                inline_data: &self.inline_data,
            }
        }
    }

    pub fn make_fast_path_payload_fixture(n: usize) -> FastPathPayloadFixture {
        FastPathPayloadFixture {
            name: format!("inode-{n}"),
            path: format!("/bench/very/realistic/path/for/{n}/payload.bin"),
            etag: vec![0x11; 16],
            checksum: vec![0x22; 32],
            inline_data: vec![0x5A; n],
        }
    }

    #[derive(Facet)]
    pub struct GnarlyAttrBorrowed<'a> {
        pub key: &'a str,
        pub value: &'a str,
    }

    #[derive(Facet)]
    #[repr(u8)]
    pub enum GnarlyKindBorrowed<'a> {
        File {
            mime: &'a str,
            tags: Vec<&'a str>,
        },
        Directory {
            child_count: u32,
            children: Vec<&'a str>,
        },
        Symlink {
            target: &'a str,
            hops: Vec<u32>,
        },
    }

    #[derive(Facet)]
    pub struct GnarlyEntryBorrowed<'a> {
        pub id: u64,
        pub parent: Option<u64>,
        pub name: &'a str,
        pub path: &'a str,
        pub attrs: Vec<GnarlyAttrBorrowed<'a>>,
        pub chunks: Vec<&'a [u8]>,
        pub kind: GnarlyKindBorrowed<'a>,
    }

    #[derive(Facet)]
    pub struct GnarlyPayloadBorrowed<'a> {
        pub revision: u64,
        pub mount: &'a str,
        pub entries: Vec<GnarlyEntryBorrowed<'a>>,
        pub footer: Option<&'a str>,
        pub digest: &'a [u8],
    }

    #[derive(Facet)]
    pub struct GnarlyAttrOwned {
        pub key: String,
        pub value: String,
    }

    #[derive(Facet)]
    #[repr(u8)]
    pub enum GnarlyKindOwned {
        File {
            mime: String,
            tags: Vec<String>,
        },
        Directory {
            child_count: u32,
            children: Vec<String>,
        },
        Symlink {
            target: String,
            hops: Vec<u32>,
        },
    }

    #[derive(Facet)]
    pub struct GnarlyEntryOwned {
        pub id: u64,
        pub parent: Option<u64>,
        pub name: String,
        pub path: String,
        pub attrs: Vec<GnarlyAttrOwned>,
        pub chunks: Vec<Vec<u8>>,
        pub kind: GnarlyKindOwned,
    }

    #[derive(Facet)]
    pub struct GnarlyPayloadOwned {
        pub revision: u64,
        pub mount: String,
        pub entries: Vec<GnarlyEntryOwned>,
        pub footer: Option<String>,
        pub digest: Vec<u8>,
    }

    type GnarlyBorrowedArgs<'a> = (GnarlyPayloadBorrowed<'a>,);

    pub static GNARLY_BORROWED_PLAN: LazyLock<PlanResult> = LazyLock::new(|| {
        let remote_extracted =
            extract_schemas(<GnarlyBorrowedArgs<'static> as Facet<'static>>::SHAPE)
                .expect("schema extraction");
        let remote =
            SchemaSet::from_root_and_schemas(remote_extracted.root, remote_extracted.schemas);
        let local_extracted =
            extract_schemas(<GnarlyBorrowedArgs<'static> as Facet<'static>>::SHAPE)
                .expect("schema extraction");
        let local = SchemaSet::from_root_and_schemas(local_extracted.root, local_extracted.schemas);
        let plan = build_plan(&PlanInput {
            remote: &remote,
            local: &local,
        })
        .expect("identity translation plan");
        PlanResult {
            plan,
            registry: remote.registry,
        }
    });

    pub struct GnarlyEntryFixture {
        pub id: u64,
        pub parent: Option<u64>,
        pub name: String,
        pub path: String,
        pub attrs: Vec<(String, String)>,
        pub chunks: Vec<Vec<u8>>,
        pub kind: GnarlyEntryKindFixture,
    }

    pub enum GnarlyEntryKindFixture {
        File {
            mime: String,
            tags: Vec<String>,
        },
        Directory {
            child_count: u32,
            children: Vec<String>,
        },
        Symlink {
            target: String,
            hops: Vec<u32>,
        },
    }

    pub struct GnarlyFixture {
        pub mount: String,
        pub footer: Option<String>,
        pub digest: Vec<u8>,
        pub entries: Vec<GnarlyEntryFixture>,
    }

    impl GnarlyFixture {
        pub fn borrowed_payload(&self) -> GnarlyPayloadBorrowed<'_> {
            GnarlyPayloadBorrowed {
                revision: 7,
                mount: &self.mount,
                entries: self
                    .entries
                    .iter()
                    .map(|entry| GnarlyEntryBorrowed {
                        id: entry.id,
                        parent: entry.parent,
                        name: &entry.name,
                        path: &entry.path,
                        attrs: entry
                            .attrs
                            .iter()
                            .map(|(key, value)| GnarlyAttrBorrowed {
                                key: key.as_str(),
                                value: value.as_str(),
                            })
                            .collect(),
                        chunks: entry.chunks.iter().map(Vec::as_slice).collect(),
                        kind: match &entry.kind {
                            GnarlyEntryKindFixture::File { mime, tags } => {
                                GnarlyKindBorrowed::File {
                                    mime,
                                    tags: tags.iter().map(String::as_str).collect(),
                                }
                            }
                            GnarlyEntryKindFixture::Directory {
                                child_count,
                                children,
                            } => GnarlyKindBorrowed::Directory {
                                child_count: *child_count,
                                children: children.iter().map(String::as_str).collect(),
                            },
                            GnarlyEntryKindFixture::Symlink { target, hops } => {
                                GnarlyKindBorrowed::Symlink {
                                    target,
                                    hops: hops.clone(),
                                }
                            }
                        },
                    })
                    .collect(),
                footer: self.footer.as_deref(),
                digest: &self.digest,
            }
        }
    }

    pub fn make_gnarly_fixture(entry_count: usize) -> GnarlyFixture {
        let entries = (0..entry_count)
            .map(|i| {
                let chunks = (0..3)
                    .map(|j| vec![((i + j) & 0xFF) as u8; 32 * (j + 1)])
                    .collect();
                let attrs = vec![
                    ("owner".to_string(), format!("user-{i}")),
                    ("class".to_string(), format!("hot-path-{i}")),
                    ("etag".to_string(), format!("etag-{i:08x}")),
                ];
                let kind = match i % 3 {
                    0 => GnarlyEntryKindFixture::File {
                        mime: "application/octet-stream".to_string(),
                        tags: vec![
                            "warm".to_string(),
                            "cacheable".to_string(),
                            format!("tag-{i}"),
                        ],
                    },
                    1 => GnarlyEntryKindFixture::Directory {
                        child_count: (i as u32) + 3,
                        children: vec![
                            format!("child-{i}-0"),
                            format!("child-{i}-1"),
                            format!("child-{i}-2"),
                        ],
                    },
                    _ => GnarlyEntryKindFixture::Symlink {
                        target: format!("/target/{i}/nested/item"),
                        hops: vec![1, 2, 3, i as u32],
                    },
                };
                GnarlyEntryFixture {
                    id: 1000 + i as u64,
                    parent: if i == 0 { None } else { Some(999 + i as u64) },
                    name: format!("entry-{i}"),
                    path: format!("/mount/very/deep/path/with/component/{i}/file.bin"),
                    attrs,
                    chunks,
                    kind,
                }
            })
            .collect();
        GnarlyFixture {
            mount: "/mnt/bench-fast-path".to_string(),
            footer: Some("benchmark footer".to_string()),
            digest: vec![0x44; 64],
            entries,
        }
    }

    pub fn postcard_plan_encode(msg: &Message<'_>) -> usize {
        let msg = divan::black_box(msg);
        to_vec(msg).expect("serialize").len()
    }

    pub fn postcard_plan_decode(input: &[u8]) -> usize {
        let input = divan::black_box(input);
        let msg: Message<'_> =
            from_slice_borrowed_with_plan(input, &MESSAGE_PLAN.plan, &MESSAGE_PLAN.registry)
                .expect("plan decode");
        score_message(&msg)
    }

    pub fn postcard_plan_roundtrip(msg: &Message<'_>) -> usize {
        let msg = divan::black_box(msg);
        let bytes = to_vec(msg).expect("serialize");
        postcard_plan_decode(divan::black_box(&bytes))
    }

    pub fn postcard_payload_message_encode(fixture: &FastPathPayloadFixture) -> usize {
        encode_postcard_payload_message(divan::black_box(fixture)).len()
    }

    pub fn postcard_payload_message_decode(input: &[u8]) -> usize {
        let input = divan::black_box(input);
        let msg: Message<'_> =
            from_slice_borrowed_with_plan(input, &MESSAGE_PLAN.plan, &MESSAGE_PLAN.registry)
                .expect("plan decode");
        score_message_and_payload(&msg)
    }

    pub fn postcard_payload_message_roundtrip(fixture: &FastPathPayloadFixture) -> usize {
        let bytes = encode_postcard_payload_message(divan::black_box(fixture));
        postcard_payload_message_decode(divan::black_box(&bytes))
    }

    pub fn postcard_payload_message_roundtrip_scan(fixture: &FastPathPayloadFixture) -> usize {
        let bytes = encode_postcard_payload_message(divan::black_box(fixture));
        let input = divan::black_box(&bytes);
        let msg: Message<'_> =
            from_slice_borrowed_with_plan(input, &MESSAGE_PLAN.plan, &MESSAGE_PLAN.registry)
                .expect("plan decode");
        score_message_and_payload_scan(&msg)
    }

    pub fn postcard_gnarly_borrowed_roundtrip(fixture: &GnarlyFixture) -> usize {
        let bytes = encode_postcard_gnarly_message(divan::black_box(fixture));
        let input = divan::black_box(&bytes);
        let msg: Message<'_> =
            from_slice_borrowed_with_plan(input, &MESSAGE_PLAN.plan, &MESSAGE_PLAN.registry)
                .expect("plan decode");
        score_message_and_gnarly_borrowed(&msg)
    }

    pub fn postcard_gnarly_owned_roundtrip(fixture: &GnarlyFixture) -> usize {
        let bytes = encode_postcard_gnarly_message(divan::black_box(fixture));
        let input = divan::black_box(&bytes);
        let msg: Message<'_> =
            from_slice_borrowed_with_plan(input, &MESSAGE_PLAN.plan, &MESSAGE_PLAN.registry)
                .expect("plan decode");
        score_message_and_gnarly_owned(&msg)
    }

    fn score_message(msg: &Message<'_>) -> usize {
        let mut score = msg.connection_id.0 as usize;
        if let MessagePayload::RequestMessage(RequestMessage {
            id,
            body:
                RequestBody::Call(RequestCall {
                    method_id,
                    metadata,
                    args,
                    schemas,
                }),
        }) = &msg.payload
        {
            score ^= id.0 as usize;
            score ^= method_id.0 as usize;
            score ^= metadata.len();
            for entry in metadata {
                score = score.wrapping_add(entry.key.len());
                score = score.wrapping_add(entry.flags.flags_score());
                score = score.wrapping_add(match &entry.value {
                    MetadataValue::String(s) => s.len(),
                    MetadataValue::Bytes(b) => b.len(),
                    MetadataValue::U64(v) => *v as usize,
                });
            }
            score = score.wrapping_add(match args {
                Payload::PostcardBytes(bytes) => bytes.len(),
                Payload::Value { .. } => 0,
            });
            score = score.wrapping_add(schemas.0.len());
        } else {
            panic!("expected request call message");
        }
        score
    }

    fn score_message_and_payload(msg: &Message<'_>) -> usize {
        let mut score = score_message(msg);
        if let MessagePayload::RequestMessage(RequestMessage {
            body:
                RequestBody::Call(RequestCall {
                    args: Payload::PostcardBytes(bytes),
                    ..
                }),
            ..
        }) = &msg.payload
        {
            let (payload,): FastPathArgs<'_> =
                from_slice_borrowed_with_plan(bytes, &PAYLOAD_PLAN.plan, &PAYLOAD_PLAN.registry)
                    .expect("payload decode");
            score = score.wrapping_add(score_fast_path_payload(&payload));
        } else {
            panic!("expected request call payload bytes");
        }
        score
    }

    fn score_message_and_payload_scan(msg: &Message<'_>) -> usize {
        let mut score = score_message(msg);
        if let MessagePayload::RequestMessage(RequestMessage {
            body:
                RequestBody::Call(RequestCall {
                    args: Payload::PostcardBytes(bytes),
                    ..
                }),
            ..
        }) = &msg.payload
        {
            let (payload,): FastPathArgs<'_> =
                from_slice_borrowed_with_plan(bytes, &PAYLOAD_PLAN.plan, &PAYLOAD_PLAN.registry)
                    .expect("payload decode");
            score = score.wrapping_add(score_fast_path_payload_scan(&payload));
        } else {
            panic!("expected request call payload bytes");
        }
        score
    }

    fn score_message_and_gnarly_borrowed(msg: &Message<'_>) -> usize {
        let mut score = score_message(msg);
        if let MessagePayload::RequestMessage(RequestMessage {
            body:
                RequestBody::Call(RequestCall {
                    args: Payload::PostcardBytes(bytes),
                    ..
                }),
            ..
        }) = &msg.payload
        {
            let (payload,): GnarlyBorrowedArgs<'_> = from_slice_borrowed_with_plan(
                bytes,
                &GNARLY_BORROWED_PLAN.plan,
                &GNARLY_BORROWED_PLAN.registry,
            )
            .expect("gnarly borrowed decode");
            score = score.wrapping_add(score_gnarly_borrowed(&payload));
        } else {
            panic!("expected request call payload bytes");
        }
        score
    }

    fn score_message_and_gnarly_owned(msg: &Message<'_>) -> usize {
        let mut score = score_message(msg);
        if let MessagePayload::RequestMessage(RequestMessage {
            body:
                RequestBody::Call(RequestCall {
                    args: Payload::PostcardBytes(bytes),
                    ..
                }),
            ..
        }) = &msg.payload
        {
            let (payload,): GnarlyBorrowedArgs<'_> = from_slice_borrowed_with_plan(
                bytes,
                &GNARLY_BORROWED_PLAN.plan,
                &GNARLY_BORROWED_PLAN.registry,
            )
            .expect("gnarly borrowed decode");
            let owned = materialize_gnarly_owned(&payload);
            score = score.wrapping_add(score_gnarly_owned(&owned));
        } else {
            panic!("expected request call payload bytes");
        }
        score
    }

    fn score_fast_path_payload(payload: &FastPathPayload<'_>) -> usize {
        let mut score = payload.inode as usize
            ^ payload.parent_inode as usize
            ^ payload.size as usize
            ^ payload.mode as usize
            ^ payload.uid as usize
            ^ payload.gid as usize
            ^ payload.link_count as usize
            ^ payload.mtime_ns as usize
            ^ payload.ctime_ns as usize;
        score = score.wrapping_add(payload.name.len());
        score = score.wrapping_add(payload.path.len());
        score = score.wrapping_add(payload.etag.len());
        score = score.wrapping_add(payload.checksum.len());
        score = score.wrapping_add(payload.inline_data.len());
        score
    }

    fn score_fast_path_payload_scan(payload: &FastPathPayload<'_>) -> usize {
        let mut score = score_fast_path_payload(payload);
        for byte in payload.inline_data {
            score = score.wrapping_add(*byte as usize);
        }
        score
    }

    fn score_gnarly_borrowed(payload: &GnarlyPayloadBorrowed<'_>) -> usize {
        let mut score = payload.revision as usize
            ^ payload.mount.len()
            ^ payload.digest.len()
            ^ payload.footer.map_or(0, str::len);
        for entry in &payload.entries {
            score = score.wrapping_add(entry.id as usize);
            score = score.wrapping_add(entry.parent.unwrap_or_default() as usize);
            score = score.wrapping_add(entry.name.len());
            score = score.wrapping_add(entry.path.len());
            for attr in &entry.attrs {
                score = score.wrapping_add(attr.key.len() ^ attr.value.len());
            }
            for chunk in &entry.chunks {
                score = score.wrapping_add(chunk.len());
            }
            score = score.wrapping_add(match &entry.kind {
                GnarlyKindBorrowed::File { mime, tags } => {
                    mime.len() + tags.iter().map(|tag| tag.len()).sum::<usize>()
                }
                GnarlyKindBorrowed::Directory {
                    child_count,
                    children,
                } => {
                    *child_count as usize + children.iter().map(|child| child.len()).sum::<usize>()
                }
                GnarlyKindBorrowed::Symlink { target, hops } => {
                    target.len() + hops.iter().copied().map(|hop| hop as usize).sum::<usize>()
                }
            });
        }
        score
    }

    fn score_gnarly_owned(payload: &GnarlyPayloadOwned) -> usize {
        let mut score = payload.revision as usize
            ^ payload.mount.len()
            ^ payload.digest.len()
            ^ payload.footer.as_ref().map_or(0, String::len);
        for entry in &payload.entries {
            score = score.wrapping_add(entry.id as usize);
            score = score.wrapping_add(entry.parent.unwrap_or_default() as usize);
            score = score.wrapping_add(entry.name.len());
            score = score.wrapping_add(entry.path.len());
            for attr in &entry.attrs {
                score = score.wrapping_add(attr.key.len() ^ attr.value.len());
            }
            for chunk in &entry.chunks {
                score = score.wrapping_add(chunk.len());
            }
            score = score.wrapping_add(match &entry.kind {
                GnarlyKindOwned::File { mime, tags } => {
                    mime.len() + tags.iter().map(String::len).sum::<usize>()
                }
                GnarlyKindOwned::Directory {
                    child_count,
                    children,
                } => *child_count as usize + children.iter().map(String::len).sum::<usize>(),
                GnarlyKindOwned::Symlink { target, hops } => {
                    target.len() + hops.iter().copied().map(|hop| hop as usize).sum::<usize>()
                }
            });
        }
        score
    }

    fn materialize_gnarly_owned(payload: &GnarlyPayloadBorrowed<'_>) -> GnarlyPayloadOwned {
        GnarlyPayloadOwned {
            revision: payload.revision,
            mount: payload.mount.to_string(),
            entries: payload
                .entries
                .iter()
                .map(|entry| GnarlyEntryOwned {
                    id: entry.id,
                    parent: entry.parent,
                    name: entry.name.to_string(),
                    path: entry.path.to_string(),
                    attrs: entry
                        .attrs
                        .iter()
                        .map(|attr| GnarlyAttrOwned {
                            key: attr.key.to_string(),
                            value: attr.value.to_string(),
                        })
                        .collect(),
                    chunks: entry.chunks.iter().map(|chunk| chunk.to_vec()).collect(),
                    kind: match &entry.kind {
                        GnarlyKindBorrowed::File { mime, tags } => GnarlyKindOwned::File {
                            mime: mime.to_string(),
                            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
                        },
                        GnarlyKindBorrowed::Directory {
                            child_count,
                            children,
                        } => GnarlyKindOwned::Directory {
                            child_count: *child_count,
                            children: children.iter().map(|child| (*child).to_string()).collect(),
                        },
                        GnarlyKindBorrowed::Symlink { target, hops } => GnarlyKindOwned::Symlink {
                            target: target.to_string(),
                            hops: hops.clone(),
                        },
                    },
                })
                .collect(),
            footer: payload.footer.map(str::to_string),
            digest: payload.digest.to_vec(),
        }
    }

    pub fn encode_postcard_payload_message(fixture: &FastPathPayloadFixture) -> Vec<u8> {
        let payload = fixture.payload();
        let args = (payload,);
        let msg = Message {
            connection_id: ConnectionId(1),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(42),
                body: RequestBody::Call(RequestCall {
                    method_id: MethodId(7),
                    metadata: vec![
                        MetadataEntry {
                            key: "authorization".into(),
                            value: MetadataValue::String(
                                "Bearer eyJhbGciOiJIUzI1NiJ9.e30.ZRrHA1JJJW8opB1Qfp7QDm".into(),
                            ),
                            flags: MetadataFlags::SENSITIVE,
                        },
                        MetadataEntry {
                            key: "attempt".into(),
                            value: MetadataValue::U64(3),
                            flags: MetadataFlags::NONE,
                        },
                    ],
                    args: Payload::outgoing(&args),
                    schemas: CborPayload::default(),
                }),
            }),
        };
        to_vec(&msg).expect("serialize")
    }

    pub fn encode_postcard_gnarly_message(fixture: &GnarlyFixture) -> Vec<u8> {
        let payload = fixture.borrowed_payload();
        let args = (payload,);
        let msg = Message {
            connection_id: ConnectionId(1),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: RequestId(42),
                body: RequestBody::Call(RequestCall {
                    method_id: MethodId(7),
                    metadata: vec![
                        MetadataEntry {
                            key: "authorization".into(),
                            value: MetadataValue::String(
                                "Bearer eyJhbGciOiJIUzI1NiJ9.e30.ZRrHA1JJJW8opB1Qfp7QDm".into(),
                            ),
                            flags: MetadataFlags::SENSITIVE,
                        },
                        MetadataEntry {
                            key: "attempt".into(),
                            value: MetadataValue::U64(3),
                            flags: MetadataFlags::NONE,
                        },
                    ],
                    args: Payload::outgoing(&args),
                    schemas: CborPayload::default(),
                }),
            }),
        };
        to_vec(&msg).expect("serialize")
    }

    trait MetadataFlagsScore {
        fn flags_score(self) -> usize;
    }

    impl MetadataFlagsScore for MetadataFlags {
        fn flags_score(self) -> usize {
            if self == MetadataFlags::NONE { 0 } else { 1 }
        }
    }

    const TAG_REQUEST_MESSAGE: u8 = 5;
    const TAG_REQUEST_CALL: u8 = 0;
    const TAG_METADATA_STRING: u8 = 0;
    const TAG_METADATA_BYTES: u8 = 1;
    const TAG_METADATA_U64: u8 = 2;

    pub fn exact_layout_encode(msg: &Message<'_>) -> usize {
        let msg = divan::black_box(msg);
        encode_exact_layout(msg).len()
    }

    pub fn exact_layout_encode_bytes(msg: &Message<'_>) -> Vec<u8> {
        let msg = divan::black_box(msg);
        encode_exact_layout(msg)
    }

    pub fn exact_layout_decode(input: &[u8]) -> usize {
        let input = divan::black_box(input);
        decode_exact_layout_score(input)
    }

    pub fn exact_layout_roundtrip(msg: &Message<'_>) -> usize {
        let msg = divan::black_box(msg);
        let bytes = encode_exact_layout(msg);
        decode_exact_layout_score(divan::black_box(&bytes))
    }

    pub fn exact_payload_message_encode(fixture: &FastPathPayloadFixture) -> usize {
        encode_exact_payload_message(divan::black_box(fixture)).len()
    }

    pub fn exact_payload_message_decode(input: &[u8]) -> usize {
        let input = divan::black_box(input);
        decode_exact_layout_payload_score(input)
    }

    pub fn exact_payload_message_roundtrip(fixture: &FastPathPayloadFixture) -> usize {
        let bytes = encode_exact_payload_message(divan::black_box(fixture));
        decode_exact_layout_payload_score(divan::black_box(&bytes))
    }

    pub fn exact_payload_message_roundtrip_scan(fixture: &FastPathPayloadFixture) -> usize {
        let bytes = encode_exact_payload_message(divan::black_box(fixture));
        decode_exact_layout_payload_score_scan(divan::black_box(&bytes))
    }

    pub fn exact_gnarly_borrowed_roundtrip(fixture: &GnarlyFixture) -> usize {
        let bytes = encode_exact_gnarly_message(divan::black_box(fixture));
        decode_exact_gnarly_borrowed_score(divan::black_box(&bytes))
    }

    pub fn exact_gnarly_owned_roundtrip(fixture: &GnarlyFixture) -> usize {
        let bytes = encode_exact_gnarly_message(divan::black_box(fixture));
        let owned = decode_exact_gnarly_owned(divan::black_box(&bytes));
        score_gnarly_owned(&owned)
    }

    fn encode_exact_layout(msg: &Message<'_>) -> Vec<u8> {
        let MessagePayload::RequestMessage(RequestMessage {
            id,
            body:
                RequestBody::Call(RequestCall {
                    method_id,
                    metadata,
                    args,
                    schemas,
                }),
        }) = &msg.payload
        else {
            panic!("expected request call message");
        };

        let Payload::PostcardBytes(arg_bytes) = args else {
            panic!("expected borrowed postcard bytes");
        };

        let mut out = Vec::with_capacity(
            128 + arg_bytes.len()
                + schemas.0.len()
                + metadata.iter().map(metadata_entry_size).sum::<usize>(),
        );
        push_u64(&mut out, msg.connection_id.0);
        push_u8(&mut out, TAG_REQUEST_MESSAGE);
        push_u64(&mut out, id.0);
        push_u8(&mut out, TAG_REQUEST_CALL);
        push_u64(&mut out, method_id.0);
        push_u32(&mut out, metadata.len() as u32);
        for entry in metadata {
            push_bytes(&mut out, entry.key.as_bytes());
            match &entry.value {
                MetadataValue::String(s) => {
                    push_u8(&mut out, TAG_METADATA_STRING);
                    push_bytes(&mut out, s.as_bytes());
                }
                MetadataValue::Bytes(bytes) => {
                    push_u8(&mut out, TAG_METADATA_BYTES);
                    push_bytes(&mut out, bytes);
                }
                MetadataValue::U64(v) => {
                    push_u8(&mut out, TAG_METADATA_U64);
                    push_u64(&mut out, *v);
                }
            }
            let flags = if entry.flags == MetadataFlags::NONE {
                0
            } else {
                1
            };
            push_u64(&mut out, flags);
        }
        push_bytes(&mut out, arg_bytes);
        push_bytes(&mut out, &schemas.0);
        out
    }

    fn decode_exact_layout_score(input: &[u8]) -> usize {
        let mut cursor = 0usize;
        let connection_id = read_u64(input, &mut cursor);
        let payload_tag = read_u8(input, &mut cursor);
        assert_eq!(payload_tag, TAG_REQUEST_MESSAGE);
        let request_id = read_u64(input, &mut cursor);
        let body_tag = read_u8(input, &mut cursor);
        assert_eq!(body_tag, TAG_REQUEST_CALL);
        let method_id = read_u64(input, &mut cursor);
        let metadata_count = read_u32(input, &mut cursor) as usize;

        let mut score =
            connection_id as usize ^ request_id as usize ^ method_id as usize ^ metadata_count;
        for _ in 0..metadata_count {
            let key = read_len_prefixed(input, &mut cursor);
            score = score.wrapping_add(key.len());
            let value_tag = read_u8(input, &mut cursor);
            score = score.wrapping_add(match value_tag {
                TAG_METADATA_STRING | TAG_METADATA_BYTES => {
                    read_len_prefixed(input, &mut cursor).len()
                }
                TAG_METADATA_U64 => read_u64(input, &mut cursor) as usize,
                other => panic!("unexpected metadata tag {other}"),
            });
            score = score.wrapping_add(read_u64(input, &mut cursor) as usize);
        }

        let args = read_len_prefixed(input, &mut cursor);
        let schemas = read_len_prefixed(input, &mut cursor);
        score = score.wrapping_add(args.len());
        score = score.wrapping_add(schemas.len());
        assert_eq!(cursor, input.len());
        score
    }

    pub fn encode_exact_payload_message(fixture: &FastPathPayloadFixture) -> Vec<u8> {
        let payload = fixture.payload();
        let mut payload_bytes = Vec::with_capacity(
            96 + payload.name.len()
                + payload.path.len()
                + payload.etag.len()
                + payload.checksum.len()
                + payload.inline_data.len(),
        );
        push_u64(&mut payload_bytes, payload.inode);
        push_u64(&mut payload_bytes, payload.parent_inode);
        push_u64(&mut payload_bytes, payload.size);
        push_u32(&mut payload_bytes, payload.mode);
        push_u32(&mut payload_bytes, payload.uid);
        push_u32(&mut payload_bytes, payload.gid);
        push_u32(&mut payload_bytes, payload.link_count);
        push_u64(&mut payload_bytes, payload.mtime_ns);
        push_u64(&mut payload_bytes, payload.ctime_ns);
        push_bytes(&mut payload_bytes, payload.name.as_bytes());
        push_bytes(&mut payload_bytes, payload.path.as_bytes());
        push_bytes(&mut payload_bytes, payload.etag);
        push_bytes(&mut payload_bytes, payload.checksum);
        push_bytes(&mut payload_bytes, payload.inline_data);

        let mut out =
            Vec::with_capacity(128 + payload_bytes.len() + 2 * metadata_entry_size_hint() + 4);
        push_u64(&mut out, 1);
        push_u8(&mut out, TAG_REQUEST_MESSAGE);
        push_u64(&mut out, 42);
        push_u8(&mut out, TAG_REQUEST_CALL);
        push_u64(&mut out, 7);
        push_u32(&mut out, 2);
        push_bytes(&mut out, b"authorization");
        push_u8(&mut out, TAG_METADATA_STRING);
        push_bytes(
            &mut out,
            b"Bearer eyJhbGciOiJIUzI1NiJ9.e30.ZRrHA1JJJW8opB1Qfp7QDm",
        );
        push_u64(&mut out, 1);
        push_bytes(&mut out, b"attempt");
        push_u8(&mut out, TAG_METADATA_U64);
        push_u64(&mut out, 3);
        push_u64(&mut out, 0);
        push_bytes(&mut out, &payload_bytes);
        push_bytes(&mut out, &[]);
        out
    }

    fn decode_exact_layout_payload_score(input: &[u8]) -> usize {
        let mut cursor = 0usize;
        let connection_id = read_u64(input, &mut cursor);
        let payload_tag = read_u8(input, &mut cursor);
        assert_eq!(payload_tag, TAG_REQUEST_MESSAGE);
        let request_id = read_u64(input, &mut cursor);
        let body_tag = read_u8(input, &mut cursor);
        assert_eq!(body_tag, TAG_REQUEST_CALL);
        let method_id = read_u64(input, &mut cursor);
        let metadata_count = read_u32(input, &mut cursor) as usize;

        let mut score =
            connection_id as usize ^ request_id as usize ^ method_id as usize ^ metadata_count;
        for _ in 0..metadata_count {
            let key = read_len_prefixed(input, &mut cursor);
            score = score.wrapping_add(key.len());
            let value_tag = read_u8(input, &mut cursor);
            score = score.wrapping_add(match value_tag {
                TAG_METADATA_STRING | TAG_METADATA_BYTES => {
                    read_len_prefixed(input, &mut cursor).len()
                }
                TAG_METADATA_U64 => read_u64(input, &mut cursor) as usize,
                other => panic!("unexpected metadata tag {other}"),
            });
            score = score.wrapping_add(read_u64(input, &mut cursor) as usize);
        }

        let payload = read_len_prefixed(input, &mut cursor);
        score = score.wrapping_add(decode_exact_payload_score(payload));
        let schemas = read_len_prefixed(input, &mut cursor);
        score = score.wrapping_add(schemas.len());
        assert_eq!(cursor, input.len());
        score
    }

    fn decode_exact_layout_payload_score_scan(input: &[u8]) -> usize {
        let mut cursor = 0usize;
        let connection_id = read_u64(input, &mut cursor);
        let payload_tag = read_u8(input, &mut cursor);
        assert_eq!(payload_tag, TAG_REQUEST_MESSAGE);
        let request_id = read_u64(input, &mut cursor);
        let body_tag = read_u8(input, &mut cursor);
        assert_eq!(body_tag, TAG_REQUEST_CALL);
        let method_id = read_u64(input, &mut cursor);
        let metadata_count = read_u32(input, &mut cursor) as usize;

        let mut score =
            connection_id as usize ^ request_id as usize ^ method_id as usize ^ metadata_count;
        for _ in 0..metadata_count {
            let key = read_len_prefixed(input, &mut cursor);
            score = score.wrapping_add(key.len());
            let value_tag = read_u8(input, &mut cursor);
            score = score.wrapping_add(match value_tag {
                TAG_METADATA_STRING | TAG_METADATA_BYTES => {
                    read_len_prefixed(input, &mut cursor).len()
                }
                TAG_METADATA_U64 => read_u64(input, &mut cursor) as usize,
                other => panic!("unexpected metadata tag {other}"),
            });
            score = score.wrapping_add(read_u64(input, &mut cursor) as usize);
        }

        let payload = read_len_prefixed(input, &mut cursor);
        score = score.wrapping_add(decode_exact_payload_score_scan(payload));
        let schemas = read_len_prefixed(input, &mut cursor);
        score = score.wrapping_add(schemas.len());
        assert_eq!(cursor, input.len());
        score
    }

    fn decode_exact_payload_score(input: &[u8]) -> usize {
        let mut cursor = 0usize;
        let inode = read_u64(input, &mut cursor);
        let parent_inode = read_u64(input, &mut cursor);
        let size = read_u64(input, &mut cursor);
        let mode = read_u32(input, &mut cursor);
        let uid = read_u32(input, &mut cursor);
        let gid = read_u32(input, &mut cursor);
        let link_count = read_u32(input, &mut cursor);
        let mtime_ns = read_u64(input, &mut cursor);
        let ctime_ns = read_u64(input, &mut cursor);
        let name = read_len_prefixed(input, &mut cursor);
        let path = read_len_prefixed(input, &mut cursor);
        let etag = read_len_prefixed(input, &mut cursor);
        let checksum = read_len_prefixed(input, &mut cursor);
        let inline_data = read_len_prefixed(input, &mut cursor);
        assert_eq!(cursor, input.len());
        inode as usize
            ^ parent_inode as usize
            ^ size as usize
            ^ mode as usize
            ^ uid as usize
            ^ gid as usize
            ^ link_count as usize
            ^ mtime_ns as usize
            ^ ctime_ns as usize
            ^ name.len()
            ^ path.len()
            ^ etag.len()
            ^ checksum.len()
            ^ inline_data.len()
    }

    fn decode_exact_payload_score_scan(input: &[u8]) -> usize {
        let mut cursor = 0usize;
        let inode = read_u64(input, &mut cursor);
        let parent_inode = read_u64(input, &mut cursor);
        let size = read_u64(input, &mut cursor);
        let mode = read_u32(input, &mut cursor);
        let uid = read_u32(input, &mut cursor);
        let gid = read_u32(input, &mut cursor);
        let link_count = read_u32(input, &mut cursor);
        let mtime_ns = read_u64(input, &mut cursor);
        let ctime_ns = read_u64(input, &mut cursor);
        let name = read_len_prefixed(input, &mut cursor);
        let path = read_len_prefixed(input, &mut cursor);
        let etag = read_len_prefixed(input, &mut cursor);
        let checksum = read_len_prefixed(input, &mut cursor);
        let inline_data = read_len_prefixed(input, &mut cursor);
        assert_eq!(cursor, input.len());
        let mut score = inode as usize
            ^ parent_inode as usize
            ^ size as usize
            ^ mode as usize
            ^ uid as usize
            ^ gid as usize
            ^ link_count as usize
            ^ mtime_ns as usize
            ^ ctime_ns as usize
            ^ name.len()
            ^ path.len()
            ^ etag.len()
            ^ checksum.len()
            ^ inline_data.len();
        for byte in inline_data {
            score = score.wrapping_add(*byte as usize);
        }
        score
    }

    fn encode_exact_gnarly_message(fixture: &GnarlyFixture) -> Vec<u8> {
        let mut payload_bytes = Vec::new();
        push_u64(&mut payload_bytes, 7);
        push_bytes(&mut payload_bytes, fixture.mount.as_bytes());
        push_u32(&mut payload_bytes, fixture.entries.len() as u32);
        for entry in &fixture.entries {
            push_u64(&mut payload_bytes, entry.id);
            match entry.parent {
                Some(parent) => {
                    push_u8(&mut payload_bytes, 1);
                    push_u64(&mut payload_bytes, parent);
                }
                None => push_u8(&mut payload_bytes, 0),
            }
            push_bytes(&mut payload_bytes, entry.name.as_bytes());
            push_bytes(&mut payload_bytes, entry.path.as_bytes());
            push_u32(&mut payload_bytes, entry.attrs.len() as u32);
            for (key, value) in &entry.attrs {
                push_bytes(&mut payload_bytes, key.as_bytes());
                push_bytes(&mut payload_bytes, value.as_bytes());
            }
            push_u32(&mut payload_bytes, entry.chunks.len() as u32);
            for chunk in &entry.chunks {
                push_bytes(&mut payload_bytes, chunk);
            }
            match &entry.kind {
                GnarlyEntryKindFixture::File { mime, tags } => {
                    push_u8(&mut payload_bytes, 0);
                    push_bytes(&mut payload_bytes, mime.as_bytes());
                    push_u32(&mut payload_bytes, tags.len() as u32);
                    for tag in tags {
                        push_bytes(&mut payload_bytes, tag.as_bytes());
                    }
                }
                GnarlyEntryKindFixture::Directory {
                    child_count,
                    children,
                } => {
                    push_u8(&mut payload_bytes, 1);
                    push_u32(&mut payload_bytes, *child_count);
                    push_u32(&mut payload_bytes, children.len() as u32);
                    for child in children {
                        push_bytes(&mut payload_bytes, child.as_bytes());
                    }
                }
                GnarlyEntryKindFixture::Symlink { target, hops } => {
                    push_u8(&mut payload_bytes, 2);
                    push_bytes(&mut payload_bytes, target.as_bytes());
                    push_u32(&mut payload_bytes, hops.len() as u32);
                    for hop in hops {
                        push_u32(&mut payload_bytes, *hop);
                    }
                }
            }
        }
        match &fixture.footer {
            Some(footer) => {
                push_u8(&mut payload_bytes, 1);
                push_bytes(&mut payload_bytes, footer.as_bytes());
            }
            None => push_u8(&mut payload_bytes, 0),
        }
        push_bytes(&mut payload_bytes, &fixture.digest);

        let mut out = Vec::with_capacity(128 + payload_bytes.len());
        push_u64(&mut out, 1);
        push_u8(&mut out, TAG_REQUEST_MESSAGE);
        push_u64(&mut out, 42);
        push_u8(&mut out, TAG_REQUEST_CALL);
        push_u64(&mut out, 7);
        push_u32(&mut out, 2);
        push_bytes(&mut out, b"authorization");
        push_u8(&mut out, TAG_METADATA_STRING);
        push_bytes(
            &mut out,
            b"Bearer eyJhbGciOiJIUzI1NiJ9.e30.ZRrHA1JJJW8opB1Qfp7QDm",
        );
        push_u64(&mut out, 1);
        push_bytes(&mut out, b"attempt");
        push_u8(&mut out, TAG_METADATA_U64);
        push_u64(&mut out, 3);
        push_u64(&mut out, 0);
        push_bytes(&mut out, &payload_bytes);
        push_bytes(&mut out, &[]);
        out
    }

    fn decode_exact_gnarly_borrowed_score(input: &[u8]) -> usize {
        let mut cursor = 0usize;
        let connection_id = read_u64(input, &mut cursor);
        let payload_tag = read_u8(input, &mut cursor);
        assert_eq!(payload_tag, TAG_REQUEST_MESSAGE);
        let request_id = read_u64(input, &mut cursor);
        let body_tag = read_u8(input, &mut cursor);
        assert_eq!(body_tag, TAG_REQUEST_CALL);
        let method_id = read_u64(input, &mut cursor);
        let metadata_count = read_u32(input, &mut cursor) as usize;
        let mut score =
            connection_id as usize ^ request_id as usize ^ method_id as usize ^ metadata_count;
        for _ in 0..metadata_count {
            let key = read_len_prefixed(input, &mut cursor);
            score = score.wrapping_add(key.len());
            let value_tag = read_u8(input, &mut cursor);
            score = score.wrapping_add(match value_tag {
                TAG_METADATA_STRING | TAG_METADATA_BYTES => {
                    read_len_prefixed(input, &mut cursor).len()
                }
                TAG_METADATA_U64 => read_u64(input, &mut cursor) as usize,
                other => panic!("unexpected metadata tag {other}"),
            });
            score = score.wrapping_add(read_u64(input, &mut cursor) as usize);
        }
        let payload = read_len_prefixed(input, &mut cursor);
        score = score.wrapping_add(decode_exact_gnarly_payload_borrowed_score(payload));
        let schemas = read_len_prefixed(input, &mut cursor);
        score = score.wrapping_add(schemas.len());
        assert_eq!(cursor, input.len());
        score
    }

    fn decode_exact_gnarly_payload_borrowed_score(input: &[u8]) -> usize {
        let mut cursor = 0usize;
        let revision = read_u64(input, &mut cursor);
        let mount = read_len_prefixed(input, &mut cursor);
        let entry_count = read_u32(input, &mut cursor) as usize;
        let mut score = revision as usize ^ mount.len() ^ entry_count;
        for _ in 0..entry_count {
            score = score.wrapping_add(read_u64(input, &mut cursor) as usize);
            let has_parent = read_u8(input, &mut cursor);
            if has_parent != 0 {
                score = score.wrapping_add(read_u64(input, &mut cursor) as usize);
            }
            score = score.wrapping_add(read_len_prefixed(input, &mut cursor).len());
            score = score.wrapping_add(read_len_prefixed(input, &mut cursor).len());
            let attr_count = read_u32(input, &mut cursor) as usize;
            for _ in 0..attr_count {
                score = score.wrapping_add(read_len_prefixed(input, &mut cursor).len());
                score = score.wrapping_add(read_len_prefixed(input, &mut cursor).len());
            }
            let chunk_count = read_u32(input, &mut cursor) as usize;
            for _ in 0..chunk_count {
                score = score.wrapping_add(read_len_prefixed(input, &mut cursor).len());
            }
            let kind_tag = read_u8(input, &mut cursor);
            score = score.wrapping_add(match kind_tag {
                0 => {
                    let mime = read_len_prefixed(input, &mut cursor).len();
                    let tag_count = read_u32(input, &mut cursor) as usize;
                    let tags = (0..tag_count)
                        .map(|_| read_len_prefixed(input, &mut cursor).len())
                        .sum::<usize>();
                    mime + tags
                }
                1 => {
                    let child_count = read_u32(input, &mut cursor) as usize;
                    let n = read_u32(input, &mut cursor) as usize;
                    let children = (0..n)
                        .map(|_| read_len_prefixed(input, &mut cursor).len())
                        .sum::<usize>();
                    child_count + children
                }
                2 => {
                    let target = read_len_prefixed(input, &mut cursor).len();
                    let n = read_u32(input, &mut cursor) as usize;
                    let hops = (0..n)
                        .map(|_| read_u32(input, &mut cursor) as usize)
                        .sum::<usize>();
                    target + hops
                }
                other => panic!("unexpected gnarly tag {other}"),
            });
        }
        let footer_tag = read_u8(input, &mut cursor);
        if footer_tag != 0 {
            score = score.wrapping_add(read_len_prefixed(input, &mut cursor).len());
        }
        score = score.wrapping_add(read_len_prefixed(input, &mut cursor).len());
        assert_eq!(cursor, input.len());
        score
    }

    fn decode_exact_gnarly_owned(input: &[u8]) -> GnarlyPayloadOwned {
        let mut cursor = 0usize;
        let _connection_id = read_u64(input, &mut cursor);
        let payload_tag = read_u8(input, &mut cursor);
        assert_eq!(payload_tag, TAG_REQUEST_MESSAGE);
        let _request_id = read_u64(input, &mut cursor);
        let body_tag = read_u8(input, &mut cursor);
        assert_eq!(body_tag, TAG_REQUEST_CALL);
        let _method_id = read_u64(input, &mut cursor);
        let metadata_count = read_u32(input, &mut cursor) as usize;
        for _ in 0..metadata_count {
            let _ = read_len_prefixed(input, &mut cursor);
            let value_tag = read_u8(input, &mut cursor);
            match value_tag {
                TAG_METADATA_STRING | TAG_METADATA_BYTES => {
                    let _ = read_len_prefixed(input, &mut cursor);
                }
                TAG_METADATA_U64 => {
                    let _ = read_u64(input, &mut cursor);
                }
                other => panic!("unexpected metadata tag {other}"),
            }
            let _ = read_u64(input, &mut cursor);
        }
        let payload = read_len_prefixed(input, &mut cursor);
        let owned = decode_exact_gnarly_payload_owned(payload);
        let _schemas = read_len_prefixed(input, &mut cursor);
        assert_eq!(cursor, input.len());
        owned
    }

    fn decode_exact_gnarly_payload_owned(input: &[u8]) -> GnarlyPayloadOwned {
        let mut cursor = 0usize;
        let revision = read_u64(input, &mut cursor);
        let mount =
            String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec()).expect("utf8");
        let entry_count = read_u32(input, &mut cursor) as usize;
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let id = read_u64(input, &mut cursor);
            let parent = if read_u8(input, &mut cursor) == 0 {
                None
            } else {
                Some(read_u64(input, &mut cursor))
            };
            let name =
                String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec()).expect("utf8");
            let path =
                String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec()).expect("utf8");
            let attr_count = read_u32(input, &mut cursor) as usize;
            let mut attrs = Vec::with_capacity(attr_count);
            for _ in 0..attr_count {
                let key = String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec())
                    .expect("utf8");
                let value = String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec())
                    .expect("utf8");
                attrs.push(GnarlyAttrOwned { key, value });
            }
            let chunk_count = read_u32(input, &mut cursor) as usize;
            let mut chunks = Vec::with_capacity(chunk_count);
            for _ in 0..chunk_count {
                chunks.push(read_len_prefixed(input, &mut cursor).to_vec());
            }
            let kind = match read_u8(input, &mut cursor) {
                0 => {
                    let mime = String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec())
                        .expect("utf8");
                    let tag_count = read_u32(input, &mut cursor) as usize;
                    let mut tags = Vec::with_capacity(tag_count);
                    for _ in 0..tag_count {
                        tags.push(
                            String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec())
                                .expect("utf8"),
                        );
                    }
                    GnarlyKindOwned::File { mime, tags }
                }
                1 => {
                    let child_count = read_u32(input, &mut cursor);
                    let n = read_u32(input, &mut cursor) as usize;
                    let mut children = Vec::with_capacity(n);
                    for _ in 0..n {
                        children.push(
                            String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec())
                                .expect("utf8"),
                        );
                    }
                    GnarlyKindOwned::Directory {
                        child_count,
                        children,
                    }
                }
                2 => {
                    let target = String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec())
                        .expect("utf8");
                    let n = read_u32(input, &mut cursor) as usize;
                    let mut hops = Vec::with_capacity(n);
                    for _ in 0..n {
                        hops.push(read_u32(input, &mut cursor));
                    }
                    GnarlyKindOwned::Symlink { target, hops }
                }
                other => panic!("unexpected gnarly tag {other}"),
            };
            entries.push(GnarlyEntryOwned {
                id,
                parent,
                name,
                path,
                attrs,
                chunks,
                kind,
            });
        }
        let footer = if read_u8(input, &mut cursor) == 0 {
            None
        } else {
            Some(String::from_utf8(read_len_prefixed(input, &mut cursor).to_vec()).expect("utf8"))
        };
        let digest = read_len_prefixed(input, &mut cursor).to_vec();
        assert_eq!(cursor, input.len());
        GnarlyPayloadOwned {
            revision,
            mount,
            entries,
            footer,
            digest,
        }
    }

    fn metadata_entry_size(entry: &MetadataEntry<'_>) -> usize {
        4 + entry.key.len()
            + 1
            + match &entry.value {
                MetadataValue::String(s) => 4 + s.len(),
                MetadataValue::Bytes(bytes) => 4 + bytes.len(),
                MetadataValue::U64(_) => 8,
            }
            + 8
    }

    const fn metadata_entry_size_hint() -> usize {
        64
    }

    fn push_u8(out: &mut Vec<u8>, value: u8) {
        out.push(value);
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u64(out: &mut Vec<u8>, value: u64) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
        push_u32(out, bytes.len() as u32);
        out.extend_from_slice(bytes);
    }

    fn read_u8(input: &[u8], cursor: &mut usize) -> u8 {
        let value = input[*cursor];
        *cursor += 1;
        value
    }

    fn read_u32(input: &[u8], cursor: &mut usize) -> u32 {
        let start = *cursor;
        let end = start + 4;
        *cursor = end;
        u32::from_le_bytes(input[start..end].try_into().expect("u32 bytes"))
    }

    fn read_u64(input: &[u8], cursor: &mut usize) -> u64 {
        let start = *cursor;
        let end = start + 8;
        *cursor = end;
        u64::from_le_bytes(input[start..end].try_into().expect("u64 bytes"))
    }

    fn read_len_prefixed<'a>(input: &'a [u8], cursor: &mut usize) -> &'a [u8] {
        let len = read_u32(input, cursor) as usize;
        let start = *cursor;
        let end = start + len;
        *cursor = end;
        &input[start..end]
    }

    // --- Blob benchmarks: struct with a large &[u8] payload ---

    #[derive(Facet)]
    pub struct BlobMessage<'a> {
        pub id: u64,
        pub label: &'a str,
        pub data: &'a [u8],
    }

    pub fn make_blob<'a>(data: &'a [u8]) -> BlobMessage<'a> {
        BlobMessage {
            id: 1,
            label: "frame-data",
            data,
        }
    }

    pub fn make_blob_msg<'a>(args: &'a (&'a BlobMessage<'a>,)) -> Message<'a> {
        Message {
            connection_id: vox_types::ConnectionId(1),
            payload: MessagePayload::RequestMessage(RequestMessage {
                id: vox_types::RequestId(1),
                body: RequestBody::Call(RequestCall {
                    method_id: vox_types::MethodId(1),
                    args: Payload::outgoing(args),
                    metadata: vec![],
                    schemas: Default::default(),
                }),
            }),
        }
    }
}

const SERIALIZE_FIELD_COUNTS: &[usize] = &[4, 16, 64, 256];
const BLOB_SIZES: &[usize] = &[256, 1024, 4096, 8192, 16384, 32768, 65536, 262144, 1048576];
const EXACT_LAYOUT_ARGS_SIZES: &[usize] = &[64, 256, 4096, 65536];
const GNARLY_ENTRY_COUNTS: &[usize] = &[2, 8, 32];

#[divan::bench(args = SERIALIZE_FIELD_COUNTS)]
fn serialize_to_vec(bencher: divan::Bencher, n: usize) {
    let profile = serialize_bench::make_profile(n);
    let args = (&profile,);
    let msg = serialize_bench::make_message(&args);
    bencher.bench_local(|| divan::black_box(serialize_bench::bench_to_vec(&msg)));
}

#[divan::bench(args = SERIALIZE_FIELD_COUNTS)]
fn serialize_scatter_plan(bencher: divan::Bencher, n: usize) {
    let profile = serialize_bench::make_profile(n);
    let args = (&profile,);
    let msg = serialize_bench::make_message(&args);
    bencher.bench_local(|| divan::black_box(serialize_bench::bench_scatter(&msg)));
}

#[divan::bench(args = SERIALIZE_FIELD_COUNTS)]
fn serialize_scatter_writev_tcp(bencher: divan::Bencher, n: usize) {
    let profile = serialize_bench::make_profile(n);
    let args = (&profile,);
    let msg = serialize_bench::make_message(&args);
    let mut stream = TOKIO.block_on(serialize_bench::tcp_sink());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            divan::black_box(serialize_bench::bench_scatter_writev(&msg, &mut stream).await)
        })
    });
}

#[divan::bench(args = SERIALIZE_FIELD_COUNTS)]
fn serialize_to_vec_write_tcp(bencher: divan::Bencher, n: usize) {
    let profile = serialize_bench::make_profile(n);
    let args = (&profile,);
    let msg = serialize_bench::make_message(&args);
    let mut stream = TOKIO.block_on(serialize_bench::tcp_sink());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            divan::black_box(serialize_bench::bench_to_vec_write(&msg, &mut stream).await)
        })
    });
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_postcard_plan_encode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_exact_call_fixture(n);
    let msg = fixture.message();
    bencher.bench_local(|| divan::black_box(serialize_bench::postcard_plan_encode(&msg)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_postcard_plan_decode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_exact_call_fixture(n);
    let msg = fixture.message();
    let bytes = vox_postcard::to_vec(&msg).expect("serialize");
    bencher.bench_local(|| divan::black_box(serialize_bench::postcard_plan_decode(&bytes)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_postcard_plan_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_exact_call_fixture(n);
    let msg = fixture.message();
    bencher.bench_local(|| divan::black_box(serialize_bench::postcard_plan_roundtrip(&msg)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_exact_layout_encode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_exact_call_fixture(n);
    let msg = fixture.message();
    bencher.bench_local(|| divan::black_box(serialize_bench::exact_layout_encode(&msg)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_exact_layout_decode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_exact_call_fixture(n);
    let msg = fixture.message();
    let bytes = serialize_bench::exact_layout_encode_bytes(&msg);
    bencher.bench_local(|| divan::black_box(serialize_bench::exact_layout_decode(&bytes)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_exact_layout_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_exact_call_fixture(n);
    let msg = fixture.message();
    bencher.bench_local(|| divan::black_box(serialize_bench::exact_layout_roundtrip(&msg)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_postcard_plan_encode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::postcard_payload_message_encode(&fixture))
    });
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_postcard_plan_decode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    let bytes = serialize_bench::encode_postcard_payload_message(&fixture);
    bencher
        .bench_local(|| divan::black_box(serialize_bench::postcard_payload_message_decode(&bytes)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_postcard_plan_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::postcard_payload_message_roundtrip(
            &fixture,
        ))
    });
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_exact_layout_encode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    bencher
        .bench_local(|| divan::black_box(serialize_bench::exact_payload_message_encode(&fixture)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_exact_layout_decode(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    let bytes = serialize_bench::encode_exact_payload_message(&fixture);
    bencher.bench_local(|| divan::black_box(serialize_bench::exact_payload_message_decode(&bytes)));
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_exact_layout_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::exact_payload_message_roundtrip(&fixture))
    });
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_postcard_plan_roundtrip_scan(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::postcard_payload_message_roundtrip_scan(
            &fixture,
        ))
    });
}

#[divan::bench(args = EXACT_LAYOUT_ARGS_SIZES)]
fn message_payload_exact_layout_roundtrip_scan(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_fast_path_payload_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::exact_payload_message_roundtrip_scan(
            &fixture,
        ))
    });
}

#[divan::bench(args = GNARLY_ENTRY_COUNTS)]
fn message_gnarly_postcard_borrowed_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_gnarly_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::postcard_gnarly_borrowed_roundtrip(
            &fixture,
        ))
    });
}

#[divan::bench(args = GNARLY_ENTRY_COUNTS)]
fn message_gnarly_postcard_owned_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_gnarly_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::postcard_gnarly_owned_roundtrip(&fixture))
    });
}

#[divan::bench(args = GNARLY_ENTRY_COUNTS)]
fn message_gnarly_exact_borrowed_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_gnarly_fixture(n);
    bencher.bench_local(|| {
        divan::black_box(serialize_bench::exact_gnarly_borrowed_roundtrip(&fixture))
    });
}

#[divan::bench(args = GNARLY_ENTRY_COUNTS)]
fn message_gnarly_exact_owned_roundtrip(bencher: divan::Bencher, n: usize) {
    let fixture = serialize_bench::make_gnarly_fixture(n);
    bencher
        .bench_local(|| divan::black_box(serialize_bench::exact_gnarly_owned_roundtrip(&fixture)));
}

// --- Blob benchmarks: large binary payload ---

#[divan::bench(args = BLOB_SIZES)]
fn blob_to_vec(bencher: divan::Bencher, n: usize) {
    let blob = vec![0xCAu8; n];
    let bm = serialize_bench::make_blob(&blob);
    let args = (&bm,);
    let msg = serialize_bench::make_blob_msg(&args);
    bencher.bench_local(|| divan::black_box(serialize_bench::bench_to_vec(&msg)));
}

#[divan::bench(args = BLOB_SIZES)]
fn blob_scatter_plan(bencher: divan::Bencher, n: usize) {
    let blob = vec![0xCAu8; n];
    let bm = serialize_bench::make_blob(&blob);
    let args = (&bm,);
    let msg = serialize_bench::make_blob_msg(&args);
    bencher.bench_local(|| divan::black_box(serialize_bench::bench_scatter(&msg)));
}

#[divan::bench(args = BLOB_SIZES)]
fn blob_scatter_writev_tcp(bencher: divan::Bencher, n: usize) {
    let blob = vec![0xCAu8; n];
    let bm = serialize_bench::make_blob(&blob);
    let args = (&bm,);
    let msg = serialize_bench::make_blob_msg(&args);
    let mut stream = TOKIO.block_on(serialize_bench::tcp_sink());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            divan::black_box(serialize_bench::bench_scatter_writev(&msg, &mut stream).await)
        })
    });
}

#[divan::bench(args = BLOB_SIZES)]
fn blob_to_vec_write_tcp(bencher: divan::Bencher, n: usize) {
    let blob = vec![0xCAu8; n];
    let bm = serialize_bench::make_blob(&blob);
    let args = (&bm,);
    let msg = serialize_bench::make_blob_msg(&args);
    let mut stream = TOKIO.block_on(serialize_bench::tcp_sink());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            divan::black_box(serialize_bench::bench_to_vec_write(&msg, &mut stream).await)
        })
    });
}

// ============================================================================
// tarpc
// ============================================================================

mod tarpc_bench {
    use futures_util::StreamExt;
    use tarpc::server::Channel;

    #[tarpc::service]
    pub trait Bench {
        async fn add(a: i32, b: i32) -> i32;
        async fn echo(data: Vec<u8>) -> Vec<u8>;
    }

    #[derive(Clone)]
    struct Handler;

    impl Bench for Handler {
        async fn add(self, _ctx: tarpc::context::Context, a: i32, b: i32) -> i32 {
            a + b
        }

        async fn echo(self, _ctx: tarpc::context::Context, data: Vec<u8>) -> Vec<u8> {
            data
        }
    }

    pub async fn setup() -> BenchClient {
        let (client_transport, server_transport) = tarpc::transport::channel::unbounded();

        tokio::spawn(async move {
            let incoming = tarpc::server::BaseChannel::with_defaults(server_transport)
                .execute(Handler.serve());
            tokio::pin!(incoming);
            while let Some(handler) = incoming.next().await {
                tokio::spawn(handler);
            }
        });

        BenchClient::new(tarpc::client::Config::default(), client_transport).spawn()
    }
}

#[divan::bench]
fn tarpc_add(bencher: divan::Bencher) {
    let client = TOKIO.block_on(tarpc_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .add(tarpc::context::current(), 3, 5)
                .await
                .expect("tarpc add failed");
            divan::black_box(resp);
        })
    });
}

#[divan::bench(args = PAYLOAD_SIZES)]
fn tarpc_echo(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(tarpc_bench::setup());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(tarpc::context::current(), payload.clone())
                .await
                .expect("tarpc echo failed");
            divan::black_box(resp.len());
        })
    });
}

// ============================================================================
// tonic (in-memory via DuplexStream)
// ============================================================================

#[cfg(feature = "protobuf")]
mod tonic_bench {
    pub mod pb {
        tonic::include_proto!("adder");
    }

    use hyper_util::rt::TokioIo;
    use pb::{
        AddRequest, AddResponse, EchoRequest, EchoResponse, GenerateRequest, Number,
        adder_client::AdderClient,
        adder_server::{Adder, AdderServer},
    };
    use tokio_stream::wrappers::ReceiverStream;
    use tonic::{
        Request, Response, Status,
        transport::{Endpoint, Server, Uri},
    };
    use tower::service_fn;

    #[derive(Default)]
    struct Handler;

    #[tonic::async_trait]
    impl Adder for Handler {
        async fn add(&self, request: Request<AddRequest>) -> Result<Response<AddResponse>, Status> {
            let req = request.into_inner();
            Ok(Response::new(AddResponse {
                result: req.a + req.b,
            }))
        }

        async fn echo(
            &self,
            request: Request<EchoRequest>,
        ) -> Result<Response<EchoResponse>, Status> {
            let req = request.into_inner();
            Ok(Response::new(EchoResponse { data: req.data }))
        }

        type GenerateStream = ReceiverStream<Result<Number, Status>>;

        async fn generate(
            &self,
            request: Request<GenerateRequest>,
        ) -> Result<Response<Self::GenerateStream>, Status> {
            let count = request.into_inner().count;
            let (tx, rx) = tokio::sync::mpsc::channel(128);
            tokio::spawn(async move {
                for i in 0..count {
                    if tx.send(Ok(Number { value: i })).await.is_err() {
                        break;
                    }
                }
            });
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    pub async fn setup_duplex() -> AdderClient<tonic::transport::Channel> {
        let (client, server) = tokio::io::duplex(1024);

        tokio::spawn(async move {
            Server::builder()
                .add_service(AdderServer::new(Handler))
                .serve_with_incoming(tokio_stream::once(Ok::<_, std::io::Error>(server)))
                .await
                .unwrap();
        });

        let mut client = Some(client);
        let channel = Endpoint::try_from("http://[::]:50051")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                let client = client.take();
                async move {
                    client
                        .map(TokioIo::new)
                        .ok_or_else(|| std::io::Error::other("client already taken"))
                }
            }))
            .await
            .unwrap();

        AdderClient::new(channel)
    }

    pub async fn setup_tcp() -> AdderClient<tonic::transport::Channel> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            stream.set_nodelay(true).unwrap();
            Server::builder()
                .add_service(AdderServer::new(Handler))
                .serve_with_incoming(tokio_stream::once(Ok::<_, std::io::Error>(stream)))
                .await
                .unwrap();
        });

        let channel = Endpoint::try_from(format!("http://{addr}"))
            .unwrap()
            .tcp_nodelay(true)
            .connect()
            .await
            .unwrap();

        AdderClient::new(channel)
    }
}

#[cfg(feature = "protobuf")]
#[divan::bench]
fn tonic_add(bencher: divan::Bencher) {
    let mut client = TOKIO.block_on(tonic_bench::setup_duplex());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .add(tonic_bench::pb::AddRequest { a: 3, b: 5 })
                .await
                .expect("tonic add failed")
                .into_inner();
            divan::black_box(resp.result);
        })
    });
}

#[cfg(feature = "protobuf")]
#[divan::bench(args = PAYLOAD_SIZES)]
fn tonic_echo(bencher: divan::Bencher, n: usize) {
    let mut client = TOKIO.block_on(tonic_bench::setup_duplex());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(tonic_bench::pb::EchoRequest {
                    data: payload.clone(),
                })
                .await
                .expect("tonic echo failed")
                .into_inner();
            divan::black_box(resp.data.len());
        })
    });
}

#[cfg(feature = "protobuf")]
#[divan::bench(args = STREAM_COUNTS)]
fn tonic_stream(bencher: divan::Bencher, n: usize) {
    let mut client = TOKIO.block_on(tonic_bench::setup_duplex());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            use tokio_stream::StreamExt;
            let mut stream = client
                .generate(tonic_bench::pb::GenerateRequest { count: n as i32 })
                .await
                .expect("tonic generate failed")
                .into_inner();
            let mut count = 0u32;
            while let Some(Ok(_)) = stream.next().await {
                count += 1;
            }
            divan::black_box(count);
        })
    });
}

// ============================================================================
// tonic-tcp (TCP loopback)
// ============================================================================

#[cfg(feature = "protobuf")]
#[divan::bench]
fn tonic_tcp_add(bencher: divan::Bencher) {
    let mut client = TOKIO.block_on(tonic_bench::setup_tcp());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .add(tonic_bench::pb::AddRequest { a: 3, b: 5 })
                .await
                .expect("tonic-tcp add failed")
                .into_inner();
            divan::black_box(resp.result);
        })
    });
}

#[cfg(feature = "protobuf")]
#[divan::bench(args = PAYLOAD_SIZES)]
fn tonic_tcp_echo(bencher: divan::Bencher, n: usize) {
    let mut client = TOKIO.block_on(tonic_bench::setup_tcp());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(tonic_bench::pb::EchoRequest {
                    data: payload.clone(),
                })
                .await
                .expect("tonic-tcp echo failed")
                .into_inner();
            divan::black_box(resp.data.len());
        })
    });
}

#[cfg(feature = "protobuf")]
#[divan::bench(args = STREAM_COUNTS)]
fn tonic_tcp_stream(bencher: divan::Bencher, n: usize) {
    let mut client = TOKIO.block_on(tonic_bench::setup_tcp());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            use tokio_stream::StreamExt;
            let mut stream = client
                .generate(tonic_bench::pb::GenerateRequest { count: n as i32 })
                .await
                .expect("tonic-tcp generate failed")
                .into_inner();
            let mut count = 0u32;
            while let Some(Ok(_)) = stream.next().await {
                count += 1;
            }
            divan::black_box(count);
        })
    });
}
