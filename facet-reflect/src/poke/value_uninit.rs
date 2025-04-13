use core::{
    alloc::Layout,
    ops::{Deref, DerefMut},
};

use crate::{ReflectError, ScalarType};
use facet_core::{Def, Facet, Opaque, OpaqueConst, OpaqueUninit, Shape, TryFromError, ValueVTable};

use super::{
    ISet, PokeEnumNoVariant, PokeListUninit, PokeMapUninit, PokeSmartPointerUninit,
    PokeStructUninit, PokeValue,
};

/// Allows initializing/setting a value.
///
/// A safe wrapper around [`OpaqueUninit`]
#[derive(Debug)]
pub struct PokeValueUninit<'mem> {
    /// pointer to the value (not initialized, or partially initialized)
    pub(crate) data: OpaqueUninit<'mem>,

    /// shape of the value
    pub(crate) shape: &'static Shape,
}

impl<'mem> PokeValueUninit<'mem> {
    /// Allocates a new poke of a type that implements facet
    #[inline(always)]
    pub fn alloc<S: Facet>() -> HeapVal<Self> {
        Self::alloc_shape(S::SHAPE)
    }

    /// Allocates a new poke from a given shape
    #[inline(always)]
    pub fn alloc_shape(shape: &'static Shape) -> HeapVal<Self> {
        let data = shape.allocate();
        let poke = Self { data, shape };
        HeapVal::Full {
            inner: poke,
            guard: Guard {
                ptr: data.as_mut_bytes(),
                layout: shape.layout,
            },
        }
    }

    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Gets the vtable for the value
    #[inline(always)]
    fn vtable(&self) -> &'static ValueVTable {
        self.shape.vtable
    }

    /// Attempts to convert a value from another type into this one
    ///
    /// Returns `Ok(Opaque)` if the conversion was successful, `Err((Self, TryFromError))` otherwise.
    pub fn try_from<'src>(
        self,
        source: OpaqueConst<'src>,
    ) -> Result<PokeValue<'mem>, (Self, TryFromError)> {
        if let Some(try_from_fn) = self.vtable().try_from {
            match unsafe { try_from_fn(source, self.data) } {
                Ok(data) => Ok(PokeValue {
                    shape: self.shape,
                    data,
                }),
                Err(err) => Err((self, err)),
            }
        } else {
            let shape = self.shape;
            Err((self, TryFromError::Unimplemented(shape)))
        }
    }

    /// Attempts to parse a string into this value
    ///
    /// Returns `Ok(Opaque)` if parsing was successful, `Err(Self)` otherwise.
    pub fn parse(self, s: &str) -> Result<PokeValue<'mem>, Self> {
        if let Some(parse_fn) = self.vtable().parse {
            match unsafe { parse_fn(s, self.data) } {
                Ok(data) => Ok(PokeValue {
                    shape: self.shape,
                    data,
                }),
                Err(_) => Err(self),
            }
        } else {
            Err(self)
        }
    }

    // /// Attempts to clone `source` into this value
    // ///
    // /// Returns `Ok(PokeValue)` if cloning was successful, `Err(Self)` otherwise.
    // pub fn clone_from<'src>(self, source: Peek<'src>) -> Result<PokeValue<'mem>, Self> {
    //     if let Some(clone_fn) = self.vtable().clone_into {
    //         // Safe because the function will initialize our data if it returns Some
    //         Ok(PokeValue {
    //             data: unsafe { clone_fn(source.data(), self.data) },
    //             shape: self.shape,
    //         })
    //     } else {
    //         Err(self)
    //     }
    // }

    /// Tries to identify this value's type as a [`ScalarType`] — returns
    /// `None` if the value isn't a scalar, or is a scalar not listed in [`ScalarType`]
    pub fn scalar_type(&self) -> Option<ScalarType> {
        ScalarType::try_from_shape(self.shape)
    }

    /// Tries to identify this value as a struct
    pub fn into_struct(self) -> Result<PokeStructUninit<'mem>, ReflectError> {
        if let Def::Struct(def) = self.shape.def {
            Ok(PokeStructUninit {
                value: self,
                iset: ISet::default(),
                def,
            })
        } else {
            Err(ReflectError::WasNotA { name: "struct" })
        }
    }

    /// Tries to identify this value as an enum
    pub fn into_enum(self) -> Result<PokeEnumNoVariant<'mem>, ReflectError> {
        if let Def::Enum(def) = self.shape.def {
            Ok(PokeEnumNoVariant { value: self, def })
        } else {
            Err(ReflectError::WasNotA { name: "enum" })
        }
    }

    /// Tries to identify this value as a map
    pub fn into_map(self) -> Result<PokeMapUninit<'mem>, ReflectError> {
        if let Def::Map(def) = self.shape.def {
            Ok(PokeMapUninit { value: self, def })
        } else {
            Err(ReflectError::WasNotA { name: "map" })
        }
    }

    /// Tries to identify this value as a list
    pub fn into_list(self) -> Result<PokeListUninit<'mem>, ReflectError> {
        if let Def::List(def) = self.shape.def {
            Ok(PokeListUninit { value: self, def })
        } else {
            Err(ReflectError::WasNotA { name: "list" })
        }
    }

    /// Tries to identify this value as a smart pointer
    pub fn into_smart_pointer(self) -> Result<PokeSmartPointerUninit<'mem>, ReflectError> {
        if let Def::SmartPointer(def) = self.shape.def {
            Ok(PokeSmartPointerUninit { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                name: "smart pointer",
            })
        }
    }
}

