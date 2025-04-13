use core::fmt;

use super::{MarkerTraits, Shape, TypeNameOpts};

/// A characteristic a shape can have
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
#[non_exhaustive]
pub enum Characteristic {
    // Marker traits
    /// Implements Send
    Send,

    /// Implements Sync
    Sync,

    /// Implements Copy
    Copy,

    /// Implements Eq
    Eq,

    /// Implements Unpin
    Unpin,

    // Functionality traits
    /// Implements Clone
    Clone,

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
}

impl Characteristic {
    /// Checks if all shapes have the given characteristic.
    pub const fn all(self, shapes: &'static [&'static Shape]) -> bool {
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
    pub const fn any(self, shapes: &'static [&'static Shape]) -> bool {
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
    pub const fn none(self, shapes: &'static [&'static Shape]) -> bool {
        let mut i = 0;
        while i < shapes.len() {
            if shapes[i].is(self) {
                return false;
            }
            i += 1;
        }
        true
    }
}

impl Shape {
    /// Checks if a shape has the given characteristic.
    pub const fn is(&'static self, characteristic: Characteristic) -> bool {
        match characteristic {
            // Marker traits
            Characteristic::Send => self.vtable.marker_traits.contains(MarkerTraits::SEND),
            Characteristic::Sync => self.vtable.marker_traits.contains(MarkerTraits::SYNC),
            Characteristic::Copy => self.vtable.marker_traits.contains(MarkerTraits::COPY),
            Characteristic::Eq => self.vtable.marker_traits.contains(MarkerTraits::EQ),
            Characteristic::Unpin => self.vtable.marker_traits.contains(MarkerTraits::UNPIN),

            // Functionality traits
            Characteristic::Clone => self.vtable.clone_into.is_some(),
            Characteristic::Debug => self.vtable.debug.is_some(),
            Characteristic::PartialEq => self.vtable.eq.is_some(),
            Characteristic::PartialOrd => self.vtable.partial_ord.is_some(),
            Characteristic::Ord => self.vtable.ord.is_some(),
            Characteristic::Hash => self.vtable.hash.is_some(),
            Characteristic::Default => self.vtable.default_in_place.is_some(),
        }
    }

    /// Check if this shape implements the Send trait
    pub const fn is_send(&'static self) -> bool {
        self.is(Characteristic::Send)
    }

    /// Check if this shape implements the Sync trait
    pub const fn is_sync(&'static self) -> bool {
        self.is(Characteristic::Sync)
    }

    /// Check if this shape implements the Copy trait
    pub const fn is_copy(&'static self) -> bool {
        self.is(Characteristic::Copy)
    }

    /// Check if this shape implements the Eq trait
    pub const fn is_eq(&'static self) -> bool {
        self.is(Characteristic::Eq)
    }

    /// Check if this shape implements the Clone trait
    pub const fn is_clone(&'static self) -> bool {
        self.is(Characteristic::Clone)
    }

    /// Check if this shape implements the Debug trait
    pub const fn is_debug(&'static self) -> bool {
        self.is(Characteristic::Debug)
    }

    /// Check if this shape implements the PartialEq trait
    pub const fn is_partial_eq(&'static self) -> bool {
        self.is(Characteristic::PartialEq)
    }

    /// Check if this shape implements the PartialOrd trait
    pub const fn is_partial_ord(&'static self) -> bool {
        self.is(Characteristic::PartialOrd)
    }

    /// Check if this shape implements the Ord trait
    pub const fn is_ord(&'static self) -> bool {
        self.is(Characteristic::Ord)
    }

    /// Check if this shape implements the Hash trait
    pub const fn is_hash(&'static self) -> bool {
        self.is(Characteristic::Hash)
    }

    /// Check if this shape implements the Default trait
    pub const fn is_default(&'static self) -> bool {
        self.is(Characteristic::Default)
    }

    /// Writes the name of this type to the given formatter
    pub fn write_type_name(&self, f: &mut fmt::Formatter<'_>, opts: TypeNameOpts) -> fmt::Result {
        (self.vtable.type_name)(f, opts)
    }
}
