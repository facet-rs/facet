//! Query key encoding and erased identifiers used for dependency graphs.

use crate::error::{PicanteError, PicanteResult};
use facet::Facet;
use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::BuildHasher;
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
    repr: KeyRepr,
    // r[key.hash]
    hash: u64,
}

#[derive(Clone)]
enum KeyRepr {
    Inline(InlineKey, Arc<dyn ErasedKeyBytes>),
    Typed(Arc<dyn ErasedFacetKey>),
    Bytes(Arc<[u8]>),
}

#[derive(Clone, Eq, PartialEq)]
enum InlineKey {
    Unit,
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
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

trait ErasedKeyBytes: Send + Sync {
    fn bytes(&self) -> PicanteResult<&[u8]>;
    fn to_bytes(&self) -> PicanteResult<Arc<[u8]>>;
}

struct InlineKeyBytes {
    key: InlineKey,
    bytes: OnceLock<PicanteResult<Arc<[u8]>>>,
}

impl InlineKeyBytes {
    fn new(key: InlineKey, bytes: Option<Arc<[u8]>>) -> Self {
        Self {
            key,
            bytes: once_lock_from_option(bytes.map(Ok)),
        }
    }
}

impl ErasedKeyBytes for InlineKeyBytes {
    fn bytes(&self) -> PicanteResult<&[u8]> {
        self.bytes
            .get_or_init(|| self.key.to_bytes())
            .as_ref()
            .map(|bytes| bytes.as_ref())
            .map_err(Clone::clone)
    }

    fn to_bytes(&self) -> PicanteResult<Arc<[u8]>> {
        self.bytes.get_or_init(|| self.key.to_bytes()).clone()
    }
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
            .is_some_and(|other| {
                eq_known_key_type(&*self.value, &*other.value).unwrap_or_else(|| {
                    crate::facet_eq::facet_eq_direct(&*self.value, &*other.value)
                })
            })
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
        let hash = if let Ok(hash) = hash_with_temp_plan(value) {
            hash
        } else {
            stable_hash(&bytes)
        };
        Ok(Self {
            repr: KeyRepr::Bytes(bytes.into()),
            hash,
        })
    }

    fn typed_value<T>(&self) -> Option<T>
    where
        T: Clone + Facet<'static> + Send + Sync + 'static,
    {
        match &self.repr {
            KeyRepr::Inline(inline, _) => inline.typed_value(),
            KeyRepr::Typed(typed) => typed
                .as_any()
                .downcast_ref::<FacetKey<T>>()
                .map(|key| (*key.value).clone()),
            KeyRepr::Bytes(_) => None,
        }
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
            repr: KeyRepr::Bytes(bytes.into()),
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
        match &self.repr {
            KeyRepr::Inline(_, bytes) => bytes.bytes(),
            KeyRepr::Typed(typed) => typed.bytes(),
            KeyRepr::Bytes(bytes) => Ok(bytes),
        }
    }

