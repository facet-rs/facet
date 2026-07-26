//! The archive-extraction output model: a plain path -> contents tree, plus the
//! backend trait that produces one.
//!
//! This used to live in `vix-core` as `vix::fetch`, over a `Tree` borrowed from
//! `vix::exec` — a domain trait (archives, checksums, extraction) sitting in the
//! language crate, implemented from outside it, over a type belonging to a
//! design oracle. `vix-core` had no consumer of any of it: this crate was the
//! only one, and it still is. So it lives here now, and the language keeps none
//! of it.
//!
//! Note this `Tree` is *not* the vix `Host("Tree")` domain value — that one is
//! an extern-backed opaque whose identity is its canonical tree encoding, and it
//! is declared by `vixen-primitives`. This is the fetch backend's own plain
//! output type, which is why it can be an ordinary map.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

/// A content-addressed tree: path -> contents. Text entries keep the readable
/// lane readable; blob entries carry real artifacts without lossy UTF-8
/// conversion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    pub entries: BTreeMap<String, String>,
    pub blobs: BTreeMap<String, Vec<u8>>,
}

impl Tree {
    #[must_use]
    pub fn of(entries: &[(&str, &str)]) -> Tree {
        Tree {
            entries: entries
                .iter()
                .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
                .collect(),
            blobs: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn of_blobs(entries: &[(&str, &[u8])]) -> Tree {
        Tree {
            entries: BTreeMap::new(),
            blobs: entries
                .iter()
                .map(|(p, c)| ((*p).to_string(), (*c).to_vec()))
                .collect(),
        }
    }

    pub fn insert_bytes(&mut self, path: impl Into<String>, contents: Vec<u8>) {
        let path = path.into();
        match String::from_utf8(contents) {
            Ok(text) => {
                self.entries.insert(path, text);
            }
            Err(err) => {
                self.blobs.insert(path, err.into_bytes());
            }
        }
    }

    #[must_use]
    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.blobs.get(path).cloned().or_else(|| {
            self.entries
                .get(path)
                .map(|contents| contents.as_bytes().to_vec())
        })
    }
}

#[derive(Debug, Clone)]
pub struct FetchOutput {
    pub tree: Tree,
    pub actual_sha256: String,
}

pub trait FetchBackend: Send + Sync {
    fn fetch(&self, url: &str, expected_sha256: Option<&str>) -> Result<FetchOutput, String>;
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
