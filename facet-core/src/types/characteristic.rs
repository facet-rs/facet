use core::fmt;

use super::{Shape, TypeNameOpts};

/// A characteristic a shape can have
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Characteristic {
    // Functionality traits
    /// Implements Clone
    Clone,

    /// Implements Display
    Display,

    /// Implements Debug
    Debug,

    /// Implements PartialEq
    PartialEq,

    /// Implements PartialOrd
    PartialOrd,

    /// Implements Ord
    Ord,

    /// Implements Hash
    Hash,

    /// Implements Default
    Default,

    /// Implements FromStr
    FromStr,
}

impl Characteristic {
    /// Checks if all shapes have the given characteristic.
    #[inline]
    pub const fn all(self, shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if !shapes[i].is(self) {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Checks if any shape has the given characteristic.
    #[inline]
    pub const fn any(self, shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if shapes[i].is(self) {
                return true;
            }
            i += 1;
        }
        false
    }

    /// Checks if none of the shapes have the given characteristic.
    #[inline]
    pub const fn none(self, shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if shapes[i].is(self) {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Checks if all shapes have the `Default` characteristic
    #[inline]
    pub const fn all_default(shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if !shapes[i].is_default() {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Checks if all shapes have the `PartialEq` characteristic
    #[inline]
    pub const fn all_partial_eq(shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if !shapes[i].is_partial_eq() {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Checks if all shapes have the `PartialOrd` characteristic
    #[inline]
    pub const fn all_partial_ord(shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if !shapes[i].is_partial_ord() {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Checks if all shapes have the `Ord` characteristic
    #[inline]
    pub const fn all_ord(shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if !shapes[i].is_ord() {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Checks if all shapes have the `Hash` characteristic
    #[inline]
    pub const fn all_hash(shapes: &[&Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if !shapes[i].is_hash() {
                return false;
            }
            i += 1;
        }
        true
    }
}

impl Shape {
    /// Checks if a shape has the given characteristic.
    #[inline]
    pub const fn is(&self, characteristic: Characteristic) -> bool {
        match characteristic {
            // Functionality traits
            Characteristic::Clone => match self.type_ops {
                Some(ops) => ops.has_clone_into(),
                None => false,
            },
            Characteristic::Display => self.vtable.has_display(),
            Characteristic::Debug => self.vtable.has_debug(),
            Characteristic::PartialEq => self.vtable.has_partial_eq(),
            Characteristic::PartialOrd => self.vtable.has_partial_ord(),
            Characteristic::Ord => self.vtable.has_ord(),
            Characteristic::Hash => self.vtable.has_hash(),
            Characteristic::Default => match self.type_ops {
                Some(ops) => ops.has_default_in_place(),
                None => false,
            },
            Characteristic::FromStr => self.vtable.has_parse(),
        }
    }

    /// Check if this shape implements the Clone trait
    #[inline]
    pub const fn is_clone(&self) -> bool {
        self.is(Characteristic::Clone)
    }

    /// Check if this shape implements the Display trait
    #[inline]
    pub const fn is_display(&self) -> bool {
        self.is(Characteristic::Display)
    }

    /// Check if this shape implements the Debug trait
    #[inline]
    pub const fn is_debug(&self) -> bool {
        self.is(Characteristic::Debug)
    }

    /// Check if this shape implements the PartialEq trait
    #[inline]
    pub const fn is_partial_eq(&self) -> bool {
        self.is(Characteristic::PartialEq)
    }

    /// Check if this shape implements the PartialOrd trait
    #[inline]
    pub const fn is_partial_ord(&self) -> bool {
        self.is(Characteristic::PartialOrd)
    }

    /// Check if this shape implements the Ord trait
    #[inline]
    pub const fn is_ord(&self) -> bool {
        self.is(Characteristic::Ord)
    }

    /// Check if this shape implements the Hash trait
    #[inline]
    pub const fn is_hash(&self) -> bool {
        self.is(Characteristic::Hash)
    }

    /// Check if this shape implements the Default trait
    #[inline]
    pub const fn is_default(&self) -> bool {
        self.is(Characteristic::Default)
    }

    /// Check if this shape implements the FromStr trait
    #[inline]
    pub const fn is_from_str(&self) -> bool {
        self.is(Characteristic::FromStr)
    }

    /// Writes the name of this type to the given formatter.
    ///
    /// If the type has a custom type_name function, it will be used.
    /// Otherwise, falls back to the type_identifier.
    #[inline]
    pub fn write_type_name(
        &'static self,
        f: &mut fmt::Formatter<'_>,
        opts: TypeNameOpts,
    ) -> fmt::Result {
        if let Some(type_name_fn) = self.type_name {
            type_name_fn(self, f, opts)
        } else {
            write!(f, "{}", self.type_identifier)
        }
    }

    /// Returns a wrapper that implements `Display` for the full type name
    /// including generic parameters.
    ///
    /// # Example
    /// ```
    /// extern crate alloc;
    /// use facet_core::Facet;
    /// use alloc::vec::Vec;
    ///
    /// let shape = <Vec<u32>>::SHAPE;
    /// assert_eq!(format!("{}", shape.type_name()), "Vec<u32>");
    /// ```
    #[inline]
    pub const fn type_name(&'static self) -> TypeNameDisplay {
        TypeNameDisplay(self)
    }
}

/// A wrapper around `&'static Shape` that implements `Display` using the
/// full type name (including generic parameters).
#[derive(Clone, Copy)]
pub struct TypeNameDisplay(&'static Shape);

impl fmt::Display for TypeNameDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write_type_name(f, TypeNameOpts::default())
    }
}

impl fmt::Debug for TypeNameDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