    /// Return the persistent byte representation of this key.
    pub fn to_bytes(&self) -> PicanteResult<Arc<[u8]>> {
        match &self.repr {
            KeyRepr::Inline(_, bytes) => bytes.to_bytes(),
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
        match (&self.repr, &other.repr) {
            (KeyRepr::Inline(left, _), KeyRepr::Inline(right, _)) => left == right,
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
            .field("repr", &self.repr.kind_name())
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

#[derive(Clone, Default)]
pub(crate) struct KeyBuildHasher;

#[derive(Default)]
pub(crate) struct KeyHasher {
    hash: u64,
    fallback: Option<DefaultHasher>,
}

impl BuildHasher for KeyBuildHasher {
    type Hasher = KeyHasher;

    fn build_hasher(&self) -> Self::Hasher {
        KeyHasher::default()
    }
}

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.fallback
            .as_ref()
            .map_or(self.hash, DefaultHasher::finish)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.fallback
            .get_or_insert_with(DefaultHasher::new)
            .write(bytes);
    }

    fn write_u64(&mut self, i: u64) {
        if let Some(fallback) = &mut self.fallback {
            fallback.write_u64(i);
        } else {
            self.hash = i;
        }
    }
}

pub(crate) type KeyMap<V> = im::HashMap<Key, V, KeyBuildHasher>;

pub(crate) fn key_map<V>() -> KeyMap<V> {
    im::HashMap::with_hasher(KeyBuildHasher)
}

pub(crate) type RuntimeKeyMap<K, V> = hashbrown::HashMap<RuntimeKey<K>, V, KeyBuildHasher>;

pub(crate) fn runtime_key_map<K, V>() -> RuntimeKeyMap<K, V> {
    hashbrown::HashMap::with_hasher(KeyBuildHasher)
}

pub(crate) struct RuntimeKey<T> {
    value: T,
    hash: u64,
    bytes: Option<Arc<[u8]>>,
}

impl<T> RuntimeKey<T>
where
    T: Clone + Facet<'static> + Send + Sync + 'static,
{
    fn new(value: T, hash: u64, bytes: Option<Arc<[u8]>>) -> Self {
        Self { value, hash, bytes }
    }

    pub(crate) fn value(&self) -> &T {
        &self.value
    }

    pub(crate) fn hash(&self) -> u64 {
        self.hash
    }

    pub(crate) fn matches(&self, value: &T) -> bool {
        eq_known_key_type(&self.value, value)
            .unwrap_or_else(|| crate::facet_eq::facet_eq_direct(&self.value, value))
    }

    pub(crate) fn to_key(&self) -> Key {
        if let Some(inline) = InlineKey::from_value(&self.value) {
            return Key {
                repr: KeyRepr::Inline(
                    inline.clone(),
                    Arc::new(InlineKeyBytes::new(inline, self.bytes.clone())),
                ),
                hash: self.hash,
            };
        }

        Key {
            repr: KeyRepr::Typed(Arc::new(FacetKey {
                value: Arc::new(self.value.clone()),
                bytes: once_lock_from_option(self.bytes.clone().map(Ok)),
            })),
            hash: self.hash,
        }
    }
}

impl<T> Clone for RuntimeKey<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            hash: self.hash,
            bytes: self.bytes.clone(),
        }
    }
}

impl<T> PartialEq for RuntimeKey<T>
where
    T: Clone + Facet<'static> + Send + Sync + 'static,
{
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.matches(&other.value)
    }
}

impl<T> Eq for RuntimeKey<T> where T: Clone + Facet<'static> + Send + Sync + 'static {}

impl<T> Hash for RuntimeKey<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl KeyRepr {
    fn kind_name(&self) -> &'static str {
        match self {
            KeyRepr::Inline(_, _) => "inline",
            KeyRepr::Typed(_) => "typed",
            KeyRepr::Bytes(_) => "bytes",
        }
    }
}

impl InlineKey {
    fn from_value<T>(value: &T) -> Option<Self>
    where
        T: 'static,
    {
        let value = value as &dyn Any;

        macro_rules! copy_as {
            ($ty:ty, $variant:ident) => {
                if let Some(value) = value.downcast_ref::<$ty>() {
                    return Some(Self::$variant(*value));
                }
            };
        }

        if value.is::<()>() {
            return Some(Self::Unit);
        }
        copy_as!(bool, Bool);
        copy_as!(u8, U8);
        copy_as!(u16, U16);
        copy_as!(u32, U32);
        copy_as!(u64, U64);
        copy_as!(u128, U128);
        copy_as!(usize, Usize);
        copy_as!(i8, I8);
        copy_as!(i16, I16);
        copy_as!(i32, I32);
        copy_as!(i64, I64);
        copy_as!(i128, I128);
        copy_as!(isize, Isize);

        None
    }

