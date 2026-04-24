use std::ffi::OsString;
use std::future::pending;
use std::sync::{Mutex, MutexGuard};

use divan::{Bencher, black_box};
use facet::Facet;
use spec_proto::{
    GnarlyAttr, GnarlyEntry, GnarlyKind, GnarlyPayload, TestbedClient, TestbedDispatcher,
};
use subject_rust::TestbedService;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use vox::transport::tcp::StreamLink;
use vox::{TransportMode, initiator_on, memory_link_pair};
use vox_types::VoxError;

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
    previous_disable: Option<OsString>,
}

static CODEC_MODE_LOCK: Mutex<()> = Mutex::new(());

impl CodecModeGuard {
    fn install(mode: RpcCodecMode) -> Self {
        let lock = CODEC_MODE_LOCK.lock().expect("codec mode lock poisoned");
        let previous_disable = std::env::var_os("VOX_JIT_DISABLE");
        unsafe {
            match mode {
                RpcCodecMode::Jit => std::env::remove_var("VOX_JIT_DISABLE"),
                RpcCodecMode::NonJit => std::env::set_var("VOX_JIT_DISABLE", "1"),
            }
        }
        Self {
            _lock: lock,
            previous_disable,
        }
    }
}

impl Drop for CodecModeGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.previous_disable {
                Some(value) => std::env::set_var("VOX_JIT_DISABLE", value),
                None => std::env::remove_var("VOX_JIT_DISABLE"),
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

fn jit_encode<T>(value: &T) -> Vec<u8>
where
    T: Facet<'static>,
{
    let ptr = facet::PtrConst::new((value as *const T).cast::<u8>());
    vox_jit::global_runtime()
        .try_encode_ptr(ptr, T::SHAPE)
        .expect("JIT encode unsupported")
        .expect("JIT encode failed")
}

fn jit_decode<T>(
    bytes: &[u8],
    plan: &vox_postcard::plan::TranslationPlan,
    registry: &vox_types::SchemaRegistry,
) -> T
where
    T: Facet<'static>,
{
    vox_jit::global_runtime()
        .try_decode_owned::<T>(bytes, 0, plan, registry)
        .expect("JIT decode unsupported")
        .expect("JIT decode failed")
}

fn make_gnarly_payload(entry_count: usize, seq: usize) -> GnarlyPayload {
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
