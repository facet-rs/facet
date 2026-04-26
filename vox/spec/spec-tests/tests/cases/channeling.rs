//! Channeling RPC compliance tests.
//!
//! Tests the channeling methods from the `Testbed` service:
//! - `sum(numbers: Rx<i32>) -> i64` - client-to-server channel
//! - `generate(count: u32, output: Tx<i32>)` - server-to-client channel
//! - `transform(input: Rx<String>, output: Tx<String>)` - bidirectional channels

use spec_tests::harness::{
    SubjectSpec, accept_subject_spec, run_async, run_subject_client_scenario, spawn_loud,
};

// r[verify channeling.type]
// r[verify channeling.data]
// r[verify channeling.close]
// r[verify channeling.caller-pov]
// r[verify channeling.allocation.caller]
pub fn run_channeling_sum_client_to_server(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;

        let (tx, rx) = vox::channel::<i32>();
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
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;

        let (tx, mut rx) = vox::channel::<i32>();
        let recv = spawn_loud(async move {
            let mut received = Vec::new();
            while let Ok(Some(n)) = rx.recv().await {
                let n = n.get();
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
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;

        let (input_tx, input_rx) = vox::channel::<String>();
        let (output_tx, mut output_rx) = vox::channel::<String>();

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
                let s = s.get();
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

// r[verify channeling.lifecycle.outlives-response]
// r[verify channeling.type]
// r[verify channeling.data]
// r[verify channeling.close]
pub fn run_channeling_post_reply_generate_server_to_client(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;

        let (tx, mut rx) = vox::channel::<i32>();

        client
            .post_reply_generate(tx)
            .await
            .map_err(|e| format!("post_reply_generate: {e:?}"))?;

        let mut received = Vec::new();
        while let Ok(Some(n)) = rx.recv().await {
            let n = n.get();
            received.push(*n);
        }

        let expected: Vec<i32> = (0..5).collect();
        if received != expected {
            return Err(format!("expected {expected:?}, got {received:?}"));
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.lifecycle.outlives-response]
// r[verify channeling.type]
// r[verify channeling.data]
// r[verify channeling.close]
pub fn run_channeling_post_reply_sum_client_to_server(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child, _sh) = accept_subject_spec(spec).await?;

        let (input_tx, input_rx) = vox::channel::<i32>();
        let (result_tx, mut result_rx) = vox::channel::<i64>();

        client
            .post_reply_sum(input_rx, result_tx)
            .await
            .map_err(|e| format!("post_reply_sum: {e:?}"))?;

        for n in [1i32, 2, 3, 4, 5] {
            input_tx.send(n).await.unwrap();
        }
        input_tx.close(Default::default()).await.unwrap();

        let total = match result_rx.recv().await {
            Ok(Some(total)) => *total.get(),
            Ok(None) => {
                return Err("post_reply_sum result channel closed without a value".to_string());
            }
            Err(e) => return Err(format!("post_reply_sum result recv: {e}")),
        };

        if total != 15 {
            return Err(format!("expected post-reply sum 15, got {total}"));
        }

        match result_rx.recv().await {
            Ok(None) => {}
            Ok(Some(extra)) => {
                return Err(format!(
                    "post_reply_sum result channel yielded extra value {}",
                    *extra.get()
                ));
            }
            Err(e) => return Err(format!("post_reply_sum result close recv: {e}")),
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

pub fn run_subject_calls_post_reply_generate(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "post_reply_generate");
}

pub fn run_subject_calls_post_reply_sum(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "post_reply_sum");
}
