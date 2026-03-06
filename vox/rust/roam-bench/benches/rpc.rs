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
// roam zerocopy-vs-owned
// ============================================================================

mod roam_zerocopy_bench {
    use moire::task::FutureExt;
    use roam_core::{NoopCaller, acceptor, initiator, memory_link_pair};
    use roam_shm::varslot::SizeClassConfig;
    use roam_shm::{Segment, SegmentConfig, create_test_link_pair};
    use roam_stream::StreamLink;

    use shm_primitives::FileCleanup;
    use tokio::net::TcpListener;

    #[roam::service]
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
                let (_caller, _sh) = acceptor(b)
                    .establish::<NoopCaller>(ZerocopyDispatcher::new(Handler))
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let (client, _sh) = initiator(a)
            .establish::<ZerocopyClient>(())
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
                let (_caller, _sh) = acceptor(b)
                    .establish::<NoopCaller>(ZerocopyDispatcher::new(Handler))
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let (client, _sh) = initiator(a)
            .establish::<ZerocopyClient>(())
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
                let (_caller, _sh) = acceptor(StreamLink::tcp(stream))
                    .establish::<NoopCaller>(ZerocopyDispatcher::new(Handler))
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.set_nodelay(true).unwrap();

        let (client, _sh) = initiator(StreamLink::tcp(stream))
            .establish::<ZerocopyClient>(())
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
    roam_mem_zerocopy_owned_len,
    roam_mem_zerocopy_borrowed_len,
    roam_zerocopy_bench::setup_mem,
    "roam-mem"
);
define_zerocopy_transport_benches!(
    roam_tcp_zerocopy_owned_len,
    roam_tcp_zerocopy_borrowed_len,
    roam_zerocopy_bench::setup_tcp,
    "roam-tcp"
);
define_zerocopy_transport_benches!(
    roam_shm_zerocopy_owned_len,
    roam_shm_zerocopy_borrowed_len,
    roam_zerocopy_bench::setup_shm,
    "roam-shm"
);

// ============================================================================
// roam
// ============================================================================

mod roam_bench {
    use moire::task::FutureExt;
    use roam_core::{NoopCaller, acceptor, initiator, memory_link_pair};

    #[roam::service]
    pub trait Bench {
        async fn add(&self, a: i32, b: i32) -> i32;
        async fn echo(&self, data: Vec<u8>) -> Vec<u8>;
        async fn generate(&self, count: u32, output: roam::Tx<i32>);
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

        async fn generate(&self, count: u32, output: roam::Tx<i32>) {
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
                let (_caller, _sh) = acceptor(b)
                    .establish::<NoopCaller>(BenchDispatcher::new(Handler))
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let (client, _sh) = initiator(a)
            .establish::<BenchClient>(())
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }
}

#[divan::bench]
fn roam_add(bencher: divan::Bencher) {
    let client = TOKIO.block_on(roam_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client.add(3, 5).await.expect("roam add failed");
            divan::black_box(resp);
        })
    });
}

#[divan::bench(args = PAYLOAD_SIZES)]
fn roam_echo(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(roam_bench::setup());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(payload.clone())
                .await
                .expect("roam echo failed");
            divan::black_box(resp.len());
        })
    });
}

#[divan::bench(args = STREAM_COUNTS)]
fn roam_stream(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(roam_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let (tx, mut rx) = roam::channel::<i32>();
            let call = client.generate(n as u32, tx);
            let recv_task = tokio::spawn(async move {
                let mut count = 0u32;
                while let Ok(Some(_item)) = rx.recv().await {
                    count += 1;
                }
                count
            });
            call.await.expect("roam generate failed");
            let count = recv_task.await.unwrap();
            divan::black_box(count);
        })
    });
}

// ============================================================================
// roam-shm (shared memory transport)
// ============================================================================

mod roam_shm_bench {
    use std::sync::Arc;

