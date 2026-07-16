//! Integration tests for the dibs LSP extension.
//!
//! These tests spawn the actual `dibs lsp-extension` binary and communicate
//! with it over vox, just like the real Styx LSP does.
//!
//! The test owns a real application tooling endpoint backed by `my-app-db`, then
//! launches the actual CLI extension against it.

use std::path::PathBuf;
use tokio::net::TcpListener;

struct AbortOnDrop(tokio::task::JoinHandle<std::io::Result<()>>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn test_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/lsp")
        .join(name)
}

#[tokio::test]
async fn test_completions() {
    my_app_db::ensure_linked();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = listener.local_addr().unwrap();
    let _service = AbortOnDrop(tokio::spawn(dibs::serve_listener(listener)));

    let workspace = tempfile::tempdir().unwrap();
    let config_dir = workspace.path().join(".config");
    let project_dir = workspace.path().join("my-app-queries");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(project_dir.join(".dibs-queries")).unwrap();
    std::fs::write(
        config_dir.join("dibs.styx"),
        format!(
            "@schema {{id crate:dibs@1, cli dibs}}\n\ndb {{\n    crate my-app-db\n    endpoint \"{endpoint}\"\n}}\n"
        ),
    )
    .unwrap();

    let document_uri = format!(
        "file://{}/.dibs-queries/queries.styx",
        project_dir.display()
    );

    styx_lsp::testing::assert_test_file_with_uri(
        env!("CARGO_BIN_EXE_dibs"),
        &["lsp-extension"],
        test_file("completions.styx"),
        "crate:dibs-queries@1",
        &document_uri,
    )
    .await;
}
