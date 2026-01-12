//! String value type.

use crate::value::{TypeTag, Value};
#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc};
#[cfg(feature = "alloc")]
use alloc::string::String;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};
use core::mem;
use core::ops::Deref;
use core::ptr;

/// Flag indicating the string is marked as "safe" (e.g., pre-escaped HTML).
/// This uses the high bit of the length field in StringHeader.
const SAFE_FLAG: usize = 1usize << (usize::BITS - 1);

/// Header for heap-allocated strings.
#[repr(C, align(8))]
struct StringHeader {
    /// Length of the string in bytes.
    /// The high bit may be set to indicate a "safe" string (see SAFE_FLAG).
    len: usize,
    // String data follows immediately after
}

impl StringHeader {
    /// Returns the actual length of the string, masking out the safe flag.
    #[inline]
    const fn actual_len(&self) -> usize {
        self.len & !SAFE_FLAG
    }

    /// Returns true if the safe flag is set.
    #[inline]
    const fn is_safe(&self) -> bool {
        self.len & SAFE_FLAG != 0
    }
}

/// A string value.
///
/// `VString` stores UTF-8 string data. Short strings (up to 7 bytes on 64-bit targets) are
/// embedded directly in the `Value` pointer bits, while longer strings fall back to heap storage.
#[repr(transparent)]
#[derive(Clone)]
pub struct VString(pub(crate) Value);

impl VString {
    const INLINE_WORD_BYTES: usize = mem::size_of::<usize>();
    const INLINE_DATA_OFFSET: usize = 1;
    const INLINE_CAP_BYTES: usize = Self::INLINE_WORD_BYTES - Self::INLINE_DATA_OFFSET;
    pub(crate) const INLINE_LEN_MAX: usize = {
        const LEN_MASK: usize = (1 << (8 - 3)) - 1;
        let cap = mem::size_of::<usize>() - 1;
        if cap < LEN_MASK { cap } else { LEN_MASK }
    };
    const INLINE_LEN_SHIFT: u8 = 3;

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
            let len = (*ptr).actual_len();
            let layout = Self::layout(len);
            dealloc(ptr.cast::<u8>(), layout);
        }
    }

    fn header(&self) -> &StringHeader {
        debug_assert!(!self.is_inline());
        unsafe { &*(self.0.heap_ptr() as *const StringHeader) }
    }

    fn data_ptr(&self) -> *const u8 {
        debug_assert!(!self.is_inline());
        // Go through heap_ptr directly to avoid creating intermediate reference
        // that would limit provenance to just the header
        unsafe { (self.0.heap_ptr() as *const StringHeader).add(1).cast() }
    }

    /// Creates a new string from a `&str`.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new(s: &str) -> Self {
        if Self::can_inline(s.len()) {
            return Self::new_inline(s);
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
        Self::new_inline("")
    }

    /// Returns the length of the string in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        if self.is_inline() {
            self.inline_len()
        } else {
            self.header().actual_len()
        }
    }

    /// Returns `true` if the string is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the string as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Returns the string as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        if self.is_inline() {
            unsafe { core::slice::from_raw_parts(self.inline_data_ptr(), self.inline_len()) }
        } else {
            unsafe { core::slice::from_raw_parts(self.data_ptr(), self.len()) }
        }
    }

    pub(crate) fn clone_impl(&self) -> Value {
        if self.is_safe() {
            // Preserve the safe flag through clone
            VSafeString::new(self.as_str()).0
        } else {
            VString::new(self.as_str()).0
        }
    }

    pub(crate) fn drop_impl(&mut self) {
        if self.is_inline() {
            return;
        }
        unsafe {
            Self::dealloc_ptr(self.0.heap_ptr_mut().cast());
        }
    }

    #[inline]
    fn is_inline(&self) -> bool {
        self.0.is_inline_string()
    }

    #[inline]
    const fn can_inline(len: usize) -> bool {
        len <= Self::INLINE_LEN_MAX && len <= Self::INLINE_CAP_BYTES
    }

    #[inline]
    const fn inline_meta_ptr(&self) -> *const u8 {
        self as *const VString as *const u8
    }

    #[inline]
    const fn inline_data_ptr(&self) -> *const u8 {
        unsafe { self.inline_meta_ptr().add(Self::INLINE_DATA_OFFSET) }
    }

    #[inline]
    fn inline_len(&self) -> usize {
        debug_assert!(self.is_inline());
        unsafe { (*self.inline_meta_ptr() >> Self::INLINE_LEN_SHIFT) as usize }
    }

    #[cfg(feature = "alloc")]
    fn new_inline(s: &str) -> Self {
        debug_assert!(Self::can_inline(s.len()));
        let mut storage = [0u8; Self::INLINE_WORD_BYTES];
        storage[0] = ((s.len() as u8) << Self::INLINE_LEN_SHIFT) | (TypeTag::InlineString as u8);
        storage[Self::INLINE_DATA_OFFSET..Self::INLINE_DATA_OFFSET + s.len()]
            .copy_from_slice(s.as_bytes());
        let bits = usize::from_ne_bytes(storage);
        VString(unsafe { Value::from_bits(bits) })
    }

    /// Allocate a heap string with the safe flag set.
    #[cfg(feature = "alloc")]
    fn alloc_safe(s: &str) -> *mut StringHeader {
        unsafe {
            let layout = Self::layout(s.len());
            let ptr = alloc(layout).cast::<StringHeader>();
            (*ptr).len = s.len() | SAFE_FLAG;

            // Copy string data
            let data_ptr = ptr.add(1).cast::<u8>();
            ptr::copy_nonoverlapping(s.as_ptr(), data_ptr, s.len());

            ptr
        }
    }

    /// Returns `true` if this string is marked as safe (e.g., pre-escaped HTML).
    ///
    /// Inline strings are never safe - only heap-allocated strings can carry the safe flag.
    #[must_use]
    pub fn is_safe(&self) -> bool {
        if self.is_inline() {
            false
        } else {
            self.header().is_safe()
        }
    }

    /// Converts this string into a safe string.
    ///
    /// If the string is already safe, returns the same string wrapped as VSafeString.
    /// If the string is inline, promotes it to heap storage with the safe flag.
    /// If the string is on the heap but not safe, reallocates with the safe flag set.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn into_safe(self) -> VSafeString {
        if self.is_safe() {
            // Already safe, just wrap it
            return VSafeString(self.0);
        }
        // Need to allocate (or reallocate) with safe flag
        let s = self.as_str();
        unsafe {
            let ptr = Self::alloc_safe(s);
            VSafeString(Value::new_ptr(ptr.cast(), TypeTag::StringOrNull))
        }
    }
}

