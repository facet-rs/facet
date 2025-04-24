use super::Shape;

/// Describes a reference or a pointer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[must_use]
#[non_exhaustive]
pub struct RefPtrDef {
    /// Reference or pointer?
    pub typ: RefPtrType,

    /// Mutability
    pub mutability: RefPtrMutability,

    /// shape of the inner type of the smart pointer, if not opaque
    pub pointee: Option<&'static Shape>,
}

/// Reference or pointer?
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefPtrType {
    /// Reference: `&T` or `&mut T`
    Reference,

    /// Pointer: `*const T` or `*mut T`
    Pointer,
}

/// `const` or `mut`?
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefPtrMutability {
    /// Const: `&T` or `*const T`
    Const,
    /// Mut: `&mut T` or `*mut T`
    Mut,
}

impl RefPtrDef {
    /// Creates a new `RefPtrDefBuilder` with all fields set to `None`.
    pub const fn builder() -> RefPtrDefBuilder {
        RefPtrDefBuilder {
            typ: None,
            mutability: None,
            pointee: None,
        }
    }
}

/// Builder for creating a `RefPointerDef`.
#[derive(Debug)]
#[must_use]
pub struct RefPtrDefBuilder {
    typ: Option<RefPtrType>,
    mutability: Option<RefPtrMutability>,
    pointee: Option<&'static Shape>,
}

impl RefPtrDefBuilder {
    /// Creates a new `RefPtrDefBuilder` with all fields set to `None`.
    #[expect(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            typ: None,
            mutability: None,
            pointee: None,
        }
    }

    /// Sets the type of the reference/pointer.
    pub const fn typ(mut self, typ: RefPtrType) -> Self {
        self.typ = Some(typ);
        self
    }

    /// Sets the mutability of the reference/pointer.
    pub const fn mutability(mut self, mutability: RefPtrMutability) -> Self {
        self.mutability = Some(mutability);
        self
    }

    /// Sets the shape of the inner type of the refernce or pointer.
    pub const fn pointee(mut self, pointee: &'static Shape) -> Self {
        self.pointee = Some(pointee);
        self
    }

    /// Builds a `RefPtrDef` from the provided configuration.
    ///
    /// # Panics
    ///
    /// Panics if any required field (typ, mutability) is not set.
    pub const fn build(self) -> RefPtrDef {
        RefPtrDef {
            typ: self.typ.unwrap(),
            mutability: self.mutability.unwrap(),
            pointee: self.pointee,
        }
    }
}
