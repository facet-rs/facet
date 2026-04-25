use std::ffi::OsString;
use std::future::pending;
use std::sync::{Mutex, MutexGuard};

use divan::{Bencher, black_box};
use facet::Facet;
use spec_proto::{GnarlyPayload, TestbedClient, TestbedDispatcher};
use subject_rust::TestbedService;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use vox::transport::tcp::StreamLink;
use vox::{TransportMode, initiator_on, memory_link_pair};
use vox_bench::{jit_decode, jit_encode, make_gnarly_payload};
use vox_types::VoxError;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    divan::main();
}

struct RpcHarness {
    rt: Runtime,
    client: TestbedClient,
    server_task: JoinHandle<()>,
}

impl RpcHarness {
    fn mem() -> Self {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        let (client, server_task) = rt.block_on(async {
            let (client_link, server_link) = memory_link_pair(64);
            let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();

            let server_task = tokio::spawn(async move {
                let root = vox::acceptor_on(server_link)
                    .on_connection(TestbedDispatcher::new(TestbedService))
                    .establish::<vox::NoopClient>()
                    .await
                    .map_err(|e| format!("server establish: {e}"));

                match root {
                    Ok(root) => {
                        let _ = ready_tx.send(Ok(()));
                        let _root = root;
                        pending::<()>().await;
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                    }
                }
            });

            let client = initiator_on(client_link, TransportMode::Bare)
                .establish::<TestbedClient>()
                .await
                .expect("client establish");

            ready_rx.await.expect("server ready").expect("server setup");
            (client, server_task)
        });

        Self {
            rt,
            client,
            server_task,
        }
    }

    fn tcp() -> Self {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        let (client, server_task) = rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let addr = listener.local_addr().expect("local_addr");
            let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();

            let server_task = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.expect("accept");
                stream.set_nodelay(true).expect("set_nodelay");
                let root = vox::acceptor_on(StreamLink::tcp(stream))
                    .on_connection(TestbedDispatcher::new(TestbedService))
                    .establish::<vox::NoopClient>()
                    .await
                    .map_err(|e| format!("server establish: {e}"));

                match root {
                    Ok(root) => {
                        let _ = ready_tx.send(Ok(()));
                        let _root = root;
                        pending::<()>().await;
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                    }
                }
            });

            let client_stream = TcpStream::connect(addr).await.expect("connect");
            client_stream.set_nodelay(true).expect("set_nodelay");

            let client = initiator_on(StreamLink::tcp(client_stream), TransportMode::Bare)
                .establish::<TestbedClient>()
                .await
                .expect("client establish");

            ready_rx.await.expect("server ready").expect("server setup");
            (client, server_task)
        });

        Self {
            rt,
            client,
            server_task,
        }
    }

    fn echo_u64(&self, value: u64) -> u64 {
        self.rt
            .block_on(self.client.echo_u64(value))
            .expect("echo_u64 call")
    }

    fn echo_gnarly(&self, payload: GnarlyPayload) -> GnarlyPayload {
        self.rt
            .block_on(self.client.echo_gnarly(payload))
            .expect("echo_gnarly call")
    }
}

impl Drop for RpcHarness {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

#[derive(Debug, Clone, Copy)]
enum RpcCodecMode {
    Jit,
    NonJit,
}

struct CodecModeGuard {
    _lock: MutexGuard<'static, ()>,
    previous: Option<OsString>,
}

static CODEC_MODE_LOCK: Mutex<()> = Mutex::new(());

impl CodecModeGuard {
    fn install(mode: RpcCodecMode) -> Self {
        let lock = CODEC_MODE_LOCK.lock().expect("codec mode lock poisoned");
        let previous = std::env::var_os("VOX_CODEC");
        unsafe {
            match mode {
                RpcCodecMode::Jit => std::env::set_var("VOX_CODEC", "jit"),
                RpcCodecMode::NonJit => std::env::set_var("VOX_CODEC", "reflect"),
            }
        }
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for CodecModeGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var("VOX_CODEC", value),
                None => std::env::remove_var("VOX_CODEC"),
            }
        }
    }
}

type GnarlyArgs = (GnarlyPayload,);
type GnarlyResponse = Result<GnarlyPayload, VoxError<std::convert::Infallible>>;

struct CodecFixture<T> {
    value: T,
    bytes: Vec<u8>,
    plan: vox_postcard::plan::TranslationPlan,
    registry: vox_types::SchemaRegistry,
}

impl<T> CodecFixture<T>
where
    T: Facet<'static>,
{
    fn new(value: T) -> Self {
        let bytes = vox_postcard::to_vec(&value).expect("reflective encode fixture");
        let plan = vox_postcard::build_identity_plan(T::SHAPE);
        let registry = vox_types::SchemaRegistry::new();
        let jit_bytes = jit_encode(&value);
        assert_eq!(jit_bytes, bytes);
        let _: T = jit_decode(&bytes, &plan, &registry);
        Self {
            value,
            bytes,
            plan,
            registry,
        }
    }
}


