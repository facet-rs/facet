//! Extensible "Other" value types using tag 7 with a secondary discriminant.
//!
//! This module provides types that share tag 7 but are distinguished by a
//! secondary `OtherKind` discriminant stored on the heap. This allows for
//! unlimited future extensibility without consuming additional tag bits.
//!
//! Current types:
//! - `VQName`: Qualified name (namespace + local name) for XML namespace support
//! - `VUuid`: 128-bit UUID for preserving semantic identity

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc};
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};

use crate::value::{TypeTag, Value};

/// Secondary discriminant for "Other" types (tag 7).
///
/// This allows 256 subtypes to share a single tag value.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OtherKind {
    /// Qualified name (namespace + local name)
    QName = 0,
    /// UUID (128-bit universally unique identifier)
    Uuid = 1,
}

// ============================================================================
// VQName - Qualified Name
// ============================================================================

/// Header for VQName values.
///
/// Layout: [kind: u8][_pad: 7 bytes][namespace: Value][local_name: Value]
#[repr(C, align(8))]
struct QNameHeader {
    /// The OtherKind discriminant (always QName = 0)
    kind: OtherKind,
    /// Padding for alignment
    _pad: [u8; 7],
    /// Optional namespace (Value::NULL if none)
    namespace: Value,
    /// Local name (always a VString)
    local_name: Value,
}

/// A qualified name consisting of an optional namespace and a local name.
///
/// `VQName` is used for XML namespace support, where elements and attributes
/// can have qualified names like `{http://example.com}element`.
///
/// Both the namespace and local name are stored as `Value`s, allowing them
/// to benefit from inline string optimization for short names.
#[repr(transparent)]
pub struct VQName(pub(crate) Value);

impl VQName {
    const fn layout() -> Layout {
        Layout::new::<QNameHeader>()
    }

    #[cfg(feature = "alloc")]
    fn alloc() -> *mut QNameHeader {
        unsafe { alloc(Self::layout()).cast::<QNameHeader>() }
    }

    #[cfg(feature = "alloc")]
    fn dealloc(ptr: *mut QNameHeader) {
        unsafe {
            dealloc(ptr.cast::<u8>(), Self::layout());
        }
    }

    fn header(&self) -> &QNameHeader {
        unsafe { &*(self.0.heap_ptr() as *const QNameHeader) }
    }

    /// Creates a new qualified name with a namespace and local name.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new(namespace: impl Into<Value>, local_name: impl Into<Value>) -> Self {
        unsafe {
            let ptr = Self::alloc();
            // Use ptr::write to avoid dropping uninitialized memory
            core::ptr::write(&raw mut (*ptr).kind, OtherKind::QName);
            core::ptr::write(&raw mut (*ptr)._pad, [0; 7]);
            core::ptr::write(&raw mut (*ptr).namespace, namespace.into());
            core::ptr::write(&raw mut (*ptr).local_name, local_name.into());
            VQName(Value::new_ptr(ptr.cast(), TypeTag::Other))
        }
    }

    /// Creates a new qualified name without a namespace.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new_local(local_name: impl Into<Value>) -> Self {
        Self::new(Value::NULL, local_name)
    }

    /// Returns the namespace, or `None` if there is no namespace.
    #[must_use]
    pub fn namespace(&self) -> Option<&Value> {
        let ns = &self.header().namespace;
        if ns.is_null() { None } else { Some(ns) }
    }

    /// Returns the local name.
    #[must_use]
    pub fn local_name(&self) -> &Value {
        &self.header().local_name
    }

    /// Returns `true` if this qualified name has a namespace.
    #[must_use]
    pub fn has_namespace(&self) -> bool {
        !self.header().namespace.is_null()
    }

    // === Internal ===

    pub(crate) fn clone_impl(&self) -> Value {
        #[cfg(feature = "alloc")]
        {
            let h = self.header();
            Self::new(h.namespace.clone(), h.local_name.clone()).0
        }
        #[cfg(not(feature = "alloc"))]
        {
            panic!("cannot clone VQName without alloc feature")
        }
    }

    pub(crate) fn drop_impl(&mut self) {
        #[cfg(feature = "alloc")]
        unsafe {
            let ptr = self.0.heap_ptr_mut() as *mut QNameHeader;
            // Drop the contained Values
            core::ptr::drop_in_place(&mut (*ptr).namespace);
            core::ptr::drop_in_place(&mut (*ptr).local_name);
            Self::dealloc(ptr);
        }
    }
}