    fn typed_value<T>(&self) -> Option<T>
    where
        T: Clone + 'static,
    {
        macro_rules! clone_as {
            ($value:expr) => {{
                let value = $value;
                return (value as &dyn Any).downcast_ref::<T>().cloned();
            }};
        }

        match self {
            Self::Unit => {
                let value = ();
                clone_as!(&value);
            }
            Self::Bool(value) => clone_as!(value),
            Self::U8(value) => clone_as!(value),
            Self::U16(value) => clone_as!(value),
            Self::U32(value) => clone_as!(value),
            Self::U64(value) => clone_as!(value),
            Self::U128(value) => clone_as!(value),
            Self::Usize(value) => clone_as!(value),
            Self::I8(value) => clone_as!(value),
            Self::I16(value) => clone_as!(value),
            Self::I32(value) => clone_as!(value),
            Self::I64(value) => clone_as!(value),
            Self::I128(value) => clone_as!(value),
            Self::Isize(value) => clone_as!(value),
        }
    }

    fn to_bytes(&self) -> PicanteResult<Arc<[u8]>> {
        macro_rules! encode {
            ($value:expr) => {
                encode_key_bytes($value)
            };
        }

        match self {
            Self::Unit => encode!(&()),
            Self::Bool(value) => encode!(value),
            Self::U8(value) => encode!(value),
            Self::U16(value) => encode!(value),
            Self::U32(value) => encode!(value),
            Self::U64(value) => encode!(value),
            Self::U128(value) => encode!(value),
            Self::Usize(value) => encode!(value),
            Self::I8(value) => encode!(value),
            Self::I16(value) => encode!(value),
            Self::I32(value) => encode!(value),
            Self::I64(value) => encode!(value),
            Self::I128(value) => encode!(value),
            Self::Isize(value) => encode!(value),
        }
    }
}

fn key_hash_error(error: facet_hash::HashError) -> Arc<PicanteError> {
    Arc::new(PicanteError::Encode {
        what: "key hash",
        message: error.to_string(),
    })
}

fn hash_with_temp_plan<T>(value: &T) -> Result<u64, facet_hash::HashError>
where
    T: Facet<'static>,
{
    KeyHashPlan::<T>::build()?.hash64(value)
}

fn eq_known_key_type<T: 'static>(left: &T, right: &T) -> Option<bool> {
    let left = left as &dyn Any;
    let right = right as &dyn Any;

    macro_rules! eq_as {
        ($ty:ty) => {
            if let Some(left) = left.downcast_ref::<$ty>() {
                return Some(left == right.downcast_ref::<$ty>()?);
            }
        };
    }

    eq_as!(u32);
    eq_as!(());
    eq_as!(String);
    eq_as!(u64);
    eq_as!(bool);
    eq_as!(u16);
    eq_as!(u8);
    eq_as!(u128);
    eq_as!(usize);
    eq_as!(i32);
    eq_as!(i64);
    eq_as!(i16);
    eq_as!(i8);
    eq_as!(i128);
    eq_as!(isize);

    None
}

enum KeyHashPlan<T> {
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    Native(facet_hash::NativeHashPlan<T>),
    Interpreted(facet_hash::HashPlan<T>),
}

impl<T> KeyHashPlan<T>
where
    T: Facet<'static>,
{
    fn build() -> Result<Self, facet_hash::HashError> {
        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        if let Ok(plan) = facet_hash::NativeHashPlan::<T>::build() {
            return Ok(Self::Native(plan));
        }

        facet_hash::HashPlan::<T>::build().map(Self::Interpreted)
    }

    fn hash64(&self, value: &T) -> Result<u64, facet_hash::HashError> {
        match self {
            #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
            Self::Native(plan) => plan.hash64(value),
            Self::Interpreted(plan) => plan.hash64(value),
        }
    }
}

/// Reusable key builder for one Facet key type.
pub struct KeyFactory<T> {
    plan: Option<KeyHashPlan<T>>,
}