mod serde_mirror {
    //! Mirror of `spec_proto::GnarlyPayload` with `serde` derives, so the
    //! codec benches can compare `vox-postcard` (reflective via Facet) and
    //! `vox-jit` (Cranelift-compiled) against the upstream `postcard` crate
    //! on equivalent data. Wire format is the same — verified by the
    //! fixture below.
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Serialize, Deserialize)]
    pub struct GnarlyAttr {
        pub key: String,
        pub value: String,
    }

    #[derive(Clone, Serialize, Deserialize)]
    pub enum GnarlyKind {
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

    #[derive(Clone, Serialize, Deserialize)]
    pub struct GnarlyEntry {
        pub id: u64,
        pub parent: Option<u64>,
        pub name: String,
        pub path: String,
        pub attrs: Vec<GnarlyAttr>,
        pub chunks: Vec<Vec<u8>>,
        pub kind: GnarlyKind,
    }

    #[derive(Clone, Serialize, Deserialize)]
    pub struct GnarlyPayload {
        pub revision: u64,
        pub mount: String,
        pub entries: Vec<GnarlyEntry>,
        pub footer: Option<String>,
        pub digest: Vec<u8>,
    }
}

fn make_gnarly_payload_serde(entry_count: usize, seq: usize) -> serde_mirror::GnarlyPayload {
    use serde_mirror::*;
    let entries = (0..entry_count)
        .map(|i| {
            let attrs = vec![
                GnarlyAttr {
                    key: "owner".to_string(),
                    value: format!("user-{seq}-{i}"),
                },
                GnarlyAttr {
                    key: "class".to_string(),
                    value: format!("hot-path-{}", (seq + i) % 17),
                },
                GnarlyAttr {
                    key: "etag".to_string(),
                    value: format!("etag-{seq:08x}-{i:08x}"),
                },
            ];
            let chunks = (0..3)
                .map(|j| {
                    let len = 32 * (j + 1);
                    vec![((seq + i + j) & 0xff) as u8; len]
                })
                .collect();
            let kind = match i % 3 {
                0 => GnarlyKind::File {
                    mime: "application/octet-stream".to_string(),
                    tags: vec![
                        "warm".to_string(),
                        "cacheable".to_string(),
                        format!("tag-{seq}-{i}"),
                    ],
                },
                1 => GnarlyKind::Directory {
                    child_count: i as u32 + 3,
                    children: vec![
                        format!("child-{seq}-{i}-0"),
                        format!("child-{seq}-{i}-1"),
                        format!("child-{seq}-{i}-2"),
                    ],
                },
                _ => GnarlyKind::Symlink {
                    target: format!("/target/{seq}/{i}/nested/item"),
                    hops: vec![1, 2, 3, i as u32],
                },
            };
            GnarlyEntry {
                id: seq as u64 * 1_000_000 + i as u64,
                parent: if i == 0 {
                    None
                } else {
                    Some(seq as u64 * 1_000_000 + i as u64 - 1)
                },
                name: format!("entry-{seq}-{i}"),
                path: format!("/mount/very/deep/path/with/component/{seq}/{i}/file.bin"),
                attrs,
                chunks,
                kind,
            }
        })
        .collect();

    GnarlyPayload {
        revision: seq as u64,
        mount: format!("/mnt/bench-fast-path-{seq:08x}"),
        entries,
        footer: Some(format!("benchmark footer {seq}")),
        digest: vec![(seq & 0xff) as u8; 64],
    }
}

mod codec {
    use super::*;

    mod args {
        use super::*;

        #[divan::bench(args = [1, 4, 16])]
        fn reflective_encode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::new((make_gnarly_payload(n, 0),));
            bencher.bench_local(|| {
                black_box(vox_postcard::to_vec(black_box(&fixture.value)).unwrap())
            });
        }

