use alloc::{boxed::Box, string::String};

use crate::{Partial, Peek, ReflectError, trace};
use core::marker::PhantomData;
use facet_core::{Facet, PtrConst, PtrUninit, Shape, Variant};

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

////////////////////////////////////////////////////////////////////////////////////////////////////
// Misc.
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Returns the current frame count (depth of nesting)
    ///
    /// The initial frame count is 1 — `begin_field` would push a new frame,
    /// bringing it to 2, then `end` would bring it back to `1`.
    ///
    /// This is an implementation detail of `Partial`, kinda, but deserializers
    /// might use this for debug assertions, to make sure the state is what
    /// they think it is.
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.inner.frame_count()
    }

    /// Returns the shape of the current frame.
    #[inline]
    pub fn shape(&self) -> &'static Shape {
        self.inner.shape()
    }

    /// Pops the current frame off the stack, indicating we're done initializing the current field
    #[inline]
    pub fn end(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.end()?;
        Ok(self)
    }

    /// Returns a human-readable path representing the current traversal in the builder,
    /// e.g., `RootStruct.fieldName[index].subfield`.
    pub fn path(&self) -> String {
        self.inner.path()
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Build
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T> TypedPartial<'facet, T> {
    /// Builds the value and returns a `Box<T>`
    pub fn build(&mut self) -> Result<Box<T>, ReflectError>
    where
        T: Facet<'facet>,
    {
        self.inner.require_active()?;

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

////////////////////////////////////////////////////////////////////////////////////////////////////
// `Set` and set helpers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Sets a value wholesale into the current frame.
    ///
    /// If the current frame was already initialized, the previous value is
    /// dropped. If it was partially initialized, the fields that were initialized
    /// are dropped, etc.
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
    #[inline]
    pub unsafe fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'static Shape,
    ) -> Result<&mut Self, ReflectError> {
        unsafe { self.inner.set_shape(src_value, src_shape)? };
        Ok(self)
    }

    /// Sets the current frame using a function that initializes the value
    ///
    /// # Safety
    ///
    /// If `f` returns Ok(), it is assumed that it initialized the passed pointer fully and with a
    /// value of the right type.
    ///
    /// If `f` returns Err(), it is assumed that it did NOT initialize the passed pointer and that
    /// there is no need to drop it in place.
    #[inline]
    pub unsafe fn set_from_function<F>(&mut self, f: F) -> Result<&mut Self, ReflectError>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError>,
    {
        unsafe {
            self.inner.set_from_function(f)?;
        }
        Ok(self)
    }

    /// Sets the current frame to its default value using `default_in_place` from the
    /// vtable.
    ///
    /// Note: if you have `struct S { field: F }`, and `F` does not implement `Default`
    /// but `S` does, this doesn't magically uses S's `Default` implementation to get a value
    /// for `field`.
    ///
    /// If the current frame's shape does not implement `Default`, then this returns an error.
    #[inline]
    pub fn set_default(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.set_default()?;
        Ok(self)
    }

    /// Copy a value from a Peek into the current frame.
    ///
    /// # Invariants
    ///
    /// `peek` must be a thin pointer, otherwise this panics.
    ///
    /// # Safety
    ///
    /// If this succeeds, the value `Peek` points to has been moved out of, and
    /// as such, should not be dropped (but should be deallocated).
    pub unsafe fn set_from_peek(&mut self, peek: &Peek<'_, '_>) -> Result<&mut Self, ReflectError> {
        unsafe {
            self.inner.set_from_peek(peek)?;
        }
        Ok(self)
    }

    /// Parses a string value into the current frame using the type's ParseFn from the vtable.
    ///
    /// If the current frame was previously initialized, its contents are dropped in place.
    #[inline]
    pub fn parse_from_str(&mut self, s: &str) -> Result<&mut Self, ReflectError> {
        self.inner.parse_from_str(s)?;
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Enum variant selection
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Get the currently selected variant for an enum
    #[inline]
    pub fn selected_variant(&self) -> Option<Variant> {
        self.inner.selected_variant()
    }

    /// Find a variant by name in the current enum
    #[inline]
    pub fn find_variant(&self, variant_name: &str) -> Option<(usize, &'static Variant)> {
        self.inner.find_variant(variant_name)
    }

    /// Assuming the current frame is an enum, this selects a variant by index
    /// (0-based, in declaration order).
    ///
    /// For example:
    ///
    /// ```rust,no_run
    /// enum E { A, B, C }
    /// ```
    ///
    /// Calling `select_nth_variant(2)` would select variant `C`.
    ///
    /// This will return an error if the current frame is anything other than fully-uninitialized.
    /// In other words, it's not possible to "switch to a different variant" once you've selected one.
    ///
    /// This does _not_ push a frame on the stack.
    #[inline]
    pub fn select_nth_variant(&mut self, index: usize) -> Result<&mut Self, ReflectError> {
        self.inner.select_nth_variant(index)?;
        Ok(self)
    }

    /// Pushes a variant for enum initialization by name
    ///
    /// See [Self::select_nth_variant] for more notes.
    #[inline]
    pub fn select_variant_named(&mut self, variant_name: &str) -> Result<&mut Self, ReflectError> {
        self.inner.select_variant_named(variant_name)?;
        Ok(self)
    }

    /// Selects a given enum variant by discriminant. If none of the variants
    /// of the frame's enum have that discriminant, this returns an error.
    ///
    /// See [Self::select_nth_variant] for more notes.
    #[inline]
    pub fn select_variant(&mut self, discriminant: i64) -> Result<&mut Self, ReflectError> {
        self.inner.select_variant(discriminant)?;
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Field selection
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Find the index of a field by name in the current struct
    ///
    /// If the current frame isn't a struct or an enum (with a selected variant)
    /// then this returns `None` for sure.
    #[inline]
    pub fn field_index(&self, field_name: &str) -> Option<usize> {
        self.inner.field_index(field_name)
    }

    /// Check if a struct field at the given index has been set
    #[inline]
    pub fn is_field_set(&self, index: usize) -> Result<bool, ReflectError> {
        self.inner.is_field_set(index)
    }

    /// Selects a field (by name) of a struct or enum data.
    ///
    /// For enums, the variant needs to be selected first, see [Self::select_nth_variant]
    /// and friends.
    #[inline]
    pub fn begin_field(&mut self, field_name: &str) -> Result<&mut Self, ReflectError> {
        self.inner.begin_field(field_name)?;
        Ok(self)
    }

    /// Begins the nth field of a struct, enum variant, or array, by index.
    ///
    /// On success, this pushes a new frame which must be ended with a call to [Partial::end]
    #[inline]
    pub fn begin_nth_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.inner.begin_nth_field(idx)?;
        Ok(self)
    }

    /// Sets the given field to its default value, preferring:
    ///
    ///   * A `default = some_fn()` function
    ///   * The field's `Default` implementation if any
    ///
    /// But without going all the way up to the parent struct's `Default` impl.
    ///
    /// Errors out if idx is out of bound, if the field has no default method or Default impl.
    #[inline]
    pub fn set_nth_field_to_default(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.inner.set_nth_field_to_default(idx)?;
        Ok(self)
    }

    /// Given a `Partial` for the same shape, and assuming that partial has the nth
    /// field initialized, move the value from `src` to `self`, marking it as deinitialized
    /// in `src`.
    #[inline]
    pub fn steal_nth_field(
        &mut self,
        src: &mut TypedPartial<T>,
        field_index: usize,
    ) -> Result<&mut Self, ReflectError> {
        self.inner.steal_nth_field(&mut src.inner, field_index)?;
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Smart pointers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Pushes a frame to initialize the inner value of a smart pointer (`Box<T>`, `Arc<T>`, etc.)
    #[inline]
    pub fn begin_smart_ptr(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_smart_ptr()?;
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Lists
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Initializes a list (Vec, etc.) if it hasn't been initialized before.
    /// This is a prerequisite to `begin_push_item`/`set`/`end` or the shorthand
    /// `push`.
    ///
    /// `begin_list` does not clear the list if it was previously initialized.
    /// `begin_list` does not push a new frame to the stack, and thus does not
    /// require `end` to be called afterwards.
    #[inline]
    pub fn begin_list(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_list()?;
        Ok(self)
    }

    /// Pushes an element to the list
    /// The element should be set using `set()` or similar methods, then `pop()` to complete
    #[inline]
    pub fn begin_list_item(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_list_item()?;
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Maps
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Begins a map initialization operation
    ///
    /// This initializes the map with default capacity and allows inserting key-value pairs
    /// It does _not_ push a new frame onto the stack.
    #[inline]
    pub fn begin_map(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_map()?;
        Ok(self)
    }

    /// Pushes a frame for the map key. After that, `set()` should be called
    /// (or the key should be initialized somehow) and `end()` should be called
    /// to pop the frame.
    #[inline]
    pub fn begin_key(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_key()?;
        Ok(self)
    }

    /// Pushes a frame for the map value
    /// Must be called after the key has been set and popped
    #[inline]
    pub fn begin_value(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_value()?;
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Option / inner
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Begin building the `Some` variant of an `Option`
    #[inline]
    pub fn begin_some(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_some()?;
        Ok(self)
    }

    /// Begin building the inner value of a wrapper type
    #[inline]
    pub fn begin_inner(&mut self) -> Result<&mut Self, ReflectError> {
        self.inner.begin_inner()?;
        Ok(self)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Shorthands
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, T: ?Sized> TypedPartial<'facet, T> {
    /// Convenience shortcut: sets the field at index `idx` directly to value, popping after.
    #[inline]
    pub fn set_nth_field<U>(&mut self, idx: usize, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_nth_field(idx, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the named field to value, popping after.
    #[inline]
    pub fn set_field<U>(&mut self, field_name: &str, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_field(field_name, value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the key for a map key-value insertion, then pops after.
    #[inline]
    pub fn set_key<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_key(value)?;
        Ok(self)
    }

    /// Convenience shortcut: sets the value for a map key-value insertion, then pops after.
    #[inline]
    pub fn set_value<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.set_value(value)?;
        Ok(self)
    }

    /// Shorthand for: begin_list_item(), set(), end(), useful when pushing a scalar
    #[inline]
    pub fn push<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.inner.push(value)?;
        Ok(self)
    }
}

impl<'facet, T> core::fmt::Debug for TypedPartial<'facet, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TypedPartial")
            .field("shape", &self.inner.frames.last().map(|frame| frame.shape))
            .finish()
    }
}
