//! Support for poking (mutating) DynamicValue types like `facet_value::Value`

use core::mem::ManuallyDrop;

use facet_core::{DynValueKind, DynamicValueDef, Facet, PtrMut, PtrUninit};

use crate::{HeapValue, ReflectError, ReflectErrorKind};

use super::Poke;

/// Lets you mutate a dynamic value (implements mutable operations for DynamicValue types).
///
/// This is used for types like `facet_value::Value` that can hold any of:
/// null, bool, number, string, bytes, array, or object - determined at runtime.
///
/// The setter methods (`set_null`, `set_bool`, etc.) drop the previous value and
/// re-initialize the storage with the new kind.
pub struct PokeDynamicValue<'mem, 'facet> {
    pub(crate) value: Poke<'mem, 'facet>,
    pub(crate) def: DynamicValueDef,
}

impl<'mem, 'facet> core::fmt::Debug for PokeDynamicValue<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeDynamicValue")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeDynamicValue<'mem, 'facet> {
    /// Creates a new poke dynamic value.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that
    /// correctly implement the dynamic-value operations for the actual type.
    #[inline]
    pub const unsafe fn new(value: Poke<'mem, 'facet>, def: DynamicValueDef) -> Self {
        Self { value, def }
    }

    /// Returns the dynamic value definition.
    #[inline(always)]
    pub const fn def(&self) -> DynamicValueDef {
        self.def
    }

    /// Returns the underlying `Poke` as a read-only `Peek`.
    #[inline]
    pub fn as_peek(&self) -> crate::Peek<'_, 'facet> {
        self.value.as_peek()
    }

    /// Returns the kind of value stored.
    #[inline]
    pub fn kind(&self) -> DynValueKind {
        unsafe { (self.def.vtable.get_kind)(self.value.data()) }
    }

    /// Returns true if the value is null.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.kind() == DynValueKind::Null
    }

    /// Returns the boolean value if this is a bool, `None` otherwise.
    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        unsafe { (self.def.vtable.get_bool)(self.value.data()) }
    }

    /// Returns the i64 value if representable, `None` otherwise.
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        unsafe { (self.def.vtable.get_i64)(self.value.data()) }
    }

    /// Returns the u64 value if representable, `None` otherwise.
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        unsafe { (self.def.vtable.get_u64)(self.value.data()) }
    }

    /// Returns the f64 value if this is a number, `None` otherwise.
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        unsafe { (self.def.vtable.get_f64)(self.value.data()) }
    }

    /// Returns the string value if this is a string, `None` otherwise.
    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        unsafe { (self.def.vtable.get_str)(self.value.data()) }
    }

    /// Returns the bytes value if this is bytes, `None` otherwise.
    #[inline]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.def
            .vtable
            .get_bytes
            .and_then(|f| unsafe { f(self.value.data()) })
    }

    /// Returns the length of the array if this is an array, `None` otherwise.
    #[inline]
    pub fn array_len(&self) -> Option<usize> {
        unsafe { (self.def.vtable.array_len)(self.value.data()) }
    }

    /// Returns the length of the object if this is an object, `None` otherwise.
    #[inline]
    pub fn object_len(&self) -> Option<usize> {
        unsafe { (self.def.vtable.object_len)(self.value.data()) }
    }

    /// Helper: drop the existing value and return a `PtrUninit` to the same location.
    #[inline]
    unsafe fn drop_and_as_uninit(&mut self) -> PtrUninit {
        unsafe { self.value.shape.call_drop_in_place(self.value.data_mut()) };
        PtrUninit::new(self.value.data_mut().as_mut_byte_ptr())
    }

    /// Replace the value with `null`, dropping the previous contents.
    pub fn set_null(&mut self) {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.set_null)(uninit);
        }
    }

    /// Replace the value with a boolean, dropping the previous contents.
    pub fn set_bool(&mut self, v: bool) {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.set_bool)(uninit, v);
        }
    }

    /// Replace the value with an i64, dropping the previous contents.
    pub fn set_i64(&mut self, v: i64) {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.set_i64)(uninit, v);
        }
    }

    /// Replace the value with a u64, dropping the previous contents.
    pub fn set_u64(&mut self, v: u64) {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.set_u64)(uninit, v);
        }
    }

    /// Replace the value with an f64, dropping the previous contents.
    ///
    /// Returns `false` if the value is not representable by the underlying type.
    pub fn set_f64(&mut self, v: f64) -> bool {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.set_f64)(uninit, v)
        }
    }

    /// Replace the value with a string, dropping the previous contents.
    pub fn set_str(&mut self, v: &str) {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.set_str)(uninit, v);
        }
    }

    /// Replace the value with a byte slice, dropping the previous contents.
    ///
    /// Returns `false` if the underlying dynamic value type doesn't support bytes.
    pub fn set_bytes(&mut self, v: &[u8]) -> bool {
        let Some(set_bytes) = self.def.vtable.set_bytes else {
            return false;
        };
        unsafe {
            let uninit = self.drop_and_as_uninit();
            set_bytes(uninit, v);
        }
        true
    }

    /// Replace the value with an empty array, dropping the previous contents.
    pub fn set_array(&mut self) {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.begin_array)(uninit);
        }
    }

    /// Replace the value with an empty object, dropping the previous contents.
    pub fn set_object(&mut self) {
        unsafe {
            let uninit = self.drop_and_as_uninit();
            (self.def.vtable.begin_object)(uninit);
        }
    }

    /// Push an element onto the array value.
    ///
    /// The value must already be an array (use [`set_array`](Self::set_array) first if needed).
    /// The element's shape must match this dynamic value's shape — a nested element is itself
    /// a full dynamic value of the same kind (e.g. another `facet_value::Value`).
    ///
    /// The element is moved into the array (the vtable does the `ptr::read`); the caller's
    /// original ownership of `element` is consumed by this call.
    pub fn push_array_element<T: Facet<'facet>>(&mut self, element: T) -> Result<(), ReflectError> {
        if self.value.shape != T::SHAPE {
            return Err(self.value.err(ReflectErrorKind::WrongShape {
                expected: self.value.shape,
                actual: T::SHAPE,
            }));
        }
        let mut element = ManuallyDrop::new(element);
        unsafe {
            let elem_ptr = PtrMut::new(&mut element as *mut ManuallyDrop<T> as *mut u8);
            (self.def.vtable.push_array_element)(self.value.data_mut(), elem_ptr);
        }
        Ok(())
    }

    /// Type-erased [`push_array_element`](Self::push_array_element).
    ///
    /// Accepts a [`HeapValue`] whose shape must match this dynamic value's shape.
    pub fn push_array_element_from_heap<const BORROW: bool>(
        &mut self,
        element: HeapValue<'facet, BORROW>,
    ) -> Result<(), ReflectError> {
        if self.value.shape != element.shape() {
            return Err(self.value.err(ReflectErrorKind::WrongShape {
                expected: self.value.shape,
                actual: element.shape(),
            }));
        }
        let mut element = element;
        let guard = element
            .guard
            .take()
            .expect("HeapValue guard was already taken");
        unsafe {
            let elem_ptr = PtrMut::new(guard.ptr.as_ptr());
            (self.def.vtable.push_array_element)(self.value.data_mut(), elem_ptr);
        }
        drop(guard);
        Ok(())
    }

    /// Finalize an array value. No-op if the underlying dynamic-value type doesn't need it.
    pub fn end_array(&mut self) {
        if let Some(end_array) = self.def.vtable.end_array {
            unsafe { end_array(self.value.data_mut()) };
        }
    }

    /// Insert a key-value pair into the object value.
    ///
    /// The value must already be an object (use [`set_object`](Self::set_object) first if needed).
    /// The value's shape must match this dynamic value's shape — a nested value is itself a full
    /// dynamic value of the same kind.
    ///
    /// `value` is moved into the object (the vtable does the `ptr::read`).
    pub fn insert_object_entry<T: Facet<'facet>>(
        &mut self,
        key: &str,
        value: T,
    ) -> Result<(), ReflectError> {
        if self.value.shape != T::SHAPE {
            return Err(self.value.err(ReflectErrorKind::WrongShape {
                expected: self.value.shape,
                actual: T::SHAPE,
            }));
        }
        let mut value = ManuallyDrop::new(value);
        unsafe {
            let value_ptr = PtrMut::new(&mut value as *mut ManuallyDrop<T> as *mut u8);
            (self.def.vtable.insert_object_entry)(self.value.data_mut(), key, value_ptr);
        }
        Ok(())
    }

    /// Type-erased [`insert_object_entry`](Self::insert_object_entry).
    ///
    /// Accepts a [`HeapValue`] whose shape must match this dynamic value's shape.
    pub fn insert_object_entry_from_heap<const BORROW: bool>(
        &mut self,
        key: &str,
        value: HeapValue<'facet, BORROW>,
    ) -> Result<(), ReflectError> {
        if self.value.shape != value.shape() {
            return Err(self.value.err(ReflectErrorKind::WrongShape {
                expected: self.value.shape,
                actual: value.shape(),
            }));
        }
        let mut value = value;
        let guard = value
            .guard
            .take()
            .expect("HeapValue guard was already taken");
        unsafe {
            let value_ptr = PtrMut::new(guard.ptr.as_ptr());
            (self.def.vtable.insert_object_entry)(self.value.data_mut(), key, value_ptr);
        }
        drop(guard);
        Ok(())
    }

    /// Finalize an object value. No-op if the underlying dynamic-value type doesn't need it.
    pub fn end_object(&mut self) {
        if let Some(end_object) = self.def.vtable.end_object {
            unsafe { end_object(self.value.data_mut()) };
        }
    }

    /// Get a mutable `Poke` for the value at the given object key.
    ///
    /// Returns `None` if the dynamic value is not an object, the key is missing, or
    /// `object_get_mut` is not implemented for this type.
    #[inline]
    pub fn object_get_mut(&mut self, key: &str) -> Option<Poke<'_, 'facet>> {
        let object_get_mut = self.def.vtable.object_get_mut?;
        let inner_ptr = unsafe { object_get_mut(self.value.data_mut(), key)? };
        // Nested dynamic values share the outer shape.
        Some(unsafe { Poke::from_raw_parts(inner_ptr, self.value.shape) })
    }

    /// Converts this `PokeDynamicValue` back into a `Poke`.
    #[inline]
    pub const fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekDynamicValue` view.
    #[inline]
    pub fn as_peek_dynamic_value(&self) -> crate::PeekDynamicValue<'_, 'facet> {
        crate::PeekDynamicValue {
            value: self.value.as_peek(),
            def: self.def,
        }
    }
}
