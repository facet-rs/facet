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
    pub fn all(self, shapes: &[&Shape]) -> bool {
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
    pub fn any(self, shapes: &[&Shape]) -> bool {
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
    pub fn none(self, shapes: &[&Shape]) -> bool {
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
    pub fn all_default(shapes: &[&Shape]) -> bool {
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
    pub fn all_partial_eq(shapes: &[&Shape]) -> bool {
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
    pub fn all_partial_ord(shapes: &[&Shape]) -> bool {
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
    pub fn all_ord(shapes: &[&Shape]) -> bool {
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
    pub fn all_hash(shapes: &[&Shape]) -> bool {
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

impl<'shape> Shape<'shape> {
    /// Checks if a shape has the given characteristic.
    pub fn is(&self, characteristic: Characteristic) -> bool {
        match characteristic {
            // Marker traits
            Characteristic::Send => self.vtable.marker_traits().contains(MarkerTraits::SEND),
            Characteristic::Sync => self.vtable.marker_traits().contains(MarkerTraits::SYNC),
            Characteristic::Copy => self.vtable.marker_traits().contains(MarkerTraits::COPY),
            Characteristic::Eq => self.vtable.marker_traits().contains(MarkerTraits::EQ),
            Characteristic::Unpin => self.vtable.marker_traits().contains(MarkerTraits::UNPIN),

            // Functionality traits
            Characteristic::Clone => self.vtable.has_clone_into(),
            Characteristic::Display => self.vtable.has_display(),
            Characteristic::Debug => self.vtable.has_debug(),
            Characteristic::PartialEq => self.vtable.has_partial_eq(),
            Characteristic::PartialOrd => self.vtable.has_partial_ord(),
            Characteristic::Ord => self.vtable.has_ord(),
            Characteristic::Hash => self.vtable.has_hash(),
            Characteristic::Default => self.vtable.has_default_in_place(),
            Characteristic::FromStr => self.vtable.has_parse(),
        }
    }

    /// Check if this shape implements the Send trait
    pub fn is_send(&self) -> bool {
        self.is(Characteristic::Send)
    }

    /// Check if this shape implements the Sync trait
    pub fn is_sync(&self) -> bool {
        self.is(Characteristic::Sync)
    }

    /// Check if this shape implements the Copy trait
    pub fn is_copy(&self) -> bool {
        self.is(Characteristic::Copy)
    }

    /// Check if this shape implements the Eq trait
    pub fn is_eq(&self) -> bool {
        self.is(Characteristic::Eq)
    }

    /// Check if this shape implements the Clone trait
    pub fn is_clone(&self) -> bool {
        self.is(Characteristic::Clone)
    }

    /// Check if this shape implements the Display trait
    pub fn is_display(&self) -> bool {
        self.is(Characteristic::Display)
    }

    /// Check if this shape implements the Debug trait
    pub fn is_debug(&self) -> bool {
        self.is(Characteristic::Debug)
    }

    /// Check if this shape implements the PartialEq trait
    pub fn is_partial_eq(&self) -> bool {
        self.is(Characteristic::PartialEq)
    }

    /// Check if this shape implements the PartialOrd trait
    pub fn is_partial_ord(&self) -> bool {
        self.is(Characteristic::PartialOrd)
    }

    /// Check if this shape implements the Ord trait
    pub fn is_ord(&self) -> bool {
        self.is(Characteristic::Ord)
    }

    /// Check if this shape implements the Hash trait
    pub fn is_hash(&self) -> bool {
        self.is(Characteristic::Hash)
    }

    /// Check if this shape implements the Default trait
    pub fn is_default(&self) -> bool {
        self.is(Characteristic::Default)
    }

    /// Check if this shape implements the FromStr trait
    pub fn is_from_str(&self) -> bool {
        self.is(Characteristic::FromStr)
    }

    /// Writes the name of this type to the given formatter
    pub fn write_type_name(&self, f: &mut fmt::Formatter<'_>, opts: TypeNameOpts) -> fmt::Result {
        (self.vtable.type_name())(f, opts)
    }
}
