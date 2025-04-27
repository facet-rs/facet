use super::*;

mod array;
pub use array::*;

mod slice;
pub use slice::*;

mod list;
pub use list::*;

mod map;
pub use map::*;

mod option;
pub use option::*;

mod smartptr;
pub use smartptr::*;

mod function;
pub use function::*;

mod scalar;
pub use scalar::*;

/// The semantic definition of a shape: is it more like a scalar, a map, a list?
#[derive(Clone, Copy, Debug)]
#[repr(C)]
#[non_exhaustive]
// this enum is only ever going to be owned in static space,
// right?
#[expect(clippy::large_enum_variant)]
pub enum Def {
    /// Undefined - you can interact with the type through [`Type`] and [`ValueVTable`].
    Undefined,

    /// Scalar — those don't have a def, they're not composed of other things.
    /// You can interact with them through [`ValueVTable`].
    ///
    /// e.g. `u32`, `String`, `bool`, `SocketAddr`, etc.
    Scalar(ScalarDef),

    /// Map — keys are dynamic (and strings, sorry), values are homogeneous
    ///
    /// e.g. `Map<String, T>`
    Map(MapDef),

    /// Ordered list of heterogenous values, variable size
    ///
    /// e.g. `Vec<T>`
    List(ListDef),

    /// Fixed-size array of heterogeneous values, fixed size
    ///
    /// e.g. `[T; 3]`
    Array(ArrayDef),

    /// Slice - a reference to a contiguous sequence of elements
    ///
    /// e.g. `[T]`
    Slice(SliceDef),

    /// Option
    ///
    /// e.g. `Option<T>`
    Option(OptionDef),

    /// Smart pointers, like `Arc<T>`, `Rc<T>`, etc.
    SmartPointer(SmartPointerDef),
}

#[expect(clippy::result_large_err, reason = "See comment of expect above Def")]
impl Def {
    /// Returns the `ScalarDef` wrapped in an `Ok` if this is a [`Def::Scalar`].
    pub fn into_scalar(self) -> Result<ScalarDef, Self> {
        match self {
            Self::Scalar(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `MapDef` wrapped in an `Ok` if this is a [`Def::Map`].
    pub fn into_map(self) -> Result<MapDef, Self> {
        match self {
            Self::Map(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `ListDef` wrapped in an `Ok` if this is a [`Def::List`].
    pub fn into_list(self) -> Result<ListDef, Self> {
        match self {
            Self::List(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `ArrayDef` wrapped in an `Ok` if this is a [`Def::Array`].
    pub fn into_array(self) -> Result<ArrayDef, Self> {
        match self {
            Self::Array(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `SliceDef` wrapped in an `Ok` if this is a [`Def::Slice`].
    pub fn into_slice(self) -> Result<SliceDef, Self> {
        match self {
            Self::Slice(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `OptionDef` wrapped in an `Ok` if this is a [`Def::Option`].
    pub fn into_option(self) -> Result<OptionDef, Self> {
        match self {
            Self::Option(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `SmartPointerDef` wrapped in an `Ok` if this is a [`Def::SmartPointer`].
    pub fn into_smart_pointer(self) -> Result<SmartPointerDef, Self> {
        match self {
            Self::SmartPointer(def) => Ok(def),
            _ => Err(self),
        }
    }
}