struct Guard {
    ptr: *mut u8,
    layout: Layout,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.layout.size() != 0 {
            // SAFETY: `ptr` has been allocated via the global allocator with the given layout
            unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
        }
    }
}

/// Ensures a value is dropped when the guard is dropped.
pub enum HeapVal<T> {
    Full { inner: T, guard: Guard },
    Empty,
}

impl<T> Deref for HeapVal<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            HeapVal::Full { inner, .. } => inner,
            HeapVal::Empty => panic!("Attempted to dereference uninitialized HeapVal"),
        }
    }
}

impl<T> DerefMut for HeapVal<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            HeapVal::Full { inner, .. } => inner,
            HeapVal::Empty => {
                panic!("Attempted to mutably dereference uninitialized HeapVal")
            }
        }
    }
}

impl<T> HeapVal<T> {
    /// Maps the inner value with a closure.
    pub(crate) fn map<U>(mut self, f: impl FnOnce(T) -> U) -> HeapVal<U> {
        match std::mem::replace(&mut self, HeapVal::Empty) {
            HeapVal::Full { inner, guard } => {
                let new_inner = f(inner);
                HeapVal::Full {
                    inner: new_inner,
                    guard,
                }
            }
            HeapVal::Empty => panic!("cannot map empty heapval"),
        }
    }

    /// Maps the inner value with a closure that returns an option
    /// If it returns `None`, the heap value is deallocated.
    pub(crate) fn map_opt<U>(mut self, f: impl FnOnce(T) -> Option<U>) -> Option<HeapVal<U>> {
        match std::mem::replace(&mut self, HeapVal::Empty) {
            HeapVal::Full { inner, guard } => f(inner).map(|new_inner| HeapVal::Full {
                inner: new_inner,
                guard,
            }),
            HeapVal::Empty => panic!("cannot map empty heapval"),
        }
    }

    /// Maps the inner value with a closure that returns a result
    /// If it returns `Err`, the heap value is deallocated.
    pub(crate) fn map_res<U, E>(
        mut self,
        f: impl FnOnce(T) -> Result<U, E>,
    ) -> Result<HeapVal<U>, E> {
        match std::mem::replace(&mut self, HeapVal::Empty) {
            HeapVal::Full { inner, guard } => f(inner).map(|new_inner| HeapVal::Full {
                inner: new_inner,
                guard,
            }),
            HeapVal::Empty => panic!("cannot map empty heapval"),
        }
    }
}

impl<'mem> HeapVal<PokeValueUninit<'mem>> {
    /// Place a value in the space provided — this is equivalent to `*self = value`
    ///
    /// This function places a value of type T into the destination space,
    /// checking that T exactly matches the expected shape.
    pub fn put<T: Facet + 'mem>(self, value: T) -> Result<PokeValue<'mem>, ReflectError> {
        if !self.shape.is_type::<T>() {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
        }
        Ok(PokeValue {
            data: unsafe { self.data.put(value) },
            shape: self.shape,
        })
    }

    /// Attempts to set the value to its default
    ///
    /// Returns `Ok(PokeValue)` if setting to default was successful, `Err(Self)` otherwise.
    pub fn default_in_place(self) -> Result<HeapVal<PokeValue<'mem>>, ReflectError> {
        let Some(default_in_place_fn) = self.vtable().default_in_place else {
            return Err(ReflectError::NoDefault { shape: self.shape });
        };
        self.map_res(|this| {
            Ok(PokeValue {
                data: unsafe { default_in_place_fn(this.data) },
                shape: this.shape,
            })
        })
    }
}