const _: () = {
    assert!(VString::INLINE_DATA_OFFSET == 1);
    assert!(VString::INLINE_CAP_BYTES <= VString::INLINE_WORD_BYTES - VString::INLINE_DATA_OFFSET);
    assert!(VString::INLINE_LEN_MAX <= VString::INLINE_CAP_BYTES);
};

/// A string value marked as "safe" (e.g., pre-escaped HTML that should not be escaped again).
///
/// `VSafeString` is semantically a string, but carries a flag indicating it has already been
/// processed (e.g., HTML-escaped) and should be output verbatim by template engines.
///
/// Unlike regular strings, safe strings are always heap-allocated since inline strings
/// don't have room for the safe flag.
///
/// # Example use case
///
/// ```ignore
/// // In a template engine:
/// {{ page.content }}           // If VSafeString, output as-is
/// {{ user_input }}             // Regular VString, escape HTML
/// {{ user_input | safe }}      // Convert to VSafeString via into_safe()
/// ```
#[repr(transparent)]
#[derive(Clone)]
pub struct VSafeString(pub(crate) Value);

impl VSafeString {
    /// Creates a new safe string from a `&str`.
    ///
    /// This always heap-allocates, even for short strings, since the safe flag
    /// is stored in the heap header.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn new(s: &str) -> Self {
        unsafe {
            let ptr = VString::alloc_safe(s);
            VSafeString(Value::new_ptr(ptr.cast(), TypeTag::StringOrNull))
        }
    }

    /// Returns the length of the string in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        // Safe strings are never inline, so we can go directly to the header
        self.header().actual_len()
    }

    /// Returns `true` if the string is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the string as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Returns the string as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.data_ptr(), self.len()) }
    }

    fn header(&self) -> &StringHeader {
        unsafe { &*(self.0.heap_ptr() as *const StringHeader) }
    }

    fn data_ptr(&self) -> *const u8 {
        unsafe { (self.0.heap_ptr() as *const StringHeader).add(1).cast() }
    }
}

