use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::exec::Tree;

#[derive(Debug, Clone)]
pub struct FetchOutput {
    pub tree: Tree,
    pub actual_sha256: String,
}

pub trait FetchBackend: Send + Sync {
    fn fetch(&self, url: &str, expected_sha256: Option<&str>) -> Result<FetchOutput, String>;
}

#[derive(Default)]
pub struct NoFetchBackend;

impl FetchBackend for NoFetchBackend {
    fn fetch(&self, url: &str, _expected_sha256: Option<&str>) -> Result<FetchOutput, String> {
        Err(format!("no fetch backend configured for `{url}`"))
    }
}

#[derive(Clone)]
struct FakeArchive {
    bytes: Vec<u8>,
    tree: Tree,
}

#[derive(Clone, Default)]
pub struct FakeFetchBackend {
    archives: BTreeMap<String, FakeArchive>,
}

impl FakeFetchBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_archive(mut self, url: impl Into<String>, bytes: &[u8], tree: Tree) -> Self {
        self.insert_archive(url, bytes, tree);
        self
    }

    pub fn insert_archive(&mut self, url: impl Into<String>, bytes: &[u8], tree: Tree) {
        self.archives.insert(
            url.into(),
            FakeArchive {
                bytes: bytes.to_vec(),
                tree,
            },
        );
    }
}

impl FetchBackend for FakeFetchBackend {
    fn fetch(&self, url: &str, expected_sha256: Option<&str>) -> Result<FetchOutput, String> {
        let archive = self
            .archives
            .get(url)
            .ok_or_else(|| format!("fake fetch has no archive for `{url}`"))?;
        let actual_sha256 = sha256_hex(&archive.bytes);
        verify_checksum(url, expected_sha256, &actual_sha256)?;
        Ok(FetchOutput {
            tree: archive.tree.clone(),
            actual_sha256,
        })
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn verify_checksum(
    url: &str,
    expected_sha256: Option<&str>,
    actual_sha256: &str,
) -> Result<(), String> {
    if let Some(expected) = expected_sha256
        && expected != actual_sha256
    {
        return Err(format!(
            "fetch checksum mismatch for `{url}`: expected {expected}, got {actual_sha256}"
        ));
    }
    Ok(())
}
