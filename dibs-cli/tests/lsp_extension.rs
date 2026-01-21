//! Integration tests for the dibs LSP extension.
//!
//! These tests spawn the actual `dibs lsp-extension` binary and communicate
//! with it over roam, just like the real Styx LSP does.
//!
//! IMPORTANT: These tests need a real dibs project context because the extension
//! connects to the service to fetch the schema. We use the `my-app-db` example.

use std::path::PathBuf;

fn test_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/lsp")
        .join(name)
}

/// Get the path to the my-app-queries example project.
fn example_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/my-app-workspace/my-app-queries")
}

#[tokio::test]
async fn test_completions() {
    // Use a document URI inside the example project so the extension can find the config
    let project_dir = example_project_dir();
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
