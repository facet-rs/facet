use core::fmt;

use crate::{Facet, OxRef, PtrConst, Shape};

/// An attribute attaches metadata to a container or a field
///
/// Attributes use syntax like `#[facet(sensitive)]` for builtins or
/// `#[facet(orm::primary_key)]` for namespaced extension attributes.
///
/// The derive macro expands attributes to macro invocations that
/// return `Attr` values with typed data.
pub struct Attr {
    /// The namespace (e.g., Some("orm") in `#[facet(orm::primary_key)]`).
    /// None for builtin attributes like `#[facet(sensitive)]`.
    pub ns: Option<&'static str>,

    /// The key (e.g., "primary_key" in `#[facet(orm::primary_key)]`)
    pub key: &'static str,

    /// Data stored by the attribute
    pub data: OxRef<'static>,
}

// SAFETY: Attr only holds `&'static T` where `T: Sync` (enforced by `Attr::new`),
// so the data can be safely accessed from any thread.
unsafe impl Send for Attr {}
unsafe impl Sync for Attr {}

impl Attr {
    /// Create a new attribute with typed data.
    ///
    /// The data must be a static reference to a sized value that implements `Facet`.
    /// The `Sized` bound is required to allow const construction.
    /// The `Sync` bound is required because `Attr` is `Sync`, so the data must be
    /// safely accessible from any thread.
    #[inline]
    pub const fn new<T: Facet<'static> + Sized + Sync>(
        ns: Option<&'static str>,
        key: &'static str,
        data: &'static T,
    ) -> Self {
        Self {
            ns,
            key,
            // SAFETY: `data` is a valid &'static T reference, so the pointer is valid
            // for the 'static lifetime and the shape matches.
            data: unsafe { OxRef::new(PtrConst::new_sized(data as *const T), T::SHAPE) },
        }
    }

    /// Create a new attribute storing a Shape reference.
    ///
    /// This is a convenience method for `shape_type` variants.
    /// Since `Shape: Facet<'static>`, this just delegates to `new`.
    #[inline]
    pub const fn new_shape(
        ns: Option<&'static str>,
        key: &'static str,
        shape: &'static Shape,
    ) -> Self {
        Self::new(ns, key, shape)
    }

    /// Returns true if this is a builtin attribute (no namespace).
    #[inline]
    pub const fn is_builtin(&self) -> bool {
        self.ns.is_none()
    }

    /// Get a typed reference to the attribute data if the shape matches `T::SHAPE`.
    ///
    /// Returns `None` if the stored shape doesn't match the expected type.
    #[inline]
    pub fn get_as<T: Facet<'static>>(&self) -> Option<&T> {
        // SAFETY: We check that the shape matches T::SHAPE before casting
        unsafe { self.data.get_as::<T>(T::SHAPE) }
    }
}

impl fmt::Debug for Attr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Write the attribute name (with or without namespace)
        match self.ns {
            Some(ns) => write!(f, "{}::{}({:?})", ns, self.key, self.data),
            None => write!(f, "{}({:?})", self.key, self.data),
        }
    }
}

impl PartialEq for Attr {
    fn eq(&self, other: &Self) -> bool {
        // Compare by namespace and key only (args don't impl PartialEq, and we don't need to compare them)
        self.ns == other.ns && self.key == other.key
    }
}

/// An attribute that can be applied to a shape.
/// This is now just an alias for `ExtensionAttr` - all attributes use the same representation.
pub type ShapeAttribute = Attr;
