//! Query key encoding and erased identifiers used for dependency graphs.

use crate::error::{PicanteError, PicanteResult};
use facet::Facet;
use std::any::Any;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::OnceLock;

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
    // r[kind.collision]
    // r[kind.uniqueness]
    /// Create a stable id from a string.
    ///
    /// This is intended for macro-generated kind ids, which must remain stable
    /// across runs for cache persistence.
    ///
    /// The hash algorithm is a 32-bit FNV-1a over UTF-8 bytes.
    pub const fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        let mut hash: u32 = 0x811c9dc5; // FNV_OFFSET
        let mut i = 0usize;
        while i < bytes.len() {
            hash ^= bytes[i] as u32;
            hash = hash.wrapping_mul(0x0100_0193); // FNV_PRIME
            i += 1;
        }
        QueryKindId(hash)
    }
}

// r[key.encoding]
/// Erased runtime key plus a deterministic cached hash for indexing/tracing.
#[derive(Clone)]
pub struct Key {
    repr: Arc<KeyRepr>,
    // r[key.hash]
    hash: u64,
}

enum KeyRepr {
    Typed(Arc<dyn ErasedFacetKey>),
    Bytes(Arc<[u8]>),
}

trait ErasedFacetKey: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn eq_typed(&self, other: &dyn ErasedFacetKey) -> bool;
    fn bytes(&self) -> PicanteResult<&[u8]>;
    fn to_bytes(&self) -> PicanteResult<Arc<[u8]>>;
}

struct FacetKey<T> {
    value: Arc<T>,
    bytes: OnceLock<PicanteResult<Arc<[u8]>>>,
}

impl<T> ErasedFacetKey for FacetKey<T>
where
    T: Facet<'static> + Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq_typed(&self, other: &dyn ErasedFacetKey) -> bool {
        other
            .as_any()
            .downcast_ref::<FacetKey<T>>()
            .is_some_and(|other| crate::facet_eq::facet_eq_direct(&*self.value, &*other.value))
    }

    fn bytes(&self) -> PicanteResult<&[u8]> {
        self.bytes
            .get_or_init(|| encode_key_bytes(&*self.value))
            .as_ref()
            .map(|bytes| bytes.as_ref())
            .map_err(Clone::clone)
    }

    fn to_bytes(&self) -> PicanteResult<Arc<[u8]>> {
        self.bytes
            .get_or_init(|| encode_key_bytes(&*self.value))
            .clone()
    }
}

impl Key {
    /// Build a runtime key from a Facet value.
    pub fn from_facet<T>(value: T) -> PicanteResult<Self>
    where
        T: Facet<'static> + Send + Sync + 'static,
    {
        KeyFactory::<T>::new().key(value)
    }

    /// Build a runtime key from an already-shared Facet value.
    pub fn from_facet_arc<T>(value: Arc<T>) -> PicanteResult<Self>
    where
        T: Facet<'static> + Send + Sync + 'static,
    {
        KeyFactory::<T>::new().key_arc(value)
    }

    /// Encode a key using `facet-postcard`.
    pub fn encode_facet<T: Facet<'static>>(value: &T) -> PicanteResult<Self> {
        let bytes = facet_postcard::to_vec(value).map_err(|e| {
            Arc::new(PicanteError::Encode {
                what: "key",
                message: format!("{e:?}"),
            })
        })?;
        let hash = if let Ok(plan) = facet_hash::HashPlan::<T>::build() {
            plan.hash64(value).map_err(|e| {
                Arc::new(PicanteError::Encode {
                    what: "key hash",
                    message: e.to_string(),
                })
            })?
        } else {
            stable_hash(&bytes)
        };
        Ok(Self {
            repr: Arc::new(KeyRepr::Bytes(bytes.into())),
            hash,
        })
    }

    fn typed_value<T>(&self) -> Option<T>
    where
        T: Clone + Facet<'static> + Send + Sync + 'static,
    {
        if let KeyRepr::Typed(typed) = &*self.repr
            && let Some(value) = typed
                .as_any()
                .downcast_ref::<FacetKey<T>>()
                .map(|key| (*key.value).clone())
        {
            return Some(value);
        }
        None
    }

    /// Decode a key using `facet-postcard`.
    pub fn decode_facet<T: Facet<'static>>(&self) -> PicanteResult<T> {
        let bytes = self.to_bytes()?;
        facet_postcard::from_slice(&bytes).map_err(|e| {
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
            repr: Arc::new(KeyRepr::Bytes(bytes.into())),
            hash,
        }
    }

    /// Access the encoded bytes, materializing them for typed keys if needed.
    pub fn bytes(&self) -> &[u8] {
        self.try_bytes()
            .unwrap_or_else(|e| panic!("encode key bytes failed: {e}"))
    }

    /// Try to access the encoded bytes without panicking on materialization errors.
    pub fn try_bytes(&self) -> PicanteResult<&[u8]> {
        match &*self.repr {
            KeyRepr::Typed(typed) => typed.bytes(),
            KeyRepr::Bytes(bytes) => Ok(bytes),
        }
    }

    /// Return the persistent byte representation of this key.
    pub fn to_bytes(&self) -> PicanteResult<Arc<[u8]>> {
        match &*self.repr {
            KeyRepr::Typed(typed) => typed.to_bytes(),
            KeyRepr::Bytes(bytes) => Ok(bytes.clone()),
        }
    }

    /// Cached hash used for runtime indexing/tracing.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Length in bytes of the encoded key.
    pub fn len(&self) -> usize {
        self.bytes().len()
    }

    /// Returns `true` if the encoded key is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes().is_empty()
    }
}