        #[divan::bench(args = [1, 4, 16])]
        fn jit_encode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::new((make_gnarly_payload(n, 0),));
            bencher.bench_local(|| black_box(super::super::jit_encode(black_box(&fixture.value))));
        }

        #[divan::bench(args = [1, 4, 16])]
        fn serde_encode(bencher: Bencher, n: usize) {
            let value = (make_gnarly_payload_serde(n, 0),);
            // Sanity: serde-postcard bytes match vox-postcard bytes for the
            // same logical payload. If this ever drifts, the codec
            // comparison stops being apples-to-apples.
            let serde_bytes = postcard::to_allocvec(&value).unwrap();
            let vox_bytes = vox_postcard::to_vec(&(make_gnarly_payload(n, 0),)).unwrap();
            assert_eq!(
                serde_bytes, vox_bytes,
                "serde-postcard and vox-postcard wire bytes diverged"
            );
            bencher
                .bench_local(|| black_box(postcard::to_allocvec(black_box(&value)).unwrap()));
        }

        #[divan::bench(args = [1, 4, 16])]
        fn reflective_decode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::<GnarlyArgs>::new((make_gnarly_payload(n, 0),));
            bencher.bench_local(|| {
                black_box(
                    vox_postcard::from_slice_with_plan::<GnarlyArgs>(
                        black_box(&fixture.bytes),
                        &fixture.plan,
                        &fixture.registry,
                    )
                    .unwrap(),
                )
            });
        }

        #[divan::bench(args = [1, 4, 16])]
        fn jit_decode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::<GnarlyArgs>::new((make_gnarly_payload(n, 0),));
            bencher.bench_local(|| {
                black_box(super::super::jit_decode::<GnarlyArgs>(
                    black_box(&fixture.bytes),
                    &fixture.plan,
                    &fixture.registry,
                ))
            });
        }

        #[divan::bench(args = [1, 4, 16])]
        fn serde_decode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::<GnarlyArgs>::new((make_gnarly_payload(n, 0),));
            bencher.bench_local(|| {
                black_box(
                    postcard::from_bytes::<(serde_mirror::GnarlyPayload,)>(black_box(
                        &fixture.bytes,
                    ))
                    .unwrap(),
                )
            });
        }

        // ---- Borrowed-decode benches: zero-copy strings + byte slices ------
        // Wire format matches the owned variant byte-for-byte; we encode the
        // owned payload once for the fixture, then decode into the borrowed
        // mirror type (`&'a str` / `&'a [u8]` slicing the input). At n=16 this
        // skips ~222 of 272 per-decode allocations (all leaf strings + chunk
        // bytes); only the structural Vec backings remain.

        #[divan::bench(args = [1, 4, 16])]
        fn jit_decode_borrowed(bencher: Bencher, n: usize) {
            type BorrowedArgs<'a> = (vox_bench::borrowed::GnarlyPayload<'a>,);
            let bytes = vox_postcard::to_vec(&(make_gnarly_payload(n, 0),)).unwrap();
            let plan =
                vox_postcard::build_identity_plan(<BorrowedArgs<'_> as Facet<'_>>::SHAPE);
            let registry = vox_types::SchemaRegistry::new();
            // Warm the JIT cache for borrowed-mode decoder.
            let _: BorrowedArgs<'_> = vox_jit::global_runtime()
                .try_decode_borrowed::<BorrowedArgs<'_>>(&bytes, 0, &plan, &registry)
                .expect("borrowed JIT decode unsupported")
                .expect("borrowed JIT decode failed");
            bencher.bench_local(|| {
                black_box(
                    vox_jit::global_runtime()
                        .try_decode_borrowed::<BorrowedArgs<'_>>(
                            black_box(&bytes),
                            0,
                            &plan,
                            &registry,
                        )
                        .unwrap()
                        .unwrap(),
                )
            });
        }

        #[divan::bench(args = [1, 4, 16])]
        fn serde_decode_borrowed(bencher: Bencher, n: usize) {
            type BorrowedArgs<'a> = (vox_bench::borrowed::GnarlyPayload<'a>,);
            let bytes = vox_postcard::to_vec(&(make_gnarly_payload(n, 0),)).unwrap();
            // Sanity: confirm the borrowed serde decode actually parses the
            // same bytes (i.e. wire format compat between owned and borrowed).
            let _: BorrowedArgs<'_> = postcard::from_bytes(&bytes).unwrap();
            bencher.bench_local(|| {
                black_box(
                    postcard::from_bytes::<BorrowedArgs<'_>>(black_box(&bytes)).unwrap(),
                )
            });
        }
    }

    mod response {
        use super::*;

        #[divan::bench(args = [1, 4, 16])]
        fn reflective_encode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::new(Ok::<_, VoxError<std::convert::Infallible>>(
                make_gnarly_payload(n, 0),
            ));
            bencher.bench_local(|| {
                black_box(vox_postcard::to_vec(black_box(&fixture.value)).unwrap())
            });
        }

        #[divan::bench(args = [1, 4, 16])]
        fn jit_encode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::new(Ok::<_, VoxError<std::convert::Infallible>>(
                make_gnarly_payload(n, 0),
            ));
            bencher.bench_local(|| black_box(super::super::jit_encode(black_box(&fixture.value))));
        }

        #[divan::bench(args = [1, 4, 16])]
        fn serde_encode(bencher: Bencher, n: usize) {
            // Mirror of `Result<GnarlyPayload, VoxError<Infallible>>` — Ok
            // variant only, no error payload, so we just wrap in a 2-arm
            // enum that reproduces postcard's variant-index wire layout.
            let value: Result<serde_mirror::GnarlyPayload, ()> =
                Ok(make_gnarly_payload_serde(n, 0));
            bencher
                .bench_local(|| black_box(postcard::to_allocvec(black_box(&value)).unwrap()));
        }

        #[divan::bench(args = [1, 4, 16])]
        fn reflective_decode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::<GnarlyResponse>::new(Ok(make_gnarly_payload(n, 0)));
            bencher.bench_local(|| {
                black_box(
                    vox_postcard::from_slice_with_plan::<GnarlyResponse>(
                        black_box(&fixture.bytes),
                        &fixture.plan,
                        &fixture.registry,
                    )
                    .unwrap(),
                )
            });
        }

        #[divan::bench(args = [1, 4, 16])]
        fn jit_decode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::<GnarlyResponse>::new(Ok(make_gnarly_payload(n, 0)));
            bencher.bench_local(|| {
                black_box(super::super::jit_decode::<GnarlyResponse>(
                    black_box(&fixture.bytes),
                    &fixture.plan,
                    &fixture.registry,
                ))
            });
        }

        #[divan::bench(args = [1, 4, 16])]
        fn serde_decode(bencher: Bencher, n: usize) {
            let fixture = CodecFixture::<GnarlyResponse>::new(Ok(make_gnarly_payload(n, 0)));
            bencher.bench_local(|| {
                black_box(
                    postcard::from_bytes::<Result<serde_mirror::GnarlyPayload, ()>>(black_box(
                        &fixture.bytes,
                    ))
                    .unwrap(),
                )
            });
        }
    }
}

