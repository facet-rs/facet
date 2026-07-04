use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::exec::Tree;
use crate::value::Value;

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

pub(crate) struct FetchObservation {
    pub(crate) key: String,
    pub(crate) replayed: bool,
}

pub(crate) fn fetch_value(
    journal: &mut BTreeMap<String, Value>,
    backend: &dyn FetchBackend,
    url: String,
    declared_sha256: Option<String>,
) -> Result<(Value, FetchObservation), String> {
    let key = match &declared_sha256 {
        Some(sha) => format!("fetch:{url}:sha256:{sha}"),
        None => format!("fetch:{url}:observed"),
    };
    if let Some(pin) = journal.get(&key) {
        let Value::Str(pinned_sha256) = pin else {
            return Err(format!("fetch journal pin `{key}` is not a sha256 string"));
        };
        let fetched = backend.fetch(&url, Some(pinned_sha256))?;
        verify_checksum(&url, Some(pinned_sha256), &fetched.actual_sha256)?;
        return Ok((
            Value::Tree(fetched.tree),
            FetchObservation {
                key,
                replayed: true,
            },
        ));
    }

    let fetched = backend.fetch(&url, declared_sha256.as_deref())?;
    verify_checksum(&url, declared_sha256.as_deref(), &fetched.actual_sha256)?;
    journal.insert(key.clone(), Value::Str(fetched.actual_sha256));
    Ok((
        Value::Tree(fetched.tree),
        FetchObservation {
            key,
            replayed: false,
        },
    ))
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
