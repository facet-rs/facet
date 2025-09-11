use core::marker::PhantomData;

use crate::trace;
use facet_core::{Facet, PtrConst, PtrUninit, Shape, Variant};

use crate::{Partial, ReflectError};

/// A typed wrapper around `Partial`, for when you want to statically
/// ensure that `build` gives you the proper type.
pub struct TypedPartial<'facet, T: ?Sized> {
    pub(crate) inner: Partial<'facet>,
    pub(crate) phantom: PhantomData<T>,
}

impl<'facet, T> TypedPartial<'facet, T> {
    /// Borrows the inner [Partial] mutably
    pub fn inner_mut(&mut self) -> &mut Partial<'facet> {
        &mut self.inner
    }
}

// This impl block mirrors/forwards/delegates methods of Partial
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Returns the current frame count (depth of nesting)
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.inner.frame_count()
    }

    /// Sets a value wholesale into the current frame
    #[inline]
    pub fn set<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set(value)?;
        Ok(self)
    }

    /// Sets a value into the current frame by shape, for shape-based operations
    ///
    /// If this returns Ok, then `src_value` has been moved out of
    ///
    /// # Safety
    ///
    /// The caller must ensure that `src_value` points to a valid instance of a value
    /// whose memory layout and type matches `src_shape`, and that this value can be
    /// safely copied (bitwise) into the destination specified by the Partial's current frame.
    /// No automatic drop will be performed for any existing value, so calling this on an
    /// already-initialized destination may result in leaks or double drops if misused.
    /// After a successful call, the ownership of the value at `src_value` is effectively moved
    /// into the Partial (i.e., the destination), and the original value should not be used
    /// or dropped by the caller; consider using `core::mem::forget` on the passed value.
    /// If an error is returned, the destination remains unmodified and safe for future operations.
    pub unsafe fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'static Shape,
    ) -> Result<&mut Self, ReflectError> {
        unsafe { self.inner.set_shape(src_value, src_shape)? };
        Ok(self)
    }

    /// Sets the current frame to its default value (if available)
    pub fn set_default(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.set_default()?;
        Ok(self)
    }

    /// See [Partial::set_from_function]
    ///
    /// # Safety
    ///
    /// See [Partial::set_from_function]
    pub unsafe fn set_from_function<F>(&mut self, f: F) -> Result<&mut Self, ReflectError>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError>,
    {
        unsafe {
            self.inner.set_from_function(f)?;
        }
        Ok(self)
    }

    /// See [Partial::parse_from_str]
    pub fn parse_from_str(&mut self, s: &str) -> Result<&mut Self, ReflectError> {
        self.inner.parse_from_str(s)?;
        Ok(self)
    }

    /// See [Partial::select_variant_named]
    pub fn select_variant_named(&mut self, variant_name: &str) -> Result<&mut Self, ReflectError> {
        self.inner.select_variant_named(variant_name)?;
        Ok(self)
    }

    /// See [Partial::select_variant]
    pub fn select_variant(&mut self, discriminant: i64) -> Result<&mut Self, ReflectError> {
        self.inner.select_variant(discriminant)?;
        Ok(self)
    }

    /// See [Partial::select_nth_variant]
    pub fn select_nth_variant(&mut self, index: usize) -> Result<&mut Self, ReflectError> {
        self.inner.select_nth_variant(index)?;
        Ok(self)
    }

    /// See [Partial::begin_field]
    pub fn begin_field(&mut self, field_name: &str) -> Result<&mut Self, ReflectError> {
        self.inner.begin_field(field_name)?;
        Ok(self)
    }

    /// See [Partial::begin_nth_field]
    pub fn begin_nth_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.inner.begin_nth_field(idx)?;
        Ok(self)
    }

    /// See [Partial::begin_nth_element]
    pub fn begin_nth_element(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.inner.begin_nth_element(idx)?;
        Ok(self)
    }

    /// See [Partial::begin_nth_enum_field]
    pub fn begin_nth_enum_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.inner.begin_nth_enum_field(idx)?;
        Ok(self)
    }

    /// See [Partial::begin_smart_ptr]
    pub fn begin_smart_ptr(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_smart_ptr()?;
        Ok(self)
    }

    /// See [Partial::begin_list]
    pub fn begin_list(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_list()?;
        Ok(self)
    }

    /// See [Partial::begin_list_item]
    pub fn begin_list_item(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_list_item()?;
        Ok(self)
    }

    /// See [Partial::begin_map]
    pub fn begin_map(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_map()?;
        Ok(self)
    }

    /// See [Partial::begin_key]
    pub fn begin_key(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_key()?;
        Ok(self)
    }

    /// See [Partial::begin_value]
    pub fn begin_value(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_value()?;
        Ok(self)
    }

    /// See [Partial::end]
    pub fn end(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.end()?;
        Ok(self)
    }

    /// Returns a human-readable path representing the current traversal in the builder,
    /// e.g., `RootStruct.fieldName[index].subfield`.
    pub fn path(&self) -> String {
        self.inner.path()
    }

    /// Returns the shape of the current frame.
    pub fn shape(&self) -> &'static Shape {
        self.inner.shape()
    }

    /// Check if a struct field at the given index has been set
    pub fn is_field_set(&self, index: usize) -> Result<bool, ReflectError> {
        self.inner.is_field_set(index)
    }

    /// Find the index of a field by name in the current struct
    pub fn field_index(&self, field_name: &str) -> Option<usize> {
        self.inner.field_index(field_name)
    }

    /// Get the currently selected variant for an enum
    pub fn selected_variant(&self) -> Option<Variant> {
        self.inner.selected_variant()
    }

    /// Find a variant by name in the current enum
    pub fn find_variant(&self, variant_name: &str) -> Option<(usize, &'static Variant)> {
        self.inner.find_variant(variant_name)
    }

    /// Begin building the `Some` variant of an `Option`
    pub fn begin_some(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_some()?;
        Ok(self)
    }

    /// Begin building the inner value of a wrapper type
    pub fn begin_inner(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_inner()?;
        Ok(self)
    }

    /// Convenience shortcut: sets the nth element of an array directly to value, popping after.
    pub fn set_nth_element<U>(&mut self, idx: usize, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_nth_element(idx, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the field at index `idx` directly to value, popping after.
    pub fn set_nth_field<U>(&mut self, idx: usize, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_nth_field(idx, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the named field to value, popping after.
    pub fn set_field<U>(&mut self, field_name: &str, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_field(field_name, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the nth field of an enum variant directly to value, popping after.
    pub fn set_nth_enum_field<U>(&mut self, idx: usize, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_nth_enum_field(idx, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the key for a map key-value insertion, then pops after.
    pub fn set_key<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_key(value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the value for a map key-value insertion, then pops after.
    pub fn set_value<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_value(value)?;
        Ok(self)
    }

    /// Shorthand for: begin_list_item(), set(), end(), useful when pushing a scalar
    pub fn push<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.push(value)?;
        Ok(self)
    }
}

impl<'facet, T> TypedPartial<'facet, T> {
    /// Builds the value and returns a `Box<T>`
    pub fn build(&mut self) -> Result<Box<T>, ReflectError>
    where
        T: Facet<'facet>,
    {
        trace!(
            "TypedPartial::build: Building value for type {} which should == {}",
            T::SHAPE,
            self.inner.shape()
        );
        let heap_value = self.inner.build()?;
        trace!(
            "TypedPartial::build: Built heap value with shape: {}",
            heap_value.shape()
        );
        // Safety: HeapValue was constructed from T and the shape layout is correct.
        let result = unsafe { heap_value.into_box_unchecked::<T>() };
        trace!("TypedPartial::build: Successfully converted to Box<T>");
        Ok(result)
    }
}

impl<'facet, T> core::fmt::Debug for TypedPartial<'facet, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TypedPartial")
            .field("shape", &self.inner.frames.last().map(|frame| frame.shape))
            .finish()
    }
}