fn bench_echo_u64(bencher: Bencher, mode: RpcCodecMode, harness: fn() -> RpcHarness) {
    let _mode = CodecModeGuard::install(mode);
    let harness = harness();
    assert_eq!(harness.echo_u64(7), 7);

    bencher.bench_local(|| black_box(harness.echo_u64(42)));
}

fn bench_echo_gnarly(bencher: Bencher, mode: RpcCodecMode, harness: fn() -> RpcHarness, n: usize) {
    let _mode = CodecModeGuard::install(mode);
    let harness = harness();
    let probe = make_gnarly_payload(n, 0);
    let probe_response = harness.echo_gnarly(probe.clone());
    assert_eq!(probe_response, probe);

    let mut seq = 1usize;
    bencher
        .with_inputs(|| {
            let payload = make_gnarly_payload(n, seq);
            seq += 1;
            payload
        })
        .bench_local_values(|payload| black_box(harness.echo_gnarly(payload)));
}

mod mem {
    use super::*;

    mod jit {
        use super::*;

        #[divan::bench]
        fn echo_u64(bencher: Bencher) {
            bench_echo_u64(bencher, RpcCodecMode::Jit, RpcHarness::mem);
        }

        #[divan::bench(args = [1, 4, 16])]
        fn echo_gnarly(bencher: Bencher, n: usize) {
            bench_echo_gnarly(bencher, RpcCodecMode::Jit, RpcHarness::mem, n);
        }
    }

    mod non_jit {
        use super::*;

        #[divan::bench]
        fn echo_u64(bencher: Bencher) {
            bench_echo_u64(bencher, RpcCodecMode::NonJit, RpcHarness::mem);
        }

        #[divan::bench(args = [1, 4, 16])]
        fn echo_gnarly(bencher: Bencher, n: usize) {
            bench_echo_gnarly(bencher, RpcCodecMode::NonJit, RpcHarness::mem, n);
        }
    }
}

mod tcp {
    use super::*;

    mod jit {
        use super::*;

        #[divan::bench]
        fn echo_u64(bencher: Bencher) {
            bench_echo_u64(bencher, RpcCodecMode::Jit, RpcHarness::tcp);
        }

        #[divan::bench(args = [1, 4, 16])]
        fn echo_gnarly(bencher: Bencher, n: usize) {
            bench_echo_gnarly(bencher, RpcCodecMode::Jit, RpcHarness::tcp, n);
        }
    }

    mod non_jit {
        use super::*;

        #[divan::bench]
        fn echo_u64(bencher: Bencher) {
            bench_echo_u64(bencher, RpcCodecMode::NonJit, RpcHarness::tcp);
        }

        #[divan::bench(args = [1, 4, 16])]
        fn echo_gnarly(bencher: Bencher, n: usize) {
            bench_echo_gnarly(bencher, RpcCodecMode::NonJit, RpcHarness::tcp, n);
        }
    }
}
