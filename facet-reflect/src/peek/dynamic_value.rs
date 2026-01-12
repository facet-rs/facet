//! Support for peeking into DynamicValue types like `facet_value::Value`

use facet_core::{DynDateTimeKind, DynValueKind, DynamicValueDef};

use super::Peek;

/// Lets you read from a dynamic value (implements read-only operations for DynamicValue types)
///
/// This is used for types like `facet_value::Value` that can hold any of:
/// null, bool, number, string, bytes, array, or object - determined at runtime.
#[derive(Clone, Copy)]
pub struct PeekDynamicValue<'mem, 'facet> {
    /// the underlying peek value
    pub(crate) value: Peek<'mem, 'facet>,

    /// the definition of the dynamic value
    pub(crate) def: DynamicValueDef,
}

impl<'mem, 'facet> PeekDynamicValue<'mem, 'facet> {
    /// Returns the dynamic value definition
    #[inline(always)]
    pub const fn def(&self) -> DynamicValueDef {
        self.def
    }

    /// Returns the underlying peek value
    #[inline(always)]
    pub const fn peek(&self) -> Peek<'mem, 'facet> {
        self.value
    }

    /// Returns the kind of value stored
    #[inline]
    pub fn kind(&self) -> DynValueKind {
        unsafe { (self.def.vtable.get_kind)(self.value.data()) }
    }

    /// Returns true if the value is null
    #[inline]
    pub fn is_null(&self) -> bool {
        self.kind() == DynValueKind::Null
    }

