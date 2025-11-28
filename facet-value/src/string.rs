//! String value type.

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc};
#[cfg(feature = "alloc")]
use alloc::string::String;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};
use core::ops::Deref;
use core::ptr;

use crate::value::{TypeTag, Value};

/// Header for heap-allocated strings.
#[repr(C, align(8))]
struct StringHeader {
    /// Length of the string in bytes
    len: usize,
    // String data follows immediately after
}

/// A string value.
///
/// `VString` stores UTF-8 string data. Unlike some implementations, strings are
/// not interned - each `VString` owns its own copy of the data.
#[repr(transparent)]
#[derive(Clone)]
pub struct VString(pub(crate) Value);

impl VString {
    fn layout(len: usize) -> Layout {
        Layout::new::<StringHeader>()
            .extend(Layout::array::<u8>(len).unwrap())
            .unwrap()
            .0
            .pad_to_align()
    }

    #[cfg(feature = "alloc")]
    fn alloc(s: &str) -> *mut StringHeader {
        unsafe {
            let layout = Self::layout(s.len());
            let ptr = alloc(layout).cast::<StringHeader>();
            (*ptr).len = s.len();

            // Copy string data
            let data_ptr = ptr.add(1).cast::<u8>();
            ptr::copy_nonoverlapping(s.as_ptr(), data_ptr, s.len());

            ptr
        }
    }

    #[cfg(feature = "alloc")]
    fn dealloc_ptr(ptr: *mut StringHeader) {
        unsafe {
            let len = (*ptr).len;
            let layout = Self::layout(len);
            dealloc(ptr.cast::<u8>(), layout);
        }
    }

    fn header(&self) -> &StringHeader {
        unsafe { &*(self.0.heap_ptr() as *const StringHeader) }
    }

    fn data_ptr(&self) -> *const u8 {
        unsafe { (self.header() as *const StringHeader).add(1).cast() }
    }

    /// Creates a new string from a `&str`.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new(s: &str) -> Self {
        if s.is_empty() {
            return Self::empty();
        }
        unsafe {
            let ptr = Self::alloc(s);
            VString(Value::new_ptr(ptr.cast(), TypeTag::StringOrNull))
        }
    }

    /// Creates an empty string.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn empty() -> Self {
        // For empty strings, we still allocate a header with len=0
        // This keeps the code simpler
        unsafe {
            let layout = Self::layout(0);
            let ptr = alloc(layout).cast::<StringHeader>();
            (*ptr).len = 0;
            VString(Value::new_ptr(ptr.cast(), TypeTag::StringOrNull))
        }
    }

    /// Returns the length of the string in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.header().len
    }

    /// Returns `true` if the string is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the string as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        unsafe {
            let bytes = core::slice::from_raw_parts(self.data_ptr(), self.len());
            core::str::from_utf8_unchecked(bytes)
        }
    }

    /// Returns the string as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.data_ptr(), self.len()) }
    }

    pub(crate) fn clone_impl(&self) -> Value {
        VString::new(self.as_str()).0
    }

    pub(crate) fn drop_impl(&mut self) {
        unsafe {
            Self::dealloc_ptr(self.0.heap_ptr().cast());
        }
    }
}

impl Deref for VString {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for VString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for VString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for VString {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for VString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for VString {}

impl PartialOrd for VString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for VString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Debug for VString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for VString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl Default for VString {
    fn default() -> Self {
        Self::empty()
    }
}

// === PartialEq with str ===

impl PartialEq<str> for VString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<VString> for str {
    fn eq(&self, other: &VString) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<&str> for VString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "alloc")]
impl PartialEq<String> for VString {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(feature = "alloc")]
impl PartialEq<VString> for String {
    fn eq(&self, other: &VString) -> bool {
        self.as_str() == other.as_str()
    }
}

// === From implementations ===

#[cfg(feature = "alloc")]
impl From<&str> for VString {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[cfg(feature = "alloc")]
impl From<String> for VString {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

#[cfg(feature = "alloc")]
impl From<&String> for VString {
    fn from(s: &String) -> Self {
        Self::new(s)
    }
}

#[cfg(feature = "alloc")]
impl From<VString> for String {
    fn from(s: VString) -> Self {
        s.as_str().into()
    }
}

// === Value conversions ===

impl AsRef<Value> for VString {
    fn as_ref(&self) -> &Value {
        &self.0
    }
}

impl AsMut<Value> for VString {
    fn as_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}

impl From<VString> for Value {
    fn from(s: VString) -> Self {
        s.0
    }
}

#[cfg(feature = "alloc")]
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        VString::new(s).0
    }
}

#[cfg(feature = "alloc")]
impl From<String> for Value {
    fn from(s: String) -> Self {
        VString::new(&s).0
    }
}

#[cfg(feature = "alloc")]
impl From<&String> for Value {
    fn from(s: &String) -> Self {
        VString::new(s).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = VString::new("hello");
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_empty() {
        let s = VString::empty();
        assert_eq!(s.as_str(), "");
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn test_equality() {
        let a = VString::new("hello");
        let b = VString::new("hello");
        let c = VString::new("world");

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, "hello");
        assert_eq!(a.as_str(), "hello");
    }

    #[test]
    fn test_clone() {
        let a = VString::new("test");
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_unicode() {
        let s = VString::new("hello 世界 🌍");
        assert_eq!(s.as_str(), "hello 世界 🌍");
    }

    #[test]
    fn test_deref() {
        let s = VString::new("hello");
        assert!(s.starts_with("hel"));
        assert!(s.ends_with("llo"));
    }

    #[test]
    fn test_ordering() {
        let a = VString::new("apple");
        let b = VString::new("banana");
        assert!(a < b);
    }
}