impl Clone for VQName {
    fn clone(&self) -> Self {
        VQName(self.clone_impl())
    }
}

impl PartialEq for VQName {
    fn eq(&self, other: &Self) -> bool {
        let (h1, h2) = (self.header(), other.header());
        h1.namespace == h2.namespace && h1.local_name == h2.local_name
    }
}

impl Eq for VQName {}

impl Hash for VQName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let h = self.header();
        h.namespace.hash(state);
        h.local_name.hash(state);
    }
}

impl Debug for VQName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let h = self.header();
        if h.namespace.is_null() {
            write!(f, "{:?}", h.local_name)
        } else {
            write!(f, "{{{:?}}}{:?}", h.namespace, h.local_name)
        }
    }
}

#[cfg(feature = "alloc")]
impl From<VQName> for Value {
    fn from(qname: VQName) -> Self {
        qname.0
    }
}

// ============================================================================
// VUuid - UUID
// ============================================================================

/// Header for VUuid values.
///
/// Layout: [kind: u8][_pad: 7 bytes][uuid_bytes: 16 bytes]
#[repr(C, align(8))]
struct UuidHeader {
    /// The OtherKind discriminant (always Uuid = 1)
    kind: OtherKind,
    /// Padding for alignment
    _pad: [u8; 7],
    /// The 128-bit UUID in big-endian byte order
    bytes: [u8; 16],
}

/// A 128-bit universally unique identifier (UUID).
///
/// `VUuid` stores UUIDs in their native 128-bit form rather than as
/// 36-character strings, preserving semantic identity while being more
/// memory-efficient.
#[repr(transparent)]
pub struct VUuid(pub(crate) Value);

impl VUuid {
    const fn layout() -> Layout {
        Layout::new::<UuidHeader>()
    }

    #[cfg(feature = "alloc")]
    fn alloc() -> *mut UuidHeader {
        unsafe { alloc(Self::layout()).cast::<UuidHeader>() }
    }

    #[cfg(feature = "alloc")]
    fn dealloc(ptr: *mut UuidHeader) {
        unsafe {
            dealloc(ptr.cast::<u8>(), Self::layout());
        }
    }

    fn header(&self) -> &UuidHeader {
        unsafe { &*(self.0.heap_ptr() as *const UuidHeader) }
    }

    /// Creates a new UUID from 16 bytes (big-endian).
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new(bytes: [u8; 16]) -> Self {
        unsafe {
            let ptr = Self::alloc();
            // Use ptr::write to avoid dropping uninitialized memory
            core::ptr::write(&raw mut (*ptr).kind, OtherKind::Uuid);
            core::ptr::write(&raw mut (*ptr)._pad, [0; 7]);
            core::ptr::write(&raw mut (*ptr).bytes, bytes);
            VUuid(Value::new_ptr(ptr.cast(), TypeTag::Other))
        }
    }

    /// Creates a new UUID from two 64-bit integers (high and low parts).
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_u64_pair(high: u64, low: u64) -> Self {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&high.to_be_bytes());
        bytes[8..].copy_from_slice(&low.to_be_bytes());
        Self::new(bytes)
    }

    /// Creates a new UUID from a u128.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_u128(value: u128) -> Self {
        Self::new(value.to_be_bytes())
    }

    /// Returns the UUID as 16 bytes (big-endian).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.header().bytes
    }

    /// Returns the UUID as a u128.
    #[must_use]
    pub fn as_u128(&self) -> u128 {
        u128::from_be_bytes(self.header().bytes)
    }

    /// Returns the high 64 bits of the UUID.
    #[must_use]
    pub fn high(&self) -> u64 {
        let bytes = &self.header().bytes;
        u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }

    /// Returns the low 64 bits of the UUID.
    #[must_use]
    pub fn low(&self) -> u64 {
        let bytes = &self.header().bytes;
        u64::from_be_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ])
    }

    // === Internal ===

    pub(crate) fn clone_impl(&self) -> Value {
        #[cfg(feature = "alloc")]
        {
            Self::new(self.header().bytes).0
        }
        #[cfg(not(feature = "alloc"))]
        {
            panic!("cannot clone VUuid without alloc feature")
        }
    }

    pub(crate) fn drop_impl(&mut self) {
        #[cfg(feature = "alloc")]
        unsafe {
            Self::dealloc(self.0.heap_ptr_mut().cast());
        }
    }
}

