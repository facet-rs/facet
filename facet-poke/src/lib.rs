#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

//! Allows poking (writing to) shapes

use core::alloc::Layout;

pub use facet_peek::*;

use facet_core::{Def, Facet, OpaqueUninit, Shape};

mod value;
pub use value::*;

mod list;
pub use list::*;

mod map;
pub use map::*;

mod struct_;
pub use struct_::*;

mod enum_;
pub use enum_::*;

/// Allows writing values of different kinds.
#[non_exhaustive]
pub enum Poke<'mem> {
    /// A scalar value. See [`PokeValue`].
    Scalar(PokeValue<'mem>),
    /// A list (array/vec/etc). See [`PokeList`].
    List(PokeListUninit<'mem>),
    /// A map (HashMap/BTreeMap/etc). See [`PokeMap`].
    Map(PokeMapUninit<'mem>),
    /// A struct, tuple struct, or tuple. See [`PokeStruct`].
    Struct(PokeStruct<'mem>),
    /// An enum variant. See [`PokeEnum`].
    Enum(PokeEnumNoVariant<'mem>),
}

/// Ensures a value is dropped when the guard is dropped.
pub struct Guard {
    ptr: *mut u8,
    layout: Layout,
    shape: &'static Shape,
}

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

impl<'mem> Poke<'mem> {
    /// Allocates a new poke of a type that implements facet
    pub fn alloc<S: Facet>() -> (Self, Guard) {
        let data = S::SHAPE.allocate();
        let layout = Layout::new::<S>();
        let guard = Guard {
            ptr: data.as_mut_bytes(),
            layout,
            shape: S::SHAPE,
        };
        let poke = unsafe { Self::unchecked_new(data, S::SHAPE) };
        (poke, guard)
    }

    /// Allocates a new poke from a given shape
    pub fn alloc_shape(shape: &'static Shape) -> (Self, Guard) {
        let data = shape.allocate();
        let layout = shape.layout;
        let guard = Guard {
            ptr: data.as_mut_bytes(),
            layout,
            shape,
        };
        let poke = unsafe { Self::unchecked_new(data, shape) };
        (poke, guard)
    }

    /// Creates a new peek, for easy manipulation of some opaque data.
    ///
    /// # Safety
    ///
    /// `data` must be initialized and well-aligned, and point to a value
    /// of the type described by `shape`.
    pub unsafe fn unchecked_new(data: OpaqueUninit<'mem>, shape: &'static Shape) -> Self {
        match shape.def {
            Def::Struct(struct_def) => {
                Poke::Struct(unsafe { PokeStruct::new(data, shape, struct_def) })
            }
            Def::Map(map_def) => {
                let pmu = unsafe { PokeMapUninit::new(data, shape, map_def) };
                Poke::Map(pmu)
            }
            Def::List(list_def) => {
                let plu = unsafe { PokeListUninit::new(data, shape, list_def) };
                Poke::List(plu)
            }
            Def::Scalar { .. } => Poke::Scalar(unsafe { PokeValue::new(data, shape) }),
            Def::Enum(enum_def) => {
                Poke::Enum(unsafe { PokeEnumNoVariant::new(data, shape, enum_def) })
            }
            _ => todo!("unsupported def: {:?}", shape.def),
        }
    }

    /// Converts this Poke into a PokeStruct, panicking if it's not a Struct variant
    pub fn into_struct(self) -> PokeStruct<'mem> {
        match self {
            Poke::Struct(s) => s,
            _ => panic!("expected Struct variant"),
        }
    }

    /// Converts this Poke into a PokeList, panicking if it's not a List variant
    pub fn into_list(self) -> PokeListUninit<'mem> {
        match self {
            Poke::List(l) => l,
            _ => panic!("expected List variant"),
        }
    }

    /// Converts this Poke into a PokeMap, panicking if it's not a Map variant
    pub fn into_map(self) -> PokeMapUninit<'mem> {
        match self {
            Poke::Map(m) => m,
            _ => panic!("expected Map variant"),
        }
    }

    /// Converts this Poke into a PokeValue, panicking if it's not a Scalar variant
    pub fn into_scalar(self) -> PokeValue<'mem> {
        match self {
            Poke::Scalar(s) => s,
            _ => panic!("expected Scalar variant"),
        }
    }

    /// Converts this Poke into a PokeEnum, panicking if it's not an Enum variant
    pub fn into_enum(self) -> PokeEnumNoVariant<'mem> {
        match self {
            Poke::Enum(e) => e,
            _ => panic!("expected Enum variant"),
        }
    }

    /// Converts into a value, so we can manipulate it
    #[inline(always)]
    pub fn into_value(self) -> PokeValue<'mem> {
        match self {
            Poke::Scalar(s) => s.into_value(),
            Poke::List(l) => l.into_value(),
            Poke::Map(m) => m.into_value(),
            Poke::Struct(s) => s.into_value(),
            Poke::Enum(e) => e.into_value(),
        }
    }

    /// Get the shape of this Poke.
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        match self {
            Poke::Scalar(poke_value) => poke_value.shape(),
            Poke::List(poke_list_uninit) => poke_list_uninit.shape(),
            Poke::Map(poke_map_uninit) => poke_map_uninit.shape(),
            Poke::Struct(poke_struct) => poke_struct.shape(),
            Poke::Enum(poke_enum_no_variant) => poke_enum_no_variant.shape(),
        }
    }
}

/// Keeps track of which fields were initialized, up to 64 fields
#[derive(Clone, Copy, Default)]
pub struct ISet(u64);

impl ISet {
    /// Sets the bit at the given index.
    pub fn set(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 |= 1 << index;
    }

    /// Unsets the bit at the given index.
    pub fn unset(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 &= !(1 << index);
    }

    /// Checks if the bit at the given index is set.
    pub fn has(&self, index: usize) -> bool {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        (self.0 & (1 << index)) != 0
    }

    /// Checks if all bits up to the given count are set.
    pub fn all_set(&self, count: usize) -> bool {
        if count > 64 {
            panic!("ISet can only track up to 64 fields. Count {count} is out of bounds.");
        }
        let mask = (1 << count) - 1;
        self.0 & mask == mask
    }
}