impl Deref for VSafeString {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for VSafeString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for VSafeString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for VSafeString {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for VSafeString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for VSafeString {}

impl PartialOrd for VSafeString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VSafeString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for VSafeString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Debug for VSafeString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SafeString").field(&self.as_str()).finish()
    }
}

impl fmt::Display for VSafeString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

// === PartialEq with str ===

impl PartialEq<str> for VSafeString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<VSafeString> for str {
    fn eq(&self, other: &VSafeString) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<&str> for VSafeString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "alloc")]
impl PartialEq<String> for VSafeString {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(feature = "alloc")]
impl PartialEq<VString> for VSafeString {
    fn eq(&self, other: &VString) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(feature = "alloc")]
impl PartialEq<VSafeString> for VString {
    fn eq(&self, other: &VSafeString) -> bool {
        self.as_str() == other.as_str()
    }
}

// === From implementations ===

#[cfg(feature = "alloc")]
impl From<&str> for VSafeString {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[cfg(feature = "alloc")]
impl From<String> for VSafeString {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

#[cfg(feature = "alloc")]
impl From<&String> for VSafeString {
    fn from(s: &String) -> Self {
        Self::new(s)
    }
}

#[cfg(feature = "alloc")]
impl From<VSafeString> for String {
    fn from(s: VSafeString) -> Self {
        s.as_str().into()
    }
}

// A safe string IS a string, so we can convert
impl From<VSafeString> for VString {
    fn from(s: VSafeString) -> Self {
        VString(s.0)
    }
}

// === Value conversions ===

impl AsRef<Value> for VSafeString {
    fn as_ref(&self) -> &Value {
        &self.0
    }
}

impl AsMut<Value> for VSafeString {
    fn as_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}

impl From<VSafeString> for Value {
    fn from(s: VSafeString) -> Self {
        s.0
    }
}

impl VSafeString {
    /// Converts this VSafeString into a Value, consuming self.
    #[inline]
    pub fn into_value(self) -> Value {
        self.0
    }