    /// Returns the boolean value if this is a bool, None otherwise
    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        unsafe { (self.def.vtable.get_bool)(self.value.data()) }
    }

    /// Returns the i64 value if representable, None otherwise
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        unsafe { (self.def.vtable.get_i64)(self.value.data()) }
    }

    /// Returns the u64 value if representable, None otherwise
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        unsafe { (self.def.vtable.get_u64)(self.value.data()) }
    }

    /// Returns the f64 value if this is a number, None otherwise
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        unsafe { (self.def.vtable.get_f64)(self.value.data()) }
    }

    /// Returns the string value if this is a string, None otherwise
    #[inline]
    pub fn as_str(&self) -> Option<&'mem str> {
        unsafe { (self.def.vtable.get_str)(self.value.data()) }
    }

    /// Returns the bytes value if this is bytes, None otherwise
    #[inline]
    pub fn as_bytes(&self) -> Option<&'mem [u8]> {
        self.def
            .vtable
            .get_bytes
            .and_then(|f| unsafe { f(self.value.data()) })
    }

    /// Returns the datetime components if this is a datetime, None otherwise
    ///
    /// Returns `(year, month, day, hour, minute, second, nanos, kind)`.
    #[inline]
    #[allow(clippy::type_complexity)]
    pub fn as_datetime(&self) -> Option<(i32, u8, u8, u8, u8, u8, u32, DynDateTimeKind)> {
        self.def
            .vtable
            .get_datetime
            .and_then(|f| unsafe { f(self.value.data()) })
    }

    /// Returns the length of the array if this is an array, None otherwise
    #[inline]
    pub fn array_len(&self) -> Option<usize> {
        unsafe { (self.def.vtable.array_len)(self.value.data()) }
    }

    /// Returns an element from the array by index, None if not an array or index out of bounds
    #[inline]
    pub fn array_get(&self, index: usize) -> Option<Peek<'mem, 'facet>> {
        let ptr = unsafe { (self.def.vtable.array_get)(self.value.data(), index)? };
        // The element is also a DynamicValue with the same shape
        Some(unsafe { Peek::unchecked_new(ptr, self.value.shape()) })
    }

    /// Returns the length of the object if this is an object, None otherwise
    #[inline]
    pub fn object_len(&self) -> Option<usize> {
        unsafe { (self.def.vtable.object_len)(self.value.data()) }
    }

    /// Returns a key-value pair from the object by index, None if not an object or index out of bounds
    #[inline]
    pub fn object_get_entry(&self, index: usize) -> Option<(&'mem str, Peek<'mem, 'facet>)> {
        let (key, value_ptr) =
            unsafe { (self.def.vtable.object_get_entry)(self.value.data(), index)? };
        // The value is also a DynamicValue with the same shape
        Some((key, unsafe {
            Peek::unchecked_new(value_ptr, self.value.shape())
        }))
    }

    /// Returns a value from the object by key, None if not an object or key not found
    #[inline]
    pub fn object_get(&self, key: &str) -> Option<Peek<'mem, 'facet>> {
        let ptr = unsafe { (self.def.vtable.object_get)(self.value.data(), key)? };
        // The value is also a DynamicValue with the same shape
        Some(unsafe { Peek::unchecked_new(ptr, self.value.shape()) })
    }

    /// Returns an iterator over array elements if this is an array
    #[inline]
    pub fn array_iter(&self) -> Option<PeekDynamicValueArrayIter<'mem, 'facet>> {
        let len = self.array_len()?;
        Some(PeekDynamicValueArrayIter {
            dyn_value: *self,
            index: 0,
            len,
        })
    }

    /// Returns an iterator over object entries if this is an object
    #[inline]
    pub fn object_iter(&self) -> Option<PeekDynamicValueObjectIter<'mem, 'facet>> {
        let len = self.object_len()?;
        Some(PeekDynamicValueObjectIter {
            dyn_value: *self,
            index: 0,
            len,
        })
    }

    /// Structurally hash the dynamic value's contents.
    ///
    /// This is called by `Peek::structural_hash` for dynamic values.
    pub fn structural_hash_inner<H: core::hash::Hasher>(&self, hasher: &mut H) {
        use core::hash::Hash;

        // Hash the kind discriminant
        let kind = self.kind();
        core::mem::discriminant(&kind).hash(hasher);

        match kind {
            DynValueKind::Null => {
                // Nothing more to hash
            }
            DynValueKind::Bool => {
                if let Some(b) = self.as_bool() {
                    b.hash(hasher);
                }
            }
            DynValueKind::Number => {
                // Try to get as various number types and hash
                if let Some(n) = self.as_i64() {
                    0u8.hash(hasher); // discriminant for i64
                    n.hash(hasher);
                } else if let Some(n) = self.as_u64() {
                    1u8.hash(hasher); // discriminant for u64
                    n.hash(hasher);
                } else if let Some(n) = self.as_f64() {
                    2u8.hash(hasher); // discriminant for f64
                    n.to_bits().hash(hasher);
                }
            }
            DynValueKind::String => {
                if let Some(s) = self.as_str() {
                    s.hash(hasher);
                }
            }
            DynValueKind::Bytes => {
                if let Some(b) = self.as_bytes() {
                    b.hash(hasher);
                }
            }
            DynValueKind::Array => {
                if let Some(len) = self.array_len() {
                    len.hash(hasher);
                    if let Some(iter) = self.array_iter() {
                        for elem in iter {
                            elem.structural_hash(hasher);
                        }
                    }
                }
            }
            DynValueKind::Object => {
                if let Some(len) = self.object_len() {
                    len.hash(hasher);
                    if let Some(iter) = self.object_iter() {
                        for (key, value) in iter {
                            key.hash(hasher);
                            value.structural_hash(hasher);
                        }
                    }
                }
            }
            DynValueKind::DateTime | DynValueKind::QName | DynValueKind::Uuid => {
                // Hash the string representation
                if let Some(s) = self.as_str() {
                    s.hash(hasher);
                }
            }
        }
    }
}

/// Iterator over array elements in a dynamic value
pub struct PeekDynamicValueArrayIter<'mem, 'facet> {
    dyn_value: PeekDynamicValue<'mem, 'facet>,
    index: usize,
    len: usize,
}

impl<'mem, 'facet> Iterator for PeekDynamicValueArrayIter<'mem, 'facet> {
    type Item = Peek<'mem, 'facet>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }
        let item = self.dyn_value.array_get(self.index)?;
        self.index += 1;
        Some(item)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PeekDynamicValueArrayIter<'_, '_> {}

/// Iterator over object entries in a dynamic value
pub struct PeekDynamicValueObjectIter<'mem, 'facet> {
    dyn_value: PeekDynamicValue<'mem, 'facet>,
    index: usize,
    len: usize,
}

impl<'mem, 'facet> Iterator for PeekDynamicValueObjectIter<'mem, 'facet> {
    type Item = (&'mem str, Peek<'mem, 'facet>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }
        let entry = self.dyn_value.object_get_entry(self.index)?;
        self.index += 1;
        Some(entry)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PeekDynamicValueObjectIter<'_, '_> {}

impl core::fmt::Debug for PeekDynamicValue<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekDynamicValue")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}
