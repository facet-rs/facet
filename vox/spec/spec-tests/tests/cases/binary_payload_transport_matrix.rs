use spec_proto::Message;
use spec_tests::harness::{
    RustTransport, SubjectSpec, accept_rust_inproc, accept_subject_spec, run_async,
};

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

fn make_payload(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i as u8).wrapping_mul(17)).collect()
}

async fn run_for_transport(transport: RustTransport) -> Result<(), String> {
    let client = accept_rust_inproc(transport).await?;
    for &size in payload_sizes() {
        let payload = make_payload(size);
        let resp = client
            .process_message(Message::Data(payload.clone()))
            .await
            .map_err(|e| format!("transport={transport:?} size={size}: {e:?}"))?;
        let actual = match &resp {
            Message::Data(actual) => actual,
            _ => {
                return Err(format!(
                    "transport={transport:?} size={size}: expected Data response"
                ));
            }
        };
        let mut expected = payload;
        expected.reverse();
        if actual != &expected {
            return Err(format!(
                "transport={transport:?} size={size}: payload mismatch (len={})",
                actual.len()
            ));
        }
    }
    Ok(())
}

async fn run_for_subject_transport(spec: SubjectSpec) -> Result<(), String> {
    let (client, mut child, _sh) = accept_subject_spec(spec).await?;
    for &size in payload_sizes() {
        eprintln!("[test] sending size={size}");
        let payload = make_payload(size);
        let resp = client
            .process_message(Message::Data(payload.clone()))
            .await
            .map_err(|e| format!("subject spec={spec:?} size={size}: {e:?}"))?;
        eprintln!("[test] got response size={size}");
        let actual = match &resp {
            Message::Data(actual) => actual,
            _ => {
                child.kill().await.ok();
                return Err(format!(
                    "subject spec={spec:?} size={size}: expected Data response"
                ));
            }
        };
        let mut expected = payload;
        expected.reverse();
        if actual != &expected {
            child.kill().await.ok();
            return Err(format!(
                "subject spec={spec:?} size={size}: payload mismatch (len={})",
                actual.len()
            ));
        }
    }
    eprintln!(
        "[test] done. our pid={} child pid={:?}",
        std::process::id(),
        child.id()
    );
    child.start_kill().ok();
    child.wait().await.ok();
    Ok(())
}

// r[verify transport.message.binary]
pub fn run_rust_binary_payload_transport_matrix_mem() {
    run_async(async {
        run_for_transport(RustTransport::Mem).await?;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify transport.message.binary]
pub fn run_rust_binary_payload_transport_matrix_subject_tcp(spec: SubjectSpec) {
    run_async(async {
        run_for_subject_transport(spec).await?;
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify transport.message.binary]
pub fn run_rust_binary_payload_transport_matrix_subject_shm(spec: SubjectSpec) {
    run_async(async {
        run_for_subject_transport(spec).await?;
        Ok::<_, String>(())
    })
    .unwrap();
}
