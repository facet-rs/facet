use core::fmt;

use crate::{Facet, OxRef, PtrConst, Shape};

/// An attribute attaches metadata to a container or a field
///
/// Attributes use syntax like `#[facet(sensitive)]` for builtins or
/// `#[facet(orm::primary_key)]` for namespaced extension attributes.
///
/// The derive macro expands attributes to macro invocations that
/// return `Attr` values with typed data.
///
/// # Why the fields are private
///
/// `Attr` carries a hand-written `unsafe impl Sync` (see below), and the whole
/// of that impl's soundness rests on one claim: *the referent behind `data` is
/// `Sync`*. `OxRef` is type-erased, so the compiler cannot check that claim —
/// only the construction site can. Every constructor is therefore either
/// bound by `T: Sync` ([`Attr::new`], [`Attr::new_shape`]) or `unsafe`
/// ([`Attr::from_raw_parts`]).
///
/// Public fields would be a fourth, *safe and unchecked*, constructor: a struct
/// literal skips the constructor entirely, and [`OxRef::from_ref`] carries no
/// `Sync` bound of its own (correctly so — `OxRef` is itself neither `Send` nor
/// `Sync`, so it needs none). That is exactly how facet-rs/facet#1573 stayed
/// open after its fix in `1524e017a`: the `T: Sync` bound was added to
/// `Attr::new`, but the fields were already `pub`, so
/// `Attr { ns: None, key: "", data: OxRef::from_ref(rc) }` reached the same
/// data race from 100% safe code. Read the fields through [`Attr::ns`],
/// [`Attr::key`] and [`Attr::data`] instead.
pub struct Attr {
    /// The namespace (e.g., Some("orm") in `#[facet(orm::primary_key)]`).
    /// None for builtin attributes like `#[facet(sensitive)]`.
    ns: Option<&'static str>,

    /// The key (e.g., "primary_key" in `#[facet(orm::primary_key)]`)
    key: &'static str,

    /// Data stored by the attribute
    data: OxRef<'static>,
}

// SAFETY: `Attr` only holds `&'static T` where `T: Sync`. That is enforced by
// the `T: Sync` bound on `Attr::new`/`Attr::new_shape`, by the safety contract
// of `Attr::from_raw_parts`, and — critically — by the fields being private, so
// that those three are the *only* ways to build an `Attr`. `&'static T: Send`
// and `&'static T: Sync` both reduce to `T: Sync`, so the same bound licenses
// both impls below.
unsafe impl Send for Attr {}
unsafe impl Sync for Attr {}

impl Attr {
    /// Create an attribute from its already-erased parts.
    ///
    /// This is the entry point for code that has built the [`OxRef`] itself —
    /// notably the derive macro, which stores `const`-promoted function
    /// pointers whose types it cannot name at the expansion site. Prefer
    /// [`Attr::new`], which checks the `Sync` requirement for you.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that:
    /// - `data` upholds [`OxRef::new`]'s contract: the pointer is valid,
    ///   points to initialized data matching the shape, and stays valid and
    ///   immutably borrowed for `'static`.
    /// - **The pointed-to type is `Sync`.** `Attr` is `Send + Sync` by the
    ///   `unsafe impl`s above, so an `Attr` built over non-`Sync` data can be
    ///   shared across threads and read concurrently — e.g. an `Rc` whose
    ///   non-atomic refcount then races. This is the requirement that
    ///   facet-rs/facet#1573 is about; it is *not* implied by the first bullet.
    #[inline]
    pub const unsafe fn from_raw_parts(
        ns: Option<&'static str>,
        key: &'static str,
        data: OxRef<'static>,
    ) -> Self {
        Self { ns, key, data }
    }

    /// The namespace (e.g., `Some("orm")` in `#[facet(orm::primary_key)]`).
    /// `None` for builtin attributes like `#[facet(sensitive)]`.
    #[inline]
    pub const fn ns(&self) -> Option<&'static str> {
        self.ns
    }

    /// The key (e.g., `"primary_key"` in `#[facet(orm::primary_key)]`).
    #[inline]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// The type-erased data stored by the attribute.
    ///
    /// [`OxRef`] is `Copy` and is itself neither `Send` nor `Sync`, so handing
    /// one out does not leak `Attr`'s `Sync` promise to the caller.
    #[inline]
    pub const fn data(&self) -> OxRef<'static> {
        self.data
    }

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
