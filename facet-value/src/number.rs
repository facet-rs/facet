//! Number value type with efficient storage for various numeric types.

#[cfg(feature = "alloc")]
use alloc::alloc::{Layout, alloc, dealloc};
use core::cmp::Ordering;
use core::fmt::{self, Debug, Formatter};
use core::hash::{Hash, Hasher};

use crate::value::{TypeTag, Value};

/// Internal representation of number type.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum NumberType {
    /// Signed 64-bit integer
    I64 = 0,
    /// Unsigned 64-bit integer
    U64 = 1,
    /// 64-bit floating point
    F64 = 2,
}

/// Header for heap-allocated numbers.
#[repr(C, align(8))]
struct NumberHeader {
    /// Type discriminant
    type_: NumberType,
    /// Padding
    _pad: [u8; 7],
    /// The actual number data (i64, u64, or f64)
    data: NumberData,
}

#[repr(C)]
union NumberData {
    i: i64,
    u: u64,
    f: f64,
}

/// A JSON number value.
///
/// `VNumber` can represent integers (signed and unsigned) and floating point numbers.
/// It stores the number in the most appropriate internal format.
#[repr(transparent)]
#[derive(Clone)]
pub struct VNumber(pub(crate) Value);

impl VNumber {
    const fn layout() -> Layout {
        Layout::new::<NumberHeader>()
    }

    #[cfg(feature = "alloc")]
    fn alloc(type_: NumberType) -> *mut NumberHeader {
        unsafe {
            let ptr = alloc(Self::layout()).cast::<NumberHeader>();
            (*ptr).type_ = type_;
            ptr
        }
    }

    #[cfg(feature = "alloc")]
    fn dealloc(ptr: *mut NumberHeader) {
        unsafe {
            dealloc(ptr.cast::<u8>(), Self::layout());
        }
    }

    fn header(&self) -> &NumberHeader {
        unsafe { &*(self.0.heap_ptr() as *const NumberHeader) }
    }

    #[allow(dead_code)]
    fn header_mut(&mut self) -> &mut NumberHeader {
        unsafe { &mut *(self.0.heap_ptr_mut() as *mut NumberHeader) }
    }