impl Clone for VUuid {
    fn clone(&self) -> Self {
        VUuid(self.clone_impl())
    }
}

impl PartialEq for VUuid {
    fn eq(&self, other: &Self) -> bool {
        self.header().bytes == other.header().bytes
    }
}

impl Eq for VUuid {}

impl Hash for VUuid {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.header().bytes.hash(state);
    }
}

impl Debug for VUuid {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let bytes = &self.header().bytes;
        // Format as standard UUID: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
            bytes[4],
            bytes[5],
            bytes[6],
            bytes[7],
            bytes[8],
            bytes[9],
            bytes[10],
            bytes[11],
            bytes[12],
            bytes[13],
            bytes[14],
            bytes[15]
        )
    }
}

#[cfg(feature = "alloc")]
impl From<VUuid> for Value {
    fn from(uuid: VUuid) -> Self {
        uuid.0
    }
}

#[cfg(feature = "alloc")]
impl From<[u8; 16]> for VUuid {
    fn from(bytes: [u8; 16]) -> Self {
        Self::new(bytes)
    }
}

#[cfg(feature = "alloc")]
impl From<u128> for VUuid {
    fn from(value: u128) -> Self {
        Self::from_u128(value)
    }
}

// ============================================================================
// Helper to get OtherKind from a Value with tag 7
// ============================================================================

/// Returns the OtherKind for a Value that has TypeTag::Other.
///
/// # Safety
/// The value must have TypeTag::Other (tag 7) and point to valid heap memory.
pub(crate) unsafe fn get_other_kind(value: &Value) -> OtherKind {
    // The first byte of any Other header is the OtherKind discriminant
    let ptr = value.heap_ptr();
    unsafe { *(ptr as *const OtherKind) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VString;

    #[test]
    fn test_qname_with_namespace() {
        let qname = VQName::new(VString::new("http://example.com"), VString::new("element"));
        assert!(qname.has_namespace());
        assert_eq!(
            qname.namespace().unwrap().as_string().unwrap().as_str(),
            "http://example.com"
        );
        assert_eq!(qname.local_name().as_string().unwrap().as_str(), "element");
    }

    #[test]
    fn test_qname_local_only() {
        let qname = VQName::new_local(VString::new("element"));
        assert!(!qname.has_namespace());
        assert!(qname.namespace().is_none());
        assert_eq!(qname.local_name().as_string().unwrap().as_str(), "element");
    }

    #[test]
    fn test_qname_clone() {
        let qname = VQName::new(VString::new("ns"), VString::new("local"));
        let cloned = qname.clone();
        assert_eq!(qname, cloned);
    }

    #[test]
    fn test_qname_debug() {
        let qname = VQName::new(VString::new("ns"), VString::new("local"));
        let debug = format!("{qname:?}");
        assert!(debug.contains("ns"));
        assert!(debug.contains("local"));
    }

    #[test]
    fn test_uuid_new() {
        let bytes = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let uuid = VUuid::new(bytes);
        assert_eq!(uuid.as_bytes(), &bytes);
    }

    #[test]
    fn test_uuid_from_u128() {
        let value: u128 = 0x0102030405060708090a0b0c0d0e0f10;
        let uuid = VUuid::from_u128(value);
        assert_eq!(uuid.as_u128(), value);
    }

    #[test]
    fn test_uuid_high_low() {
        let uuid = VUuid::from_u64_pair(0x0102030405060708, 0x090a0b0c0d0e0f10);
        assert_eq!(uuid.high(), 0x0102030405060708);
        assert_eq!(uuid.low(), 0x090a0b0c0d0e0f10);
    }

    #[test]
    fn test_uuid_clone() {
        let uuid = VUuid::from_u128(0x12345678_9abc_def0_1234_56789abcdef0);
        let cloned = uuid.clone();
        assert_eq!(uuid, cloned);
    }

    #[test]
    fn test_uuid_debug_format() {
        let uuid = VUuid::from_u128(0x12345678_9abc_def0_1234_56789abcdef0);
        let debug = format!("{uuid:?}");
        assert_eq!(debug, "12345678-9abc-def0-1234-56789abcdef0");
    }
}
