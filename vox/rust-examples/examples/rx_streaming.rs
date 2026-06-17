use eyre::{Result, WrapErr, eyre};
use vox::transport::tcp::StreamLink;
use vox::{Rx, Tx, channel};

/// Demonstrates Rx<T> in argument position for client-to-server streaming.
///
/// - `sum`: the client streams integers to the server, which accumulates and
///   returns the total.
/// - `generate`: the server streams results back through a Tx<T> in argument
///   position while consuming configuration from the client.
#[vox::service]
trait NumberLab {
    /// Client sends a stream of i64 values; server returns the sum.
    async fn sum(&self, numbers: Rx<i64>) -> i64;

    /// Client sends a count; server writes that many squares into `output`.
    async fn squares(&self, count: u32, output: Tx<i64>);
}

#[derive(Clone, Copy)]
struct NumberLabService;

impl NumberLab for NumberLabService {
    async fn sum(&self, mut numbers: Rx<i64>) -> i64 {
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            let n = n.get();
            total += *n;
        }
        total
    }

    async fn squares(&self, count: u32, output: Tx<i64>) {
        for i in 1..=count as i64 {
            if output.send(i * i).await.is_err() {
                break;
            }
        }
        let _ = output.close(Default::default()).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    println!("[demo] binding TCP listener");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .wrap_err("binding TCP listener")?;
    let addr = listener.local_addr().wrap_err("reading listener addr")?;
    println!("[demo] listening on {addr}");
    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();

    let server_task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("accept");
        let server_connection = vox::acceptor_on(StreamLink::tcp(socket))
            .on_lane(NumberLabDispatcher::new(NumberLabService))
            .establish_connection()
            .await
            .expect("server establish");
        let _ = server_ready_tx.send(());
        let _server_connection = server_connection;
        std::future::pending::<()>().await;
    });

    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .wrap_err("connecting")?;
    let connection = vox::initiator_on(StreamLink::tcp(socket))
        .establish_connection()
        .await
        .map_err(|e| eyre!("establish failed: {e:?}"))?;
    println!("[client] connection established");
    server_ready_rx
        .await
        .map_err(|_| eyre!("server task ended before signaling readiness"))?;
    let client: NumberLabClient = connection
        .open_lane()
        .await
        .map_err(|e| eyre!("open NumberLab lane failed: {e:?}"))?;
    println!("[client] NumberLab lane open");

    // --- Rx<T> in arg position: client-to-server streaming ---
    println!("\n[client] calling sum (client→server streaming via Rx<i64>)");
    let (tx, rx) = channel::<i64>();
    let send_task = tokio::spawn(async move {
        for n in 1..=10 {
            println!("[client/send] -> {n}");
            tx.send(n).await.expect("send");
        }
        tx.close(Default::default()).await.expect("close");
    });
    let total = client
        .sum(rx)
        .await
        .map_err(|e| eyre!("sum failed: {e:?}"))?;
    send_task.await.wrap_err("joining send_task")?;
    assert_eq!(total, 55);
    println!("[client] sum returned {total}");

    // --- Tx<T> in arg position: server-to-client streaming ---
    println!("\n[client] calling squares (server→client streaming via Tx<i64>)");
    let (output_tx, mut output_rx) = channel::<i64>();
    let recv_task = tokio::spawn(async move {
        let mut squares = Vec::new();
        while let Some(val) = output_rx.recv().await.wrap_err("recv")? {
            let val = val.get();
            println!("[client/recv] <- {}", *val);
            squares.push(*val);
        }
        Ok::<_, eyre::Report>(squares)
    });
    client
        .squares(5, output_tx)
        .await
        .map_err(|e| eyre!("squares failed: {e:?}"))?;
    let squares = recv_task.await.wrap_err("joining squares receiver")??;
    assert_eq!(squares, vec![1, 4, 9, 16, 25]);
    println!("[client] squares returned {squares:?}");

    server_task.abort();
    let _ = server_task.await;
    println!("\n[demo] rx_streaming: complete");
    Ok(())
}
