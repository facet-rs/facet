use std::future::pending;

use divan::{Bencher, black_box};
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

mod mem {
    use super::*;

    #[divan::bench]
    fn echo_u64(bencher: Bencher) {
        let harness = RpcHarness::mem();
        assert_eq!(harness.echo_u64(7), 7);

        bencher.bench_local(|| black_box(harness.echo_u64(42)));
    }

    #[divan::bench(args = [1, 4, 16])]
    fn echo_gnarly(bencher: Bencher, n: usize) {
        let harness = RpcHarness::mem();
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
}

mod tcp {
    use super::*;

    #[divan::bench]
    fn echo_u64(bencher: Bencher) {
        let harness = RpcHarness::tcp();
        assert_eq!(harness.echo_u64(7), 7);

        bencher.bench_local(|| black_box(harness.echo_u64(42)));
    }

    #[divan::bench(args = [1, 4, 16])]
    fn echo_gnarly(bencher: Bencher, n: usize) {
        let harness = RpcHarness::tcp();
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
}