// r[key.equality]
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        match (&*self.repr, &*other.repr) {
            (KeyRepr::Typed(left), KeyRepr::Typed(right)) => left.eq_typed(&**right),
            (KeyRepr::Bytes(left), KeyRepr::Bytes(right)) => left == right,
            _ => match (self.to_bytes(), other.to_bytes()) {
                (Ok(left), Ok(right)) => left == right,
                _ => false,
            },
        }
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
    facet_hash::hash_bytes_fnv1a64(bytes)
}

/// Reusable key builder for one Facet key type.
pub struct KeyFactory<T> {
    plan: Option<facet_hash::HashPlan<T>>,
}

impl<T> KeyFactory<T>
where
    T: Facet<'static>,
{
    /// Build a key factory for `T`.
    pub fn new() -> Self {
        Self {
            plan: facet_hash::HashPlan::<T>::build().ok(),
        }
    }

    /// Build a key from an owned value.
    pub fn key(&self, value: T) -> PicanteResult<Key>
    where
        T: Send + Sync + 'static,
    {
        self.key_arc(Arc::new(value))
    }

    /// Build a key from an already-shared value.
    pub fn key_arc(&self, value: Arc<T>) -> PicanteResult<Key>
    where
        T: Send + Sync + 'static,
    {
        let mut bytes = None;
        let hash = if let Some(plan) = &self.plan {
            plan.hash64(&*value).map_err(|e| {
                Arc::new(PicanteError::Encode {
                    what: "key hash",
                    message: e.to_string(),
                })
            })?
        } else {
            let encoded = encode_key_bytes(&*value)?;
            let hash = stable_hash(&encoded);
            bytes = Some(encoded);
            hash
        };

        let key = FacetKey {
            value,
            bytes: once_lock_from_option(bytes.map(Ok)),
        };

        Ok(Key {
            repr: Arc::new(KeyRepr::Typed(Arc::new(key))),
            hash,
        })
    }

    /// Decode persistent key bytes and rehydrate them as a typed runtime key.
    pub fn key_from_bytes(&self, bytes: Vec<u8>) -> PicanteResult<Key>
    where
        T: Send + Sync + 'static,
    {
        let value: T = facet_postcard::from_slice(&bytes).map_err(|e| {
            Arc::new(PicanteError::Decode {
                what: "key",
                message: format!("{e:?}"),
            })
        })?;
        let bytes: Arc<[u8]> = bytes.into();
        let hash = if let Some(plan) = &self.plan {
            plan.hash64(&value).map_err(|e| {
                Arc::new(PicanteError::Encode {
                    what: "key hash",
                    message: e.to_string(),
                })
            })?
        } else {
            stable_hash(&bytes)
        };
        let key = FacetKey {
            value: Arc::new(value),
            bytes: once_lock_from_option(Some(Ok(bytes))),
        };
        Ok(Key {
            repr: Arc::new(KeyRepr::Typed(Arc::new(key))),
            hash,
        })
    }

    /// Normalize an erased key into this factory's typed runtime representation.
    pub fn normalize_key(&self, key: Key) -> PicanteResult<Key>
    where
        T: Clone + Send + Sync + 'static,
    {
        let already_typed = match &*key.repr {
            KeyRepr::Typed(typed) => typed.as_any().is::<FacetKey<T>>(),
            KeyRepr::Bytes(_) => false,
        };
        if already_typed {
            return Ok(key);
        }

        self.key(key.decode_facet::<T>()?)
    }

    /// Decode a key, reusing the typed value when the erased key already has one.
    pub fn decode_key(&self, key: &Key) -> PicanteResult<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        if let Some(value) = key.typed_value::<T>() {
            return Ok(value);
        }
        key.decode_facet::<T>()
    }
}

impl<T> Default for KeyFactory<T>
where
    T: Facet<'static>,
{
    fn default() -> Self {
        Self::new()
    }
}

fn encode_key_bytes<T>(value: &T) -> PicanteResult<Arc<[u8]>>
where
    T: Facet<'static>,
{
    facet_postcard::to_vec(value).map(Arc::from).map_err(|e| {
        Arc::new(PicanteError::Encode {
            what: "key",
            message: format!("{e:?}"),
        })
    })
}

fn once_lock_from_option<T>(value: Option<T>) -> OnceLock<T> {
    let lock = OnceLock::new();
    if let Some(value) = value {
        let _ = lock.set(value);
    }
    lock
}
