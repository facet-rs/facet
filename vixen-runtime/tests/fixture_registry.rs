//! Keeps the offline fixture registry honest: every manifest row's pinned vix
//! ContentHash must equal the framed Blob identity of the archive bytes on
//! disk, computed through the ONE hash definition (`FramedNode::identity`,
//! `machine.identity.blake3`). Pinning the hash is a lock-time act; this test
//! is the lock. On mismatch it prints the corrected row.

use std::path::Path;

use vix::runtime::FramedNode;
use vix::vir::{ExternKind, Type};

fn blob_identity_hex(bytes: &[u8]) -> String {
    let node = FramedNode::leaf(Type::Extern(ExternKind::Blob).schema_ref(), bytes.to_vec());
    node.identity().content.hex()
}

#[test]
fn fixture_registry_manifest_is_locked_to_the_archives() {
    let registry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry");
    let manifest = std::fs::read_to_string(registry.join("manifest")).expect("manifest exists");
    let mut rows = 0;
    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields.next().expect("manifest row has a name");
        let url = fields.next().expect("manifest row has a url");
        let pinned = fields.next().expect("manifest row has a content hash");
        assert!(fields.next().is_none(), "manifest row has three fields");
        let relative = url
            .strip_prefix("fixture://registry/")
            .expect("fixture urls resolve inside the registry fixture directory");
        let bytes = std::fs::read(registry.join(relative)).expect("archive exists");
        let actual = blob_identity_hex(&bytes);
        assert_eq!(
            pinned, actual,
            "manifest row for `{name}` is stale; expected:\n{name} {url} {actual}"
        );
        rows += 1;
    }
    assert!(rows >= 1, "manifest lists at least one archive");
}

#[test]
fn fixture_archive_is_exactly_4096_bytes_of_plain_tar() {
    let registry = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry");
    let bytes = std::fs::read(registry.join("tokio-1.52.3.crate")).expect("archive exists");
    assert_eq!(bytes.len(), 4096, "rung 075 pins the archive length");
    // ustar magic at offset 257 of the first header block.
    assert_eq!(&bytes[257..262], b"ustar", "plain (uncompressed) tar");
}
