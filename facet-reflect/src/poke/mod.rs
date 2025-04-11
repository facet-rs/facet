extern crate alloc;

use core::alloc::Layout;

use facet_core::{Def, Facet, Opaque, OpaqueUninit, Shape};

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

mod option;
pub use option::*;

/// Allows initializing values of different kinds.
#[non_exhaustive]
pub enum PokeUninit<'mem> {
    /// A scalar value. See [`PokeValue`].
    Scalar(PokeValueUninit<'mem>),
    /// A list (array/vec/etc). See [`PokeList`].
    List(PokeListUninit<'mem>),
    /// A map (HashMap/BTreeMap/etc). See [`PokeMap`].
    Map(PokeMapUninit<'mem>),
    /// A struct, tuple struct, or tuple. See [`PokeStruct`].
    Struct(PokeStruct<'mem>),
    /// An enum variant. See [`PokeEnum`].
    Enum(PokeEnumNoVariant<'mem>),
    /// An option value. See [`PokeOption`].
    Option(PokeOptionUninit<'mem>),
}

/// Ensures a value is dropped when the guard is dropped.
pub struct Guard {
    ptr: *mut u8,
    layout: Layout,
    shape: &'static Shape,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.layout.size() == 0 {
            return;
        }
        // SAFETY: `ptr` has been allocated via the global allocator with the given layout
        unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
    }
}

impl<'mem> PokeUninit<'mem> {
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
                PokeUninit::Struct(unsafe { PokeStruct::new(data, shape, struct_def) })
            }
            Def::Map(map_def) => {
                let pmu = unsafe { PokeMapUninit::new(data, shape, map_def) };
                PokeUninit::Map(pmu)
            }
            Def::List(list_def) => {
                let plu = unsafe { PokeListUninit::new(data, shape, list_def) };
                PokeUninit::List(plu)
            }
            Def::Scalar { .. } => PokeUninit::Scalar(unsafe { PokeValueUninit::new(data, shape) }),
            Def::Enum(enum_def) => {
                PokeUninit::Enum(unsafe { PokeEnumNoVariant::new(data, shape, enum_def) })
            }
            Def::Option(option_def) => {
                let pou = unsafe { PokeOptionUninit::new(data, shape, option_def) };
                PokeUninit::Option(pou)
            }
            _ => todo!("unsupported def: {:?}", shape.def),
        }
    }

    /// Converts this Poke into a PokeStruct, panicking if it's not a Struct variant
    pub fn into_struct(self) -> PokeStruct<'mem> {
        match self {
            PokeUninit::Struct(s) => s,
            _ => panic!("expected Struct variant"),
        }
    }

    /// Converts this Poke into a PokeList, panicking if it's not a List variant
    pub fn into_list(self) -> PokeListUninit<'mem> {
        match self {
            PokeUninit::List(l) => l,
            _ => panic!("expected List variant"),
        }
    }

    /// Converts this Poke into a PokeMap, panicking if it's not a Map variant
    pub fn into_map(self) -> PokeMapUninit<'mem> {
        match self {
            PokeUninit::Map(m) => m,
            _ => panic!("expected Map variant"),
        }
    }

    /// Converts this Poke into a PokeValue, panicking if it's not a Scalar variant
    pub fn into_scalar(self) -> PokeValueUninit<'mem> {
        match self {
            PokeUninit::Scalar(s) => s,
            _ => panic!("expected Scalar variant"),
        }
    }

    /// Converts this Poke into a PokeEnum, panicking if it's not an Enum variant
    pub fn into_enum(self) -> PokeEnumNoVariant<'mem> {
        match self {
            PokeUninit::Enum(e) => e,
            _ => panic!("expected Enum variant"),
        }
    }

    /// Converts this Poke into a PokeOption, panicking if it's not an Option variant
    pub fn into_option(self) -> PokeOptionUninit<'mem> {
        match self {
            PokeUninit::Option(o) => o,
            _ => panic!("expected Option variant"),
        }
    }

    /// Converts into a value, so we can manipulate it
    #[inline(always)]
    pub fn into_value(self) -> PokeValueUninit<'mem> {
        match self {
            PokeUninit::Scalar(s) => s.into_value(),
            PokeUninit::List(l) => l.into_value(),
            PokeUninit::Map(m) => m.into_value(),
            PokeUninit::Struct(s) => s.into_value(),
            PokeUninit::Enum(e) => e.into_value(),
            PokeUninit::Option(o) => o.into_value(),
        }
    }

    /// Get the shape of this Poke.
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        match self {
            PokeUninit::Scalar(poke_value) => poke_value.shape(),
            PokeUninit::List(poke_list_uninit) => poke_list_uninit.shape(),
            PokeUninit::Map(poke_map_uninit) => poke_map_uninit.shape(),
            PokeUninit::Struct(poke_struct) => poke_struct.shape(),
            PokeUninit::Enum(poke_enum_no_variant) => poke_enum_no_variant.shape(),
            PokeUninit::Option(poke_option_uninit) => poke_option_uninit.shape(),
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

/// Allows manipulating already-initialized values of different kinds.
#[non_exhaustive]
pub enum Poke<'mem> {
    /// A scalar value. See [`PokeValue`].
    Scalar(PokeValue<'mem>),
    /// A list (array/vec/etc). See [`PokeList`].
    List(PokeList<'mem>),
    /// A map (HashMap/BTreeMap/etc). See [`PokeMap`].
    Map(PokeMap<'mem>),
    /// A struct, tuple struct, or tuple. See [`PokeStruct`].
    Struct(PokeStruct<'mem>),
    /// An enum variant. See [`PokeEnum`].
    Enum(PokeEnum<'mem>),
    /// An option value. See [`PokeOption`].
    Option(PokeOption<'mem>),
}

impl<'mem> Poke<'mem> {
    /// Creates a new peek, for easy manipulation of some opaque data.
    ///
    /// # Safety
    ///
    /// `data` must be initialized and well-aligned, and point to a value
    /// of the type described by `shape`.
    pub unsafe fn unchecked_new(data: Opaque<'mem>, shape: &'static Shape) -> Self {
        match shape.def {
            Def::Struct(struct_def) => Poke::Struct(unsafe {
                let mut ps =
                    PokeStruct::new(OpaqueUninit::new(data.as_mut_byte_ptr()), shape, struct_def);
                for (i, _f) in ps.def().fields.iter().enumerate() {
                    ps.mark_initialized(i);
                }
                ps
            }),
            Def::Map(map_def) => {
                let pm = unsafe { PokeMap::new(data, shape, map_def) };
                Poke::Map(pm)
            }
            Def::List(list_def) => {
                let pl = unsafe { PokeList::new(data, shape, list_def) };
                Poke::List(pl)
            }
            Def::Scalar { .. } => Poke::Scalar(unsafe { PokeValue::new(data, shape) }),
            Def::Enum(_enum_def) => {
                todo!("we need to get the active variant somehow")
            }
            Def::Option(option_def) => {
                let po = unsafe { PokeOption::new(data, shape, option_def) };
                Poke::Option(po)
            }
            _ => todo!("unsupported def: {:?}", shape.def),
        }
    }

    /// Borrows the value for a different kind of inspection.
    #[inline(always)]
    pub fn borrow<T: Facet>(data: &'mem mut T) -> Poke<'mem> {
        let shape = T::SHAPE;
        let data = Opaque::new(data);
        unsafe { Poke::unchecked_new(data, shape) }
    }

    /// Converts this Poke into a PokeValue, panicking if it's not a Scalar variant
    pub fn into_scalar(self) -> PokeValue<'mem> {
        match self {
            Poke::Scalar(s) => s,
            _ => panic!("expected Scalar variant"),
        }
    }

    /// Converts this Poke into a PokeList, panicking if it's not a List variant
    pub fn into_list(self) -> PokeList<'mem> {
        match self {
            Poke::List(l) => l,
            _ => panic!("expected List variant"),
        }
    }

    /// Converts this Poke into a PokeMap, panicking if it's not a Map variant
    pub fn into_map(self) -> PokeMap<'mem> {
        match self {
            Poke::Map(m) => m,
            _ => panic!("expected Map variant"),
        }
    }

    /// Converts this Poke into a PokeStruct, panicking if it's not a Struct variant
    pub fn into_struct(self) -> PokeStruct<'mem> {
        match self {
            Poke::Struct(s) => s,
            _ => panic!("expected Struct variant"),
        }
    }

    /// Converts this Poke into a PokeEnum, panicking if it's not an Enum variant
    pub fn into_enum(self) -> PokeEnum<'mem> {
        match self {
            Poke::Enum(e) => e,
            _ => panic!("expected Enum variant"),
        }
    }

    /// Converts this Poke into a PokeOption, panicking if it's not an Option variant
    pub fn into_option(self) -> PokeOption<'mem> {
        match self {
            Poke::Option(o) => o,
            _ => panic!("expected Option variant"),
        }
    }

    /// Get the shape of this Poke.
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        match self {
            Poke::Scalar(poke_value) => poke_value.shape(),
            Poke::List(poke_list) => poke_list.shape(),
            Poke::Map(poke_map) => poke_map.shape(),
            Poke::Struct(poke_struct) => poke_struct.shape(),
            Poke::Enum(poke_enum) => poke_enum.shape(),
            Poke::Option(poke_option) => poke_option.shape(),
        }
    }
}