    /// Creates a number from an i64.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_i64(v: i64) -> Self {
        unsafe {
            let ptr = Self::alloc(NumberType::I64);
            (*ptr).data.i = v;
            VNumber(Value::new_ptr(ptr.cast(), TypeTag::Number))
        }
    }

    /// Creates a number from a u64.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_u64(v: u64) -> Self {
        // If it fits in i64, use that for consistency
        if let Ok(i) = i64::try_from(v) {
            Self::from_i64(i)
        } else {
            unsafe {
                let ptr = Self::alloc(NumberType::U64);
                (*ptr).data.u = v;
                VNumber(Value::new_ptr(ptr.cast(), TypeTag::Number))
            }
        }
    }

    /// Creates a number from an f64.
    ///
    /// Returns `None` if the value is NaN or infinite.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_f64(v: f64) -> Option<Self> {
        if !v.is_finite() {
            return None;
        }
        unsafe {
            let ptr = Self::alloc(NumberType::F64);
            (*ptr).data.f = v;
            Some(VNumber(Value::new_ptr(ptr.cast(), TypeTag::Number)))
        }
    }

    /// Returns the number zero.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn zero() -> Self {
        Self::from_i64(0)
    }

    /// Returns the number one.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn one() -> Self {
        Self::from_i64(1)
    }

    /// Converts to i64 if it can be represented exactly.
    #[must_use]
    pub fn to_i64(&self) -> Option<i64> {
        let hd = self.header();
        unsafe {
            match hd.type_ {
                NumberType::I64 => Some(hd.data.i),
                NumberType::U64 => i64::try_from(hd.data.u).ok(),
                NumberType::F64 => {
                    let f = hd.data.f;
                    // Check if in range and is a whole number via round-trip cast
                    if f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                        let i = f as i64;
                        if i as f64 == f {
                            return Some(i);
                        }
                    }
                    None
                }
            }
        }
    }

    /// Converts to u64 if it can be represented exactly.
    #[must_use]
    pub fn to_u64(&self) -> Option<u64> {
        let hd = self.header();
        unsafe {
            match hd.type_ {
                NumberType::I64 => u64::try_from(hd.data.i).ok(),
                NumberType::U64 => Some(hd.data.u),
                NumberType::F64 => {
                    let f = hd.data.f;
                    // Check if in range and is a whole number via round-trip cast
                    if f >= 0.0 && f <= u64::MAX as f64 {
                        let u = f as u64;
                        if u as f64 == f {
                            return Some(u);
                        }
                    }
                    None
                }
            }
        }
    }

    /// Converts to f64 if it can be represented exactly.
    #[must_use]
    pub fn to_f64(&self) -> Option<f64> {
        let hd = self.header();
        unsafe {
            match hd.type_ {
                NumberType::I64 => {
                    let i = hd.data.i;
                    let f = i as f64;
                    if f as i64 == i { Some(f) } else { None }
                }
                NumberType::U64 => {
                    let u = hd.data.u;
                    let f = u as f64;
                    if f as u64 == u { Some(f) } else { None }
                }
                NumberType::F64 => Some(hd.data.f),
            }
        }
    }

    /// Converts to f64, potentially losing precision.
    #[must_use]
    pub fn to_f64_lossy(&self) -> f64 {
        let hd = self.header();
        unsafe {
            match hd.type_ {
                NumberType::I64 => hd.data.i as f64,
                NumberType::U64 => hd.data.u as f64,
                NumberType::F64 => hd.data.f,
            }
        }
    }

    /// Converts to i32 if it can be represented exactly.
    #[must_use]
    pub fn to_i32(&self) -> Option<i32> {
        self.to_i64().and_then(|v| i32::try_from(v).ok())
    }

    /// Converts to u32 if it can be represented exactly.
    #[must_use]
    pub fn to_u32(&self) -> Option<u32> {
        self.to_u64().and_then(|v| u32::try_from(v).ok())
    }

    /// Converts to f32 if it can be represented exactly.
    #[must_use]
    pub fn to_f32(&self) -> Option<f32> {
        self.to_f64().and_then(|f| {
            let f32_val = f as f32;
            if f32_val as f64 == f {
                Some(f32_val)
            } else {
                None
            }
        })
    }

    /// Returns true if this number was created from a floating point value.
    #[must_use]
    pub fn is_float(&self) -> bool {
        self.header().type_ == NumberType::F64
    }

    /// Returns true if this number is an integer (signed or unsigned).
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(self.header().type_, NumberType::I64 | NumberType::U64)
    }

    pub(crate) fn clone_impl(&self) -> Value {
        let hd = self.header();
        unsafe {
            match hd.type_ {
                NumberType::I64 => Self::from_i64(hd.data.i).0,
                NumberType::U64 => {
                    let ptr = Self::alloc(NumberType::U64);
                    (*ptr).data.u = hd.data.u;
                    Value::new_ptr(ptr.cast(), TypeTag::Number)
                }
                NumberType::F64 => Self::from_f64(hd.data.f).unwrap().0,
            }
        }
    }

    pub(crate) fn drop_impl(&mut self) {
        unsafe {
            Self::dealloc(self.0.heap_ptr_mut().cast());
        }
    }
}

impl PartialEq for VNumber {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for VNumber {}

impl PartialOrd for VNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        let h1 = self.header();
        let h2 = other.header();

        unsafe {
            // Fast path: same type
            if h1.type_ == h2.type_ {
                match h1.type_ {
                    NumberType::I64 => h1.data.i.cmp(&h2.data.i),
                    NumberType::U64 => h1.data.u.cmp(&h2.data.u),
                    NumberType::F64 => h1.data.f.partial_cmp(&h2.data.f).unwrap_or(Ordering::Equal),
                }
            } else {
                // Cross-type comparison: convert to f64 for simplicity
                // (This loses precision for very large integers, but is simple)
                self.to_f64_lossy()
                    .partial_cmp(&other.to_f64_lossy())
                    .unwrap_or(Ordering::Equal)
            }
        }
    }
}

impl Hash for VNumber {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash based on the "canonical" representation
        if let Some(i) = self.to_i64() {
            0u8.hash(state); // discriminant for integer
            i.hash(state);
        } else if let Some(u) = self.to_u64() {
            1u8.hash(state); // discriminant for large unsigned
            u.hash(state);
        } else if let Some(f) = self.to_f64() {
            2u8.hash(state); // discriminant for float
            f.to_bits().hash(state);
        }
    }
}

impl Debug for VNumber {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(i) = self.to_i64() {
            Debug::fmt(&i, f)
        } else if let Some(u) = self.to_u64() {
            Debug::fmt(&u, f)
        } else if let Some(fl) = self.to_f64() {
            Debug::fmt(&fl, f)
        } else {
            f.write_str("NaN")
        }
    }
}

