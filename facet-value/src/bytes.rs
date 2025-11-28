//! Bytes value type for binary data.

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc};
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use core::ptr;

use crate::value::{TypeTag, Value};

/// Header for heap-allocated bytes.
#[repr(C, align(8))]
struct BytesHeader {
    /// Length of the data in bytes
    len: usize,
    // Byte data follows immediately after
}

/// A binary data value.
///
/// `VBytes` stores arbitrary binary data. This is useful for binary serialization
/// formats like MessagePack, CBOR, etc. that support raw bytes.
#[repr(transparent)]
#[derive(Clone)]
pub struct VBytes(pub(crate) Value);

impl VBytes {
    fn layout(len: usize) -> Layout {
        Layout::new::<BytesHeader>()
            .extend(Layout::array::<u8>(len).unwrap())
            .unwrap()
            .0
            .pad_to_align()
    }

    #[cfg(feature = "alloc")]
    fn alloc(data: &[u8]) -> *mut BytesHeader {
        unsafe {
            let layout = Self::layout(data.len());
            let ptr = alloc(layout).cast::<BytesHeader>();
            (*ptr).len = data.len();

            // Copy byte data
            let data_ptr = ptr.add(1).cast::<u8>();
            ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, data.len());

            ptr
        }
    }

    #[cfg(feature = "alloc")]
    fn dealloc_ptr(ptr: *mut BytesHeader) {
        unsafe {
            let len = (*ptr).len;
            let layout = Self::layout(len);
            dealloc(ptr.cast::<u8>(), layout);
        }
    }

    fn header(&self) -> &BytesHeader {
        unsafe { &*(self.0.heap_ptr() as *const BytesHeader) }
    }

    fn data_ptr(&self) -> *const u8 {
        unsafe { (self.header() as *const BytesHeader).add(1).cast() }
    }

    /// Creates new bytes from a byte slice.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new(data: &[u8]) -> Self {
        if data.is_empty() {
            return Self::empty();
        }
        unsafe {
            let ptr = Self::alloc(data);
            VBytes(Value::new_ptr(ptr.cast(), TypeTag::BytesOrFalse))
        }
    }

    /// Creates empty bytes.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn empty() -> Self {
        unsafe {
            let layout = Self::layout(0);
            let ptr = alloc(layout).cast::<BytesHeader>();
            (*ptr).len = 0;
            VBytes(Value::new_ptr(ptr.cast(), TypeTag::BytesOrFalse))
        }
    }

    /// Returns the length of the bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.header().len
    }

    /// Returns `true` if the bytes are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the data as a byte slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.data_ptr(), self.len()) }
    }

    pub(crate) fn clone_impl(&self) -> Value {
        VBytes::new(self.as_slice()).0
    }

    pub(crate) fn drop_impl(&mut self) {
        unsafe {
            Self::dealloc_ptr(self.0.heap_ptr().cast());
        }
    }
}

impl Deref for VBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Borrow<[u8]> for VBytes {
    fn borrow(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsRef<[u8]> for VBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl PartialEq for VBytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for VBytes {}

impl PartialOrd for VBytes {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VBytes {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl Hash for VBytes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl Debug for VBytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Display as hex for readability
        write!(f, "b\"")?;
        for byte in self.as_slice() {
            write!(f, "\\x{byte:02x}")?;
        }
        write!(f, "\"")
    }
}

impl Default for VBytes {
    fn default() -> Self {
        Self::empty()
    }
}

// === PartialEq with [u8] ===

impl PartialEq<[u8]> for VBytes {
    fn eq(&self, other: &[u8]) -> bool {
        self.as_slice() == other
    }
}

impl PartialEq<VBytes> for [u8] {
    fn eq(&self, other: &VBytes) -> bool {
        self == other.as_slice()
    }
}

impl PartialEq<&[u8]> for VBytes {
    fn eq(&self, other: &&[u8]) -> bool {
        self.as_slice() == *other
    }
}

#[cfg(feature = "alloc")]
impl PartialEq<Vec<u8>> for VBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

#[cfg(feature = "alloc")]
impl PartialEq<VBytes> for Vec<u8> {
    fn eq(&self, other: &VBytes) -> bool {
        self.as_slice() == other.as_slice()
    }
}

// === From implementations ===

#[cfg(feature = "alloc")]
impl From<&[u8]> for VBytes {
    fn from(data: &[u8]) -> Self {
        Self::new(data)
    }
}

#[cfg(feature = "alloc")]
impl From<Vec<u8>> for VBytes {
    fn from(data: Vec<u8>) -> Self {
        Self::new(&data)
    }
}

#[cfg(feature = "alloc")]
impl From<&Vec<u8>> for VBytes {
    fn from(data: &Vec<u8>) -> Self {
        Self::new(data)
    }
}

#[cfg(feature = "alloc")]
impl From<VBytes> for Vec<u8> {
    fn from(b: VBytes) -> Self {
        b.as_slice().to_vec()
    }
}

// === Value conversions ===

impl AsRef<Value> for VBytes {
    fn as_ref(&self) -> &Value {
        &self.0
    }
}

impl AsMut<Value> for VBytes {
    fn as_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}

impl From<VBytes> for Value {
    fn from(b: VBytes) -> Self {
        b.0
    }
}

#[cfg(feature = "alloc")]
impl From<&[u8]> for Value {
    fn from(data: &[u8]) -> Self {
        VBytes::new(data).0
    }
}

#[cfg(feature = "alloc")]
impl From<Vec<u8>> for Value {
    fn from(data: Vec<u8>) -> Self {
        VBytes::new(&data).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let b = VBytes::new(&[1, 2, 3, 4, 5]);
        assert_eq!(b.as_slice(), &[1, 2, 3, 4, 5]);
        assert_eq!(b.len(), 5);
        assert!(!b.is_empty());
    }

    #[test]
    fn test_empty() {
        let b = VBytes::empty();
        assert_eq!(b.as_slice(), &[]);
        assert_eq!(b.len(), 0);
        assert!(b.is_empty());
    }

    #[test]
    fn test_equality() {
        let a = VBytes::new(&[1, 2, 3]);
        let b = VBytes::new(&[1, 2, 3]);
        let c = VBytes::new(&[4, 5, 6]);

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, [1, 2, 3].as_slice());
    }

    #[test]
    fn test_clone() {
        let a = VBytes::new(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_ordering() {
        let a = VBytes::new(&[1, 2, 3]);
        let b = VBytes::new(&[1, 2, 4]);
        assert!(a < b);
    }

    #[test]
    fn test_debug() {
        let b = VBytes::new(&[0xDE, 0xAD]);
        let s = format!("{b:?}");
        assert_eq!(s, "b\"\\xde\\xad\"");
    }
}