    /// Converts this VSafeString into a VString, consuming self.
    /// The resulting VString will still have the safe flag set.
    #[inline]
    pub fn into_string(self) -> VString {
        VString(self.0)
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

impl VString {
    /// Converts this VString into a Value, consuming self.
    #[inline]
    pub fn into_value(self) -> Value {
        self.0
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
    use crate::value::{TypeTag, Value};

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

    #[test]
    fn test_inline_representation() {
        let s = VString::new("inline");
        assert!(s.is_inline(), "expected inline storage");
        assert_eq!(s.as_str(), "inline");
    }

    #[test]
    fn test_heap_representation() {
        let long_input = "a".repeat(VString::INLINE_LEN_MAX + 1);
        let s = VString::new(&long_input);
        assert!(!s.is_inline(), "expected heap storage");
        assert_eq!(s.as_str(), long_input);
    }

    #[test]
    fn inline_capacity_boundaries() {
        for len in 0..=VString::INLINE_LEN_MAX {
            let input = "x".repeat(len);
            let s = VString::new(&input);
            assert!(
                s.is_inline(),
                "expected inline storage for length {} (capacity {})",
                len,
                VString::INLINE_LEN_MAX
            );
            assert_eq!(s.len(), len);
            assert_eq!(s.as_str(), input);
            assert_eq!(s.as_bytes(), input.as_bytes());
        }

        let overflow = "y".repeat(VString::INLINE_LEN_MAX + 1);
        let heap = VString::new(&overflow);
        assert!(
            !heap.is_inline(),
            "length {} should force heap allocation",
            overflow.len()
        );
    }

    #[test]
    fn inline_value_tag_matches() {
        for len in 0..=VString::INLINE_LEN_MAX {
            let input = "z".repeat(len);
            let value = Value::from(input.as_str());
            assert!(value.is_inline_string(), "Value should mark inline string");
            assert_eq!(
                value.ptr_usize() & 0b111,
                TypeTag::InlineString as usize,
                "low bits must store inline string tag"
            );
            let roundtrip = value.as_string().expect("string value");
            assert_eq!(roundtrip.as_str(), input);
            assert_eq!(roundtrip.as_bytes(), input.as_bytes());
        }
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn inline_len_max_is_seven_on_64_bit() {
        assert_eq!(VString::INLINE_LEN_MAX, 7);
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn inline_len_max_is_three_on_32_bit() {
        assert_eq!(VString::INLINE_LEN_MAX, 3);
    }

    // === VSafeString tests ===

    #[test]
    fn test_safe_string_new() {
        let s = VSafeString::new("hello");
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_safe_string_roundtrip() {
        let original = "<b>bold</b>";
        let safe = VSafeString::new(original);
        assert_eq!(safe.as_str(), original);
    }

    #[test]
    fn test_safe_string_is_always_heap() {
        // Even short strings should be heap-allocated for safe strings
        let short = VSafeString::new("hi");
        assert_eq!(short.len(), 2);
        assert_eq!(short.as_str(), "hi");
        // The value should have tag 1 (StringOrNull) not tag 6 (InlineString)
        let value: Value = short.into();
        assert!(!value.is_inline_string());
        assert!(value.is_string());
    }

    #[test]
    fn test_vstring_is_safe() {
        let normal = VString::new("hello");
        assert!(!normal.is_safe());

        let safe = VSafeString::new("hello");
        // When viewed as VString, should still report safe
        let as_vstring: VString = safe.into();
        assert!(as_vstring.is_safe());
    }

    #[test]
    fn test_vstring_into_safe() {
        // Test inline string promotion
        let inline = VString::new("hi");
        assert!(inline.is_inline());
        let safe = inline.into_safe();
        assert_eq!(safe.as_str(), "hi");

        // Test heap string conversion
        let long = "a".repeat(VString::INLINE_LEN_MAX + 10);
        let heap = VString::new(&long);
        assert!(!heap.is_inline());
        let safe_heap = heap.into_safe();
        assert_eq!(safe_heap.as_str(), long);
    }

    #[test]
    fn test_safe_flag_preserved_through_clone() {
        let safe = VSafeString::new("<b>bold</b>");
        let value: Value = safe.into();
        assert!(value.is_safe_string());

        let cloned = value.clone();
        assert!(cloned.is_safe_string());
        assert_eq!(cloned.as_string().unwrap().as_str(), "<b>bold</b>");
    }

    #[test]
    fn test_value_as_safe_string() {
        let safe = VSafeString::new("safe content");
        let value: Value = safe.into();

        // is_string should return true (safe strings ARE strings)
        assert!(value.is_string());
        // is_safe_string should also return true
        assert!(value.is_safe_string());
        // as_string should work
        assert_eq!(value.as_string().unwrap().as_str(), "safe content");
        // as_safe_string should work
        assert_eq!(value.as_safe_string().unwrap().as_str(), "safe content");
    }

    #[test]
    fn test_normal_string_not_safe() {
        let normal = VString::new("normal");
        let value: Value = normal.into();

        assert!(value.is_string());
        assert!(!value.is_safe_string());
        assert!(value.as_string().is_some());
        assert!(value.as_safe_string().is_none());
    }

    #[test]
    fn test_safe_string_equality() {
        let a = VSafeString::new("hello");
        let b = VSafeString::new("hello");
        let c = VSafeString::new("world");

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, "hello");

        // Equality with VString
        let vstring = VString::new("hello");
        assert_eq!(a, vstring);
        assert_eq!(vstring, a);
    }

    #[test]
    fn test_safe_string_into_string() {
        let safe = VSafeString::new("test");
        let vstring = safe.into_string();
        assert_eq!(vstring.as_str(), "test");
        assert!(vstring.is_safe()); // Flag should be preserved
    }

    #[test]
    fn test_safe_flag_constant() {
        // Verify the safe flag uses the high bit
        assert_eq!(SAFE_FLAG, 1usize << (usize::BITS - 1));
        // On 64-bit: 0x8000_0000_0000_0000
        // On 32-bit: 0x8000_0000
    }

    #[test]
    fn test_safe_string_long() {
        // Test with a string that would definitely be heap-allocated anyway
        let long = "a".repeat(1000);
        let safe = VSafeString::new(&long);
        assert_eq!(safe.len(), 1000);
        assert_eq!(safe.as_str(), long);

        let value: Value = safe.into();
        assert!(value.is_safe_string());
        assert_eq!(value.as_string().unwrap().len(), 1000);
    }
}

#[cfg(all(test, feature = "bolero-inline-tests"))]
mod bolero_props {
    use super::*;
    use crate::ValueType;
    use crate::array::VArray;
    use alloc::string::String;
    use alloc::vec::Vec;
    use bolero::check;

    #[test]
    fn bolero_inline_string_round_trip() {
        check!().with_type::<Vec<u8>>().for_each(|bytes: &Vec<u8>| {
            if bytes.len() > VString::INLINE_LEN_MAX + 8 {
                // Keep the generator focused on short payloads to hit inline cases hard.
                return;
            }

            if let Ok(s) = String::from_utf8(bytes.clone()) {
                let value = Value::from(s.as_str());
                let roundtrip = value.as_string().expect("expected string value");
                assert_eq!(roundtrip.as_str(), s);

                if VString::can_inline(s.len()) {
                    assert!(value.is_inline_string(), "expected inline tag for {s:?}");
                } else {
                    assert!(!value.is_inline_string(), "unexpected inline tag for {s:?}");
                }
            }
        });
    }

    #[test]
    fn bolero_string_mutation_sequences() {
        check!().with_type::<Vec<u8>>().for_each(|bytes: &Vec<u8>| {
            let mut value = Value::from("");
            let mut expected = String::new();

            for chunk in bytes.chunks(3).take(24) {
                let selector = chunk.first().copied().unwrap_or(0) % 3;
                match selector {
                    0 => {
                        let ch = (b'a' + chunk.get(1).copied().unwrap_or(0) % 26) as char;
                        expected.push(ch);
                    }
                    1 => {
                        if !expected.is_empty() {
                            let len = chunk
                                .get(1)
                                .copied()
                                .map(|n| (n as usize) % expected.len())
                                .unwrap_or(0);
                            expected.truncate(len);
                        }
                    }
                    _ => expected.clear(),
                }

                overwrite_value_string(&mut value, &expected);
                assert_eq!(value.as_string().unwrap().as_str(), expected);
                assert_eq!(
                    value.is_inline_string(),
                    expected.len() <= VString::INLINE_LEN_MAX,
                    "mutation sequence should keep inline status accurate"
                );
            }
        });
    }

    #[test]
    fn bolero_array_model_matches() {
        check!().with_type::<Vec<u8>>().for_each(|bytes: &Vec<u8>| {
            let mut arr = VArray::new();
            let mut model: Vec<String> = Vec::new();

            for chunk in bytes.chunks(4).take(20) {
                match chunk.first().copied().unwrap_or(0) % 4 {
                    0 => {
                        let content = inline_string_from_chunk(chunk, 1);
                        arr.push(Value::from(content.as_str()));
                        model.push(content);
                    }
                    1 => {
                        let idx = chunk.get(1).copied().unwrap_or(0) as usize;
                        if !model.is_empty() {
                            let idx = idx % model.len();
                            model.remove(idx);
                            let _ = arr.remove(idx);
                        }
                    }
                    2 => {
                        let content = inline_string_from_chunk(chunk, 2);
                        if model.is_empty() {
                            arr.insert(0, Value::from(content.as_str()));
                            model.insert(0, content);
                        } else {
                            let len = model.len();
                            let idx = (chunk.get(2).copied().unwrap_or(0) as usize) % (len + 1);
                            arr.insert(idx, Value::from(content.as_str()));
                            model.insert(idx, content);
                        }
                    }
                    _ => {
                        arr.clear();
                        model.clear();
                    }
                }

                assert_eq!(arr.len(), model.len());
                for (value, expected) in arr.iter().zip(model.iter()) {
                    assert_eq!(value.value_type(), ValueType::String);
                    assert_eq!(value.as_string().unwrap().as_str(), expected);
                    assert_eq!(
                        value.is_inline_string(),
                        expected.len() <= VString::INLINE_LEN_MAX
                    );
                }
            }
        });
    }

    fn overwrite_value_string(value: &mut Value, new_value: &str) {
        let slot = value.as_string_mut().expect("expected string value");
        *slot = VString::new(new_value);
    }

    fn inline_string_from_chunk(chunk: &[u8], seed_idx: usize) -> String {
        let len_hint = chunk.get(seed_idx).copied().unwrap_or(0) as usize;
        let len = len_hint % (VString::INLINE_LEN_MAX.saturating_sub(1).max(1));
        (0..len)
            .map(|i| {
                let byte = chunk.get(i % chunk.len()).copied().unwrap_or(b'a');
                (b'a' + (byte % 26)) as char
            })
            .collect()
    }
}
