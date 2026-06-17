use std::time::Duration;

use spec_tests::harness::{SubjectLanguage, SubjectSpec, run_async};
use tokio::process::Child;

const SUBJECT_EXIT_TIMEOUT: Duration = Duration::from_secs(3);

async fn reap_or_kill(mut child: Child, reason: &str) -> Result<(), String> {
    let pid = child.id().unwrap_or_default();
    match tokio::time::timeout(SUBJECT_EXIT_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(format!(
            "subject pid={pid} exited during {reason} with {status}"
        )),
        Ok(Err(err)) => Err(format!(
            "subject pid={pid} wait failed during {reason}: {err}"
        )),
        Err(_) => {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            Err(format!(
                "subject pid={pid} did not exit within {SUBJECT_EXIT_TIMEOUT:?} after {reason}"
            ))
        }
    }
}

// r[verify hosted.subject.lifecycle]
fn subject_exits_when_harness_shutdowns(spec: SubjectSpec) {
    run_async(async move {
        let (client, child, connection_handle) =
            spec_tests::harness::accept_subject_spec(spec).await?;

        client
            .echo("lifecycle-disconnect".to_string())
            .await
            .map_err(|err| format!("echo before disconnect failed: {err:?}"))?;

        drop(client);
        connection_handle
            .shutdown()
            .map_err(|err| format!("connection shutdown failed: {err:?}"))?;
        drop(connection_handle);
        reap_or_kill(child, "harness shutdown").await
    })
    .unwrap();
}

#[test]
fn rust_tcp_subject_exits_when_harness_shutdowns() {
    subject_exits_when_harness_shutdowns(SubjectSpec::tcp(SubjectLanguage::Rust));
}

#[test]
fn typescript_tcp_subject_exits_when_harness_shutdowns() {
    subject_exits_when_harness_shutdowns(SubjectSpec::tcp(SubjectLanguage::TypeScript));
}

#[test]
fn typescript_websocket_subject_exits_when_harness_shutdowns() {
    subject_exits_when_harness_shutdowns(SubjectSpec::ws(SubjectLanguage::TypeScript));
}

#[test]
fn swift_tcp_subject_exits_when_harness_shutdowns() {
    subject_exits_when_harness_shutdowns(SubjectSpec::tcp(SubjectLanguage::Swift));
}
