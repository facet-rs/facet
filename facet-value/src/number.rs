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
    /// Signed 128-bit integer
    I128 = 3,
    /// Unsigned 128-bit integer
    U128 = 4,
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
    i128: i128,
    u128: u128,
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
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_f64(v: f64) -> Self {
        unsafe {
            let ptr = Self::alloc(NumberType::F64);
            (*ptr).data.f = v;
            VNumber(Value::new_ptr(ptr.cast(), TypeTag::Number))
        }
    }

    /// Creates a number from an i128, canonicalizing to the smallest representation.
    ///
    /// Magnitude canonicalization keeps the representations over disjoint ranges so
    /// that equal values always share a single internal form:
    /// `I64=[i64::MIN, i64::MAX]`, `U64=(i64::MAX, u64::MAX]`,
    /// `U128=(u64::MAX, u128::MAX]`, `I128=[i128::MIN, i64::MIN)`.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_i128(v: i128) -> Self {
        if let Ok(i) = i64::try_from(v) {
            Self::from_i64(i)
        } else if v >= 0 {
            if let Ok(u) = u64::try_from(v) {
                Self::from_u64(u)
            } else {
                // v > u64::MAX and positive: store as u128
                unsafe {
                    let ptr = Self::alloc(NumberType::U128);
                    (*ptr).data.u128 = v as u128;
                    VNumber(Value::new_ptr(ptr.cast(), TypeTag::Number))
                }
            }
        } else {
            // v < i64::MIN: store as i128
            unsafe {
                let ptr = Self::alloc(NumberType::I128);
                (*ptr).data.i128 = v;
                VNumber(Value::new_ptr(ptr.cast(), TypeTag::Number))
            }
        }
    }

    /// Creates a number from a u128, canonicalizing to the smallest representation.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn from_u128(v: u128) -> Self {
        if let Ok(u) = u64::try_from(v) {
            Self::from_u64(u)
        } else {
            unsafe {
                let ptr = Self::alloc(NumberType::U128);
                (*ptr).data.u128 = v;
                VNumber(Value::new_ptr(ptr.cast(), TypeTag::Number))
            }
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
                NumberType::I128 => i64::try_from(hd.data.i128).ok(),
                NumberType::U128 => i64::try_from(hd.data.u128).ok(),
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
                NumberType::I128 => u64::try_from(hd.data.i128).ok(),
                NumberType::U128 => u64::try_from(hd.data.u128).ok(),
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

    /// Converts to i128 if it can be represented exactly.
    #[must_use]
    pub fn to_i128(&self) -> Option<i128> {
        let hd = self.header();
        unsafe {
            match hd.type_ {
                NumberType::I64 => Some(i128::from(hd.data.i)),
                NumberType::U64 => Some(i128::from(hd.data.u)),
                NumberType::I128 => Some(hd.data.i128),
                NumberType::U128 => i128::try_from(hd.data.u128).ok(),
                NumberType::F64 => {
                    let f = hd.data.f;
                    // Check if in range and is a whole number via round-trip cast
                    if f >= i128::MIN as f64 && f <= i128::MAX as f64 {
                        let i = f as i128;
                        if i as f64 == f {
                            return Some(i);
                        }
                    }
                    None
                }
            }
        }
    }

    /// Converts to u128 if it can be represented exactly.
    #[must_use]
    pub fn to_u128(&self) -> Option<u128> {
        let hd = self.header();
        unsafe {
            match hd.type_ {
                NumberType::I64 => u128::try_from(hd.data.i).ok(),
                NumberType::U64 => Some(u128::from(hd.data.u)),
                NumberType::I128 => u128::try_from(hd.data.i128).ok(),
                NumberType::U128 => Some(hd.data.u128),
                NumberType::F64 => {
                    let f = hd.data.f;
                    // Check if in range and is a whole number via round-trip cast
                    if f >= 0.0 && f <= u128::MAX as f64 {
                        let u = f as u128;
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
                NumberType::I128 => {
                    let i = hd.data.i128;
                    let f = i as f64;
                    if f as i128 == i { Some(f) } else { None }
                }
                NumberType::U128 => {
                    let u = hd.data.u128;
                    let f = u as f64;
                    if f as u128 == u { Some(f) } else { None }
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
                NumberType::I128 => hd.data.i128 as f64,
                NumberType::U128 => hd.data.u128 as f64,
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
        matches!(
            self.header().type_,
            NumberType::I64 | NumberType::U64 | NumberType::I128 | NumberType::U128
        )
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
                NumberType::I128 => {
                    let ptr = Self::alloc(NumberType::I128);
                    (*ptr).data.i128 = hd.data.i128;
                    Value::new_ptr(ptr.cast(), TypeTag::Number)
                }
                NumberType::U128 => {
                    let ptr = Self::alloc(NumberType::U128);
                    (*ptr).data.u128 = hd.data.u128;
                    Value::new_ptr(ptr.cast(), TypeTag::Number)
                }
                NumberType::F64 => Self::from_f64(hd.data.f).0,
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
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

impl PartialOrd for VNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let h1 = self.header();
        let h2 = other.header();

        unsafe {
            // Fast path: same type
            if h1.type_ == h2.type_ {
                match h1.type_ {
                    NumberType::I64 => Some(h1.data.i.cmp(&h2.data.i)),
                    NumberType::U64 => Some(h1.data.u.cmp(&h2.data.u)),
                    NumberType::I128 => Some(h1.data.i128.cmp(&h2.data.i128)),
                    NumberType::U128 => Some(h1.data.u128.cmp(&h2.data.u128)),
                    NumberType::F64 => h1.data.f.partial_cmp(&h2.data.f),
                }
            } else if h1.type_ == NumberType::F64 || h2.type_ == NumberType::F64 {
                // If either operand is a float, fall back to lossy f64 comparison.
                // This preserves int == whole-float equality and NaN -> None.
                self.to_f64_lossy().partial_cmp(&other.to_f64_lossy())
            } else {
                // Both are integers: compare exactly.
                match (self.to_i128(), other.to_i128()) {
                    (Some(a), Some(b)) => Some(a.cmp(&b)),
                    // Exactly one is None means it's a U128 exceeding i128::MAX,
                    // so that side is the greater value.
                    (None, Some(_)) => Some(Ordering::Greater),
                    (Some(_), None) => Some(Ordering::Less),
                    // Both None: both exceed i128::MAX, compare via u128.
                    (None, None) => Some(self.to_u128().unwrap().cmp(&other.to_u128().unwrap())),
                }
            }
        }
    }
}

impl Hash for VNumber {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash based on the "canonical" representation. The chain mirrors the
        // canonicalization order so that equal values (including whole floats
        // equal to an integer) always land in the same bucket.
        if let Some(i) = self.to_i64() {
            0u8.hash(state); // discriminant for integer
            i.hash(state);
        } else if let Some(u) = self.to_u64() {
            1u8.hash(state); // discriminant for large unsigned
            u.hash(state);
        } else if let Some(i) = self.to_i128() {
            3u8.hash(state); // discriminant for 128-bit signed
            i.hash(state);
        } else if let Some(u) = self.to_u128() {
            4u8.hash(state); // discriminant for 128-bit unsigned
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
        } else if let Some(i) = self.to_i128() {
            Debug::fmt(&i, f)
        } else if let Some(u) = self.to_u128() {
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

// 128-bit integers must NOT go through the `as _` cast in `impl_from_int!`
// (that would be lossy), so they get explicit impls calling the dedicated
// canonicalizing constructors.

#[cfg(feature = "alloc")]
impl From<i128> for VNumber {
    fn from(v: i128) -> Self {
        Self::from_i128(v)
    }
}

#[cfg(feature = "alloc")]
impl From<i128> for Value {
    fn from(v: i128) -> Self {
        VNumber::from_i128(v).0
    }
}

#[cfg(feature = "alloc")]
impl From<u128> for VNumber {
    fn from(v: u128) -> Self {
        Self::from_u128(v)
    }
}

#[cfg(feature = "alloc")]
impl From<u128> for Value {
    fn from(v: u128) -> Self {
        VNumber::from_u128(v).0
    }
}

#[cfg(feature = "alloc")]
impl From<f32> for VNumber {
    fn from(v: f32) -> Self {
        Self::from_f64(f64::from(v))
    }
}

#[cfg(feature = "alloc")]
impl From<f64> for VNumber {
    fn from(v: f64) -> Self {
        Self::from_f64(v)
    }
}

#[cfg(feature = "alloc")]
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        VNumber::from_f64(f64::from(v)).into_value()
    }
}

#[cfg(feature = "alloc")]
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        VNumber::from_f64(v).into_value()
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
        let n = VNumber::from_f64(2.5);
        assert_eq!(n.to_f64(), Some(2.5));
        assert_eq!(n.to_i64(), None); // has fractional part
        assert!(n.is_float());
        assert!(!n.is_integer());
    }

    #[test]
    fn test_f64_whole() {
        let n = VNumber::from_f64(42.0);
        assert_eq!(n.to_f64(), Some(42.0));
        assert_eq!(n.to_i64(), Some(42)); // whole number
    }

    #[test]
    fn test_nan_roundtrip() {
        assert!(VNumber::from_f64(f64::NAN).to_f64().unwrap().is_nan());
        assert_eq!(
            VNumber::from_f64(f64::INFINITY).to_f64().unwrap(),
            f64::INFINITY
        );
        assert_eq!(
            VNumber::from_f64(f64::NEG_INFINITY).to_f64().unwrap(),
            f64::NEG_INFINITY
        );
    }

    #[test]
    fn test_equality() {
        let a = VNumber::from_i64(42);
        let b = VNumber::from_i64(42);
        let c = VNumber::from_f64(42.0);
        let nan = VNumber::from_f64(f64::NAN);

        assert_eq!(a, b);
        assert_eq!(a, c); // integer 42 equals float 42.0

        // nan should != any value including itself
        assert_ne!(c, nan);
        assert_ne!(nan, nan);
    }

    #[test]
    fn test_ordering() {
        let a = VNumber::from_i64(1);
        let b = VNumber::from_i64(2);
        let c = VNumber::from_f64(1.5);
        let nan = VNumber::from_f64(f64::NAN);
        let inf = VNumber::from_f64(f64::INFINITY);

        assert!(a < b);
        assert!(a < c);
        assert!(c < b);
        assert!(b < inf);
        assert!(!(c > nan || c < nan));
    }

    #[test]
    fn test_u128_max_roundtrip() {
        let n = VNumber::from_u128(u128::MAX);
        assert_eq!(n.to_u128(), Some(u128::MAX));
        // u128::MAX exceeds i128::MAX, so to_i128 must be None.
        assert_eq!(n.to_i128(), None);
        assert_eq!(n.to_u64(), None);
        assert!(n.is_integer());
    }

    #[test]
    fn test_i128_min_roundtrip() {
        let n = VNumber::from_i128(i128::MIN);
        assert_eq!(n.to_i128(), Some(i128::MIN));
        assert_eq!(n.to_u128(), None);
        assert_eq!(n.to_i64(), None);
        assert!(n.is_integer());
    }

    #[test]
    fn test_above_u64_max_roundtrip() {
        let v = u128::from(u64::MAX) + 1;
        let n = VNumber::from_u128(v);
        assert_eq!(n.to_u128(), Some(v));
        assert_eq!(n.to_i128(), Some(v as i128));
        assert_eq!(n.to_u64(), None);
    }

    #[test]
    fn test_128_canonicalization() {
        // Small values canonicalize down to I64.
        assert_eq!(VNumber::from_i128(5), VNumber::from_i64(5));
        assert_eq!(VNumber::from_u128(5), VNumber::from_i64(5));
        // u64::MAX canonicalizes to the same U64 representation.
        assert_eq!(
            VNumber::from_u128(u128::from(u64::MAX)),
            VNumber::from_u64(u64::MAX)
        );
        // A negative value within i64 range canonicalizes to I64.
        assert_eq!(VNumber::from_i128(-42), VNumber::from_i64(-42));
    }

    #[test]
    fn test_128_eq_hash_consistency() {
        use std::collections::HashSet;

        // Equal values built via different constructors must be Eq and hash equal.
        let a = VNumber::from_i128(5);
        let b = VNumber::from_u128(5);
        assert_eq!(a, b);

        let mut set = HashSet::new();
        set.insert(crate::Value::from(a.clone()));
        // Inserting the equal-but-differently-constructed value must collide.
        assert!(!set.insert(crate::Value::from(b.clone())));

        // A big value above u64::MAX, equal across i128/u128 construction.
        let big = u128::from(u64::MAX) + 12345;
        let x = VNumber::from_u128(big);
        let y = VNumber::from_i128(big as i128);
        assert_eq!(x, y);
        let mut set2 = HashSet::new();
        set2.insert(crate::Value::from(x));
        assert!(!set2.insert(crate::Value::from(y)));
    }

    #[test]
    fn test_128_cross_type_ordering() {
        // U128 above i128::MAX must order greater than any i128.
        let huge = VNumber::from_u128(u128::MAX);
        let big_signed = VNumber::from_i128(i128::MAX);
        assert!(big_signed < huge);

        // I128 min is the smallest.
        let small = VNumber::from_i128(i128::MIN);
        assert!(small < big_signed);
        assert!(small < VNumber::from_i64(0));
    }

    #[test]
    fn test_from_128_impls() {
        let v: crate::Value = (u128::MAX).into();
        assert_eq!(v.as_number().unwrap().to_u128(), Some(u128::MAX));
        let v: crate::Value = (i128::MIN).into();
        assert_eq!(v.as_number().unwrap().to_i128(), Some(i128::MIN));
    }
}
