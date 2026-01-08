//! Query key encoding and erased identifiers used for dependency graphs.

use crate::error::{PicanteError, PicanteResult};
use facet::Facet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

// r[kind.type]
/// Stable identifier for a query/input kind.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct QueryKindId(pub u32);

impl QueryKindId {
    /// Convert to the underlying `u32`.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    // r[kind.hash]
    // r[kind.stability]
    /// Create a stable id from a string.
    ///
    /// This is intended for macro-generated kind ids, which must remain stable
    /// across runs for cache persistence.
    ///
    /// The hash algorithm is a 32-bit FNV-1a over UTF-8 bytes.
    pub const fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        let mut hash: u32 = 0x811c9dc5; // FNV offset basis
        let mut i = 0usize;
        while i < bytes.len() {
            hash ^= bytes[i] as u32;
            hash = hash.wrapping_mul(0x0100_0193); // FNV prime
            i += 1;
        }
        QueryKindId(hash)
    }
}

// r[key.encoding]
/// Postcard-encoded bytes for a key, plus a deterministic hash for tracing/debugging.
#[derive(Clone)]
pub struct Key {
    bytes: Arc<[u8]>,
    // r[key.hash]
    hash: u64,
}

impl Key {
    /// Encode a key using `facet-postcard`.
    pub fn encode_facet<T: Facet<'static>>(value: &T) -> PicanteResult<Self> {
        let bytes = facet_postcard::to_vec(value).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "key",
                message: format!("{e:?}"),
            })
        })?;
        Ok(Self::from_bytes(bytes))
    }

    /// Decode a key using `facet-postcard`.
    pub fn decode_facet<T: Facet<'static>>(&self) -> PicanteResult<T> {
        facet_postcard::from_slice(self.bytes()).map_err(|e| {
            Arc::new(PicanteError::Decode {
                what: "key",
                message: format!("{e:?}"),
            })
        })
    }

    /// Construct from already-encoded bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let hash = stable_hash(&bytes);
        Self {
            bytes: bytes.into(),
            hash,
        }
    }

    /// Access the encoded bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Deterministic hash of the encoded bytes.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Length in bytes of the encoded key.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns `true` if the encoded key is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

// r[key.equality]
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes // exact byte equality, not hash
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Key")
            .field("hash", &format_args!("{:016x}", self.hash))
            .field("len", &self.bytes.len())
            .finish()
    }
}

// r[key.dyn-key]
/// Erased key for diagnostics/cycle detection.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DynKey {
    /// Kind identifier.
    pub kind: QueryKindId,
    /// Encoded key.
    pub key: Key,
}

// r[key.dep]
/// A recorded dependency edge.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Dep {
    /// The depended-on query kind.
    pub kind: QueryKindId,
    /// Encoded key for that kind.
    pub key: Key,
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}
