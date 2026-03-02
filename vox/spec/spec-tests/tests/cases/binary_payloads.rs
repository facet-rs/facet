use std::time::Duration;

use spec_proto::Message;
use spec_tests::harness::{SubjectSpec, accept_subject_spec, run_async};

fn payload_sizes() -> &'static [usize] {
    &[
        0,
        1,
        247,
        248,
        249,
        31,
        32,
        63,
        64,
        127,
        128,
        255,
        256,
        257,
        511,
        512,
        1024,
        4091,
        4092,
        4093,
        4095,
        4096,
        4097,
        16 * 1024,
        64 * 1024,
        256 * 1024,
        900_000,
        1_000_000,
    ]
}

fn shm_cutover_payload_sizes() -> &'static [usize] {
    &[247, 248, 249, 4091, 4092, 4093, 16 * 1024, 64 * 1024]
}

fn make_payload(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i as u8).wrapping_mul(31)).collect()
}

// r[verify transport.message.binary]
pub fn run_subject_process_message_binary_payload_sizes(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        for &size in payload_sizes() {
            let payload = make_payload(size);
            let resp = client
                .process_message(Message::Data(payload.clone()))
                .await
                .map_err(|e| format!("process_message size={size}: {e:?}"))?;
            let actual = match &resp {
                Message::Data(actual) => actual,
                _ => {
                    child.kill().await.ok();
                    return Err(format!(
                        "process_message size={size}: expected Data response, got different variant"
                    ));
                }
            };
            let mut expected = payload;
            expected.reverse();
            if actual != &expected {
                child.kill().await.ok();
                return Err(format!(
                    "process_message size={size}: payload mismatch (len={})",
                    actual.len()
                ));
            }
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify transport.message.binary]
// r[verify shm.framing.threshold]
pub fn run_subject_process_message_binary_payload_shm_cutover_boundaries(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        for &size in shm_cutover_payload_sizes() {
            let payload = make_payload(size);
            let resp = tokio::time::timeout(
                Duration::from_secs(3),
                client.process_message(Message::Data(payload.clone())),
            )
            .await
            .map_err(|_| format!("process_message size={size}: timed out after 3s"))?
            .map_err(|e| format!("process_message size={size}: {e:?}"))?;
            let actual = match &resp {
                Message::Data(actual) => actual,
                _ => {
                    child.kill().await.ok();
                    return Err(format!(
                        "process_message size={size}: expected Data response, got different variant"
                    ));
                }
            };
            let mut expected = payload;
            expected.reverse();
            if actual != &expected {
                child.kill().await.ok();
                return Err(format!(
                    "process_message size={size}: payload mismatch (len={})",
                    actual.len()
                ));
            }
        }

        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}
