use core::cmp::Ordering;
use facet_core::{Opaque, OpaqueConst, Shape, TypeNameOpts, ValueVTable};

use crate::Peek;

/// Lets you read from a value (implements read-only [`ValueVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekValue<'mem> {
    data: OpaqueConst<'mem>,
    shape: &'static Shape,
}

impl core::fmt::Display for PeekValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(display_fn) = self.vtable().display {
            unsafe { display_fn(self.data, f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

impl core::fmt::Debug for PeekValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = self.vtable().debug {
            unsafe { debug_fn(self.data, f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

impl core::cmp::PartialEq for PeekValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        let eq_fn = match self.shape.vtable.eq {
            Some(eq_fn) => eq_fn,
            None => return false,
        };
        unsafe { eq_fn(self.data, other.data) }
    }
}

impl core::cmp::PartialOrd for PeekValue<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.shape != other.shape {
            return None;
        }
        let partial_ord_fn = self.shape.vtable.partial_ord?;
        unsafe { partial_ord_fn(self.data, other.data) }
    }
}

impl<'mem> PeekValue<'mem> {
    /// Creates a new `PeekValue` instance.
    ///
    /// # Safety
    ///
    /// `data` must be initialized and well-aligned, and point to a value
    /// of the type described by `shape`.
    pub(crate) unsafe fn unchecked_new(data: OpaqueConst<'mem>, shape: &'static Shape) -> Self {
        Self { data, shape }
    }

    /// Returns the vtable
    #[inline(always)]
    pub fn vtable(&self) -> &'static ValueVTable {
        self.shape.vtable
    }

    /// Returns true if this scalar is equal to the other scalar
    ///
    /// # Returns
    ///
    /// `false` if equality comparison is not supported for this scalar type
    #[inline]
    pub fn eq(&self, other: &PeekValue<'_>) -> Option<bool> {
        unsafe {
            self.shape
                .vtable
                .eq
                .map(|eq_fn| eq_fn(self.data, other.data))
        }
    }

    /// Compares this scalar with another and returns their ordering
    ///
    /// # Returns
    ///
    /// `None` if comparison is not supported for this scalar type
    #[inline]
    pub fn partial_cmp(&self, other: &PeekValue<'_>) -> Option<Ordering> {
        unsafe {
            self.shape
                .vtable
                .partial_ord
                .and_then(|partial_ord_fn| partial_ord_fn(self.data, other.data))
        }
    }

    /// Compares this scalar with another and returns their ordering
    ///
    /// # Returns
    ///
    /// `None` if comparison is not supported for this scalar type
    #[inline]
    // Note: we cannot implement `Ord` for `PeekValue` because the underlying shape might
    // not implement `Ord` — unlike the `PartialOrd` case where we can just pretend it
    // could not order two specific values.
    #[expect(clippy::should_implement_trait)]
    pub fn cmp(&self, other: &PeekValue<'_>) -> Option<Ordering> {
        unsafe {
            self.shape
                .vtable
                .ord
                .map(|ord_fn| ord_fn(self.data, other.data))
        }
    }

    /// Returns true if this scalar is greater than the other scalar
    ///
    /// # Returns
    ///
    /// `false` if comparison is not supported for this scalar type
    #[inline]
    pub fn gt(&self, other: &PeekValue<'_>) -> bool {
        self.cmp(other)
            .map(|ordering| ordering == Ordering::Greater)
            .unwrap_or(false)
    }

    /// Returns true if this scalar is greater than or equal to the other scalar
    ///
    /// # Returns
    ///
    /// `false` if comparison is not supported for this scalar type
    #[inline]
    pub fn gte(&self, other: &PeekValue<'_>) -> bool {
        self.cmp(other)
            .map(|ordering| ordering == Ordering::Greater || ordering == Ordering::Equal)
            .unwrap_or(false)
    }

    /// Returns true if this scalar is less than the other scalar
    ///
    /// # Returns
    ///
    /// `false` if comparison is not supported for this scalar type
    #[inline]
    pub fn lt(&self, other: &PeekValue<'_>) -> bool {
        self.cmp(other)
            .map(|ordering| ordering == Ordering::Less)
            .unwrap_or(false)
    }

    /// Returns true if this scalar is less than or equal to the other scalar
    ///
    /// # Returns
    ///
    /// `false` if comparison is not supported for this scalar type
    #[inline(always)]
    pub fn lte(&self, other: &PeekValue<'_>) -> bool {
        self.cmp(other)
            .map(|ordering| ordering == Ordering::Less || ordering == Ordering::Equal)
            .unwrap_or(false)
    }

    /// Formats this scalar for display
    ///
    /// # Returns
    ///
    /// `None` if display formatting is not supported for this scalar type
    #[inline(always)]
    pub fn display(&self, f: &mut core::fmt::Formatter<'_>) -> Option<core::fmt::Result> {
        unsafe {
            self.shape
                .vtable
                .display
                .map(|display_fn| display_fn(self.data, f))
        }
    }

    /// Formats this scalar for debug
    ///
    /// # Returns
    ///
    /// `None` if debug formatting is not supported for this scalar type
    #[inline(always)]
    pub fn debug(&self, f: &mut core::fmt::Formatter<'_>) -> Option<core::fmt::Result> {
        unsafe {
            self.shape
                .vtable
                .debug
                .map(|debug_fn| debug_fn(self.data, f))
        }
    }

    /// Hashes this scalar
    ///
    /// # Returns
    ///
    /// `false` if hashing is not supported for this scalar type, `true` otherwise
    #[inline(always)]
    pub fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) -> bool {
        unsafe {
            if let Some(hash_fn) = self.shape.vtable.hash {
                let hasher_opaque = Opaque::new(hasher);
                hash_fn(self.data, hasher_opaque, |opaque, bytes| {
                    opaque.as_mut::<H>().write(bytes)
                });
                true
            } else {
                false
            }
        }
    }

    /// Returns the type name of this scalar
    ///
    /// # Arguments
    ///
    /// * `f` - A mutable reference to a `core::fmt::Formatter`
    /// * `opts` - The `TypeNameOpts` to use for formatting
    ///
    /// # Returns
    ///
    /// The result of the type name formatting
    #[inline(always)]
    pub fn type_name(
        &self,
        f: &mut core::fmt::Formatter<'_>,
        opts: TypeNameOpts,
    ) -> core::fmt::Result {
        (self.shape.vtable.type_name)(f, opts)
    }

    /// Returns the shape
    #[inline(always)]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Returns the data
    #[inline(always)]
    pub const fn data(&self) -> OpaqueConst<'mem> {
        self.data
    }

    /// Wraps this scalar back into a `Peek`
    #[inline(always)]
    pub fn wrap(self) -> Peek<'mem> {
        unsafe { Peek::unchecked_new(self.data, self.shape) }
    }
}