impl Default for VNumber {
    fn default() -> Self {
        Self::zero()
    }
}

// === From implementations ===

macro_rules! impl_from_int {
    ($($t:ty => $method:ident),* $(,)?) => {
        $(
            #[cfg(feature = "alloc")]
            impl From<$t> for VNumber {
                fn from(v: $t) -> Self {
                    Self::$method(v as _)
                }
            }

            #[cfg(feature = "alloc")]
            impl From<$t> for Value {
                fn from(v: $t) -> Self {
                    VNumber::from(v).0
                }
            }
        )*
    };
}

impl_from_int! {
    i8 => from_i64,
    i16 => from_i64,
    i32 => from_i64,
    i64 => from_i64,
    isize => from_i64,
    u8 => from_i64,
    u16 => from_i64,
    u32 => from_i64,
    u64 => from_u64,
    usize => from_u64,
}

#[cfg(feature = "alloc")]
impl TryFrom<f32> for VNumber {
    type Error = ();

    fn try_from(v: f32) -> Result<Self, Self::Error> {
        Self::from_f64(f64::from(v)).ok_or(())
    }
}

#[cfg(feature = "alloc")]
impl TryFrom<f64> for VNumber {
    type Error = ();

    fn try_from(v: f64) -> Result<Self, Self::Error> {
        Self::from_f64(v).ok_or(())
    }
}

#[cfg(feature = "alloc")]
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        VNumber::from_f64(f64::from(v))
            .map(|n| n.0)
            .unwrap_or(Value::NULL)
    }
}

#[cfg(feature = "alloc")]
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        VNumber::from_f64(v).map(|n| n.0).unwrap_or(Value::NULL)
    }
}

// === Conversion traits ===

impl AsRef<Value> for VNumber {
    fn as_ref(&self) -> &Value {
        &self.0
    }
}

impl AsMut<Value> for VNumber {
    fn as_mut(&mut self) -> &mut Value {
        &mut self.0
    }
}

impl From<VNumber> for Value {
    fn from(n: VNumber) -> Self {
        n.0
    }
}

impl VNumber {
    /// Converts this VNumber into a Value, consuming self.
    #[inline]
    pub fn into_value(self) -> Value {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i64() {
        let n = VNumber::from_i64(42);
        assert_eq!(n.to_i64(), Some(42));
        assert_eq!(n.to_u64(), Some(42));
        assert_eq!(n.to_f64(), Some(42.0));
        assert!(n.is_integer());
        assert!(!n.is_float());
    }

    #[test]
    fn test_negative() {
        let n = VNumber::from_i64(-100);
        assert_eq!(n.to_i64(), Some(-100));
        assert_eq!(n.to_u64(), None);
        assert_eq!(n.to_f64(), Some(-100.0));
    }

    #[test]
    fn test_large_u64() {
        let v = u64::MAX;
        let n = VNumber::from_u64(v);
        assert_eq!(n.to_u64(), Some(v));
        assert_eq!(n.to_i64(), None);
    }

    #[test]
    fn test_f64() {
        let n = VNumber::from_f64(2.5).unwrap();
        assert_eq!(n.to_f64(), Some(2.5));
        assert_eq!(n.to_i64(), None); // has fractional part
        assert!(n.is_float());
        assert!(!n.is_integer());
    }

    #[test]
    fn test_f64_whole() {
        let n = VNumber::from_f64(42.0).unwrap();
        assert_eq!(n.to_f64(), Some(42.0));
        assert_eq!(n.to_i64(), Some(42)); // whole number
    }

    #[test]
    fn test_nan_rejected() {
        assert!(VNumber::from_f64(f64::NAN).is_none());
        assert!(VNumber::from_f64(f64::INFINITY).is_none());
        assert!(VNumber::from_f64(f64::NEG_INFINITY).is_none());
    }

    #[test]
    fn test_equality() {
        let a = VNumber::from_i64(42);
        let b = VNumber::from_i64(42);
        let c = VNumber::from_f64(42.0).unwrap();

        assert_eq!(a, b);
        assert_eq!(a, c); // integer 42 equals float 42.0
    }

    #[test]
    fn test_ordering() {
        let a = VNumber::from_i64(1);
        let b = VNumber::from_i64(2);
        let c = VNumber::from_f64(1.5).unwrap();

        assert!(a < b);
        assert!(a < c);
        assert!(c < b);
    }
}