impl<T> KeyFactory<T>
where
    T: Facet<'static>,
{
    /// Build a key factory for `T`.
    pub fn new() -> Self {
        Self {
            plan: KeyHashPlan::<T>::build().ok(),
        }
    }

    pub(crate) fn hash_parts(&self, value: &T) -> PicanteResult<(u64, Option<Arc<[u8]>>)> {
        if let Some(plan) = &self.plan {
            return Ok((plan.hash64(value).map_err(key_hash_error)?, None));
        }

        let bytes = encode_key_bytes(value)?;
        Ok((stable_hash(&bytes), Some(bytes)))
    }

    pub(crate) fn hash_borrowed(&self, value: &T) -> PicanteResult<u64> {
        self.hash_parts(value).map(|(hash, _)| hash)
    }

    pub(crate) fn runtime_key(&self, value: T) -> PicanteResult<RuntimeKey<T>>
    where
        T: Clone + Send + Sync + 'static,
    {
        let (hash, bytes) = self.hash_parts(&value)?;
        Ok(self.runtime_key_from_parts(value, hash, bytes))
    }

    pub(crate) fn runtime_key_from_parts(
        &self,
        value: T,
        hash: u64,
        bytes: Option<Arc<[u8]>>,
    ) -> RuntimeKey<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        RuntimeKey::new(value, hash, bytes)
    }

    /// Build a key from an owned value.
    pub fn key(&self, value: T) -> PicanteResult<Key>
    where
        T: Send + Sync + 'static,
    {
        let (hash, bytes) = self.hash_parts(&value)?;
        Ok(self.key_with_parts(value, hash, bytes))
    }

    pub(crate) fn key_with_parts(&self, value: T, hash: u64, bytes: Option<Arc<[u8]>>) -> Key
    where
        T: Send + Sync + 'static,
    {
        if let Some(inline) = InlineKey::from_value(&value) {
            return Key {
                repr: KeyRepr::Inline(inline.clone(), Arc::new(InlineKeyBytes::new(inline, bytes))),
                hash,
            };
        }

        self.key_arc_with_hash(Arc::new(value), hash, bytes)
    }

    /// Build a key from an already-shared value.
    pub fn key_arc(&self, value: Arc<T>) -> PicanteResult<Key>
    where
        T: Send + Sync + 'static,
    {
        let (hash, bytes) = self.hash_parts(&*value)?;
        if let Some(inline) = InlineKey::from_value(&*value) {
            return Ok(Key {
                repr: KeyRepr::Inline(inline.clone(), Arc::new(InlineKeyBytes::new(inline, bytes))),
                hash,
            });
        }

        Ok(self.key_arc_with_hash(value, hash, bytes))
    }

    fn key_arc_with_hash(&self, value: Arc<T>, hash: u64, bytes: Option<Arc<[u8]>>) -> Key
    where
        T: Send + Sync + 'static,
    {
        let key = FacetKey {
            value,
            bytes: once_lock_from_option(bytes.map(Ok)),
        };

        Key {
            repr: KeyRepr::Typed(Arc::new(key)),
            hash,
        }
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
            plan.hash64(&value).map_err(key_hash_error)?
        } else {
            stable_hash(&bytes)
        };
        Ok(self.key_with_parts(value, hash, Some(bytes)))
    }

    /// Normalize an erased key into this factory's typed runtime representation.
    pub fn normalize_key(&self, key: Key) -> PicanteResult<Key>
    where
        T: Clone + Send + Sync + 'static,
    {
        let already_typed = match &key.repr {
            KeyRepr::Inline(_, _) => key.typed_value::<T>().is_some(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_key_hash_plan_uses_native_when_available() {
        let plan = KeyHashPlan::<u32>::build().unwrap();

        #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
        assert!(matches!(plan, KeyHashPlan::Native(_)));

        #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
        assert!(matches!(plan, KeyHashPlan::Interpreted(_)));
    }

    #[test]
    fn aggregate_key_hash_plan_falls_back_to_interpreted() {
        let plan = KeyHashPlan::<Vec<u8>>::build().unwrap();
        assert!(matches!(plan, KeyHashPlan::Interpreted(_)));
    }

    #[test]
    fn inline_scalar_key_keeps_persistent_bytes_and_decode_parity() {
        let inline = Key::from_facet(7u32).unwrap();
        let encoded = Key::encode_facet(&7u32).unwrap();

        assert_eq!(inline, encoded);
        assert_eq!(inline.to_bytes().unwrap(), encoded.to_bytes().unwrap());
        assert_eq!(inline.bytes(), encoded.bytes());
        assert_eq!(inline.decode_facet::<u32>().unwrap(), 7);
    }
}
