//! Channeling RPC compliance tests.
//!
//! Tests the channeling methods from the `Testbed` service:
//! - `sum(numbers: Rx<i32>) -> i64` - client-to-server channel
//! - `generate(count: u32, output: Tx<i32>)` - server-to-client channel
//! - `transform(input: Rx<String>, output: Tx<String>)` - bidirectional channels

use spec_tests::harness::{SubjectSpec, accept_subject_spec, run_async, spawn_loud};

// r[verify channeling.type]
// r[verify channeling.data]
// r[verify channeling.close]
// r[verify channeling.caller-pov]
// r[verify channeling.allocation.caller]
pub fn run_channeling_sum_client_to_server(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;

        let (tx, rx) = roam::channel::<i32>();
        spawn_loud(async move {
            for n in [1i32, 2, 3, 4, 5] {
                tx.send(n).await.unwrap();
            }
            tx.close(Default::default()).await.unwrap();
        });

        let resp = client.sum(rx).await.map_err(|e| format!("sum: {e:?}"))?;
        if resp != 15 {
            return Err(format!("expected sum 15, got {}", resp));
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.type]
// r[verify channeling.data]
// r[verify channeling.close]
pub fn run_channeling_generate_server_to_client(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;

        let (tx, mut rx) = roam::channel::<i32>();
        let recv = spawn_loud(async move {
            let mut received = Vec::new();
            while let Ok(Some(n)) = rx.recv().await {
                received.push(*n);
            }
            received
        });

        client
            .generate(5, tx)
            .await
            .map_err(|e| format!("generate: {e:?}"))?;

        let received = recv.await.map_err(|e| format!("recv task: {e}"))?;
        let expected: Vec<i32> = (0..5).collect();
        if received != expected {
            return Err(format!("expected {expected:?}, got {received:?}"));
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
pub fn run_channeling_transform_bidirectional(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;

        let (input_tx, input_rx) = roam::channel::<String>();
        let (output_tx, mut output_rx) = roam::channel::<String>();

        let messages = ["hello", "world", "test"];
        spawn_loud(async move {
            for msg in messages {
                input_tx.send(msg.to_string()).await.unwrap();
            }
            input_tx.close(Default::default()).await.unwrap();
        });

        let recv = spawn_loud(async move {
            let mut received = Vec::new();
            while let Ok(Some(s)) = output_rx.recv().await {
                received.push(s.clone());
            }
            received
        });

        client
            .transform(input_rx, output_tx)
            .await
            .map_err(|e| format!("transform: {e:?}"))?;

        let received = recv.await.map_err(|e| format!("recv task: {e}"))?;
        let expected: Vec<String> = messages.iter().map(|s| s.to_string()).collect();
        if received != expected {
            return Err(format!("expected {expected:?}, got {received:?}"));
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}