    use moire::task::FutureExt;
    use roam_core::{NoopCaller, acceptor, initiator};
    use roam_shm::varslot::SizeClassConfig;
    use roam_shm::{Segment, SegmentConfig, create_test_link_pair};

    use shm_primitives::FileCleanup;

    use super::roam_bench::{BenchClient, BenchDispatcher, Handler};

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
                let (_caller, _sh) = acceptor(b)
                    .establish::<NoopCaller>(BenchDispatcher::new(Handler))
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let (client, _sh) = initiator(a)
            .establish::<BenchClient>(())
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }
}

#[divan::bench]
fn roam_shm_add(bencher: divan::Bencher) {
    let client = TOKIO.block_on(roam_shm_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client.add(3, 5).await.expect("roam-shm add failed");
            divan::black_box(resp);
        })
    });
}

#[divan::bench(args = PAYLOAD_SIZES)]
fn roam_shm_echo(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(roam_shm_bench::setup());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(payload.clone())
                .await
                .expect("roam-shm echo failed");
            divan::black_box(resp.len());
        })
    });
}

#[divan::bench(args = STREAM_COUNTS)]
fn roam_shm_stream(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(roam_shm_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let (tx, mut rx) = roam::channel::<i32>();
            let call = client.generate(n as u32, tx);
            let recv_task = tokio::spawn(async move {
                let mut count = 0u32;
                while let Ok(Some(_item)) = rx.recv().await {
                    count += 1;
                }
                count
            });
            call.await.expect("roam-shm generate failed");
            let count = recv_task.await.unwrap();
            divan::black_box(count);
        })
    });
}

// ============================================================================
// roam-tcp (TCP loopback transport)
// ============================================================================

mod roam_tcp_bench {
    use moire::task::FutureExt;
    use roam_core::{NoopCaller, acceptor, initiator};
    use roam_stream::StreamLink;

    use tokio::net::TcpListener;

    use super::roam_bench::{BenchClient, BenchDispatcher, Handler};

    pub async fn setup() -> BenchClient {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
        let _server_task = moire::task::spawn(
            async move {
                let (stream, _) = listener.accept().await.unwrap();
                stream.set_nodelay(true).unwrap();
                let (_caller, _sh) = acceptor(StreamLink::tcp(stream))
                    .establish::<NoopCaller>(BenchDispatcher::new(Handler))
                    .await
                    .expect("server handshake failed");
                let _ = server_ready_tx.send(());
                std::future::pending::<()>().await;
            }
            .named("server_setup"),
        );

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.set_nodelay(true).unwrap();

        let (client, _sh) = initiator(StreamLink::tcp(stream))
            .establish::<BenchClient>(())
            .await
            .expect("client handshake failed");

        server_ready_rx.await.expect("server setup failed");

        client
    }
}

#[divan::bench]
fn roam_tcp_add(bencher: divan::Bencher) {
    let client = TOKIO.block_on(roam_tcp_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client.add(3, 5).await.expect("roam-tcp add failed");
            divan::black_box(resp);
        })
    });
}

#[divan::bench(args = PAYLOAD_SIZES)]
fn roam_tcp_echo(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(roam_tcp_bench::setup());
    let payload = vec![42u8; n];
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let resp = client
                .echo(payload.clone())
                .await
                .expect("roam-tcp echo failed");
            divan::black_box(resp.len());
        })
    });
}

#[divan::bench(args = STREAM_COUNTS)]
fn roam_tcp_stream(bencher: divan::Bencher, n: usize) {
    let client = TOKIO.block_on(roam_tcp_bench::setup());
    bencher.bench_local(|| {
        TOKIO.block_on(async {
            let (tx, mut rx) = roam::channel::<i32>();
            let call = client.generate(n as u32, tx);
            let recv_task = tokio::spawn(async move {
                let mut count = 0u32;
                while let Ok(Some(_item)) = rx.recv().await {
                    count += 1;
                }
                count
            });
            call.await.expect("roam-tcp generate failed");
            let count = recv_task.await.unwrap();
            divan::black_box(count);
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
