use super::*;

mod array;
pub use array::*;

mod slice;
pub use slice::*;

mod iter;
pub use iter::*;

mod list;
pub use list::*;

mod map;
pub use map::*;

mod set;
pub use set::*;

mod option;
pub use option::*;

mod smartptr;
pub use smartptr::*;

mod function;
pub use function::*;

mod scalar;
pub use scalar::*;

/// The semantic definition of a shape: is it more like a scalar, a map, a list?
#[derive(Clone, Copy)]
#[repr(C)]
#[non_exhaustive]
// this enum is only ever going to be owned in static space,
// right?
pub enum Def<'shape> {
    /// Undefined - you can interact with the type through [`Type`] and [`ValueVTable`].
    Undefined,

    /// Scalar — those don't have a def, they're not composed of other things.
    /// You can interact with them through [`ValueVTable`].
    ///
    /// e.g. `u32`, `String`, `bool`, `SocketAddr`, etc.
    Scalar(ScalarDef<'shape>),

    /// Map — keys are dynamic (and strings, sorry), values are homogeneous
    ///
    /// e.g. `HashMap<String, T>`
    Map(MapDef<'shape>),

    /// Unique set of homogenous values
    ///
    /// e.g. `HashSet<T>`
    Set(SetDef<'shape>),

    /// Ordered list of heterogenous values, variable size
    ///
    /// e.g. `Vec<T>`
    List(ListDef<'shape>),

    /// Fixed-size array of heterogeneous values, fixed size
    ///
    /// e.g. `[T; 3]`
    Array(ArrayDef<'shape>),

    /// Slice - a reference to a contiguous sequence of elements
    ///
    /// e.g. `[T]`
    Slice(SliceDef<'shape>),

    /// Option
    ///
    /// e.g. `Option<T>`
    Option(OptionDef<'shape>),

    /// Smart pointers, like `Arc<T>`, `Rc<T>`, etc.
    SmartPointer(SmartPointerDef<'shape>),
}

impl<'shape> core::fmt::Debug for Def<'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Def::Undefined => write!(f, "Undefined"),
            Def::Scalar(scalar_def) => {
                let affinity_name = match scalar_def.affinity {
                    crate::ScalarAffinity::Number(_) => "Number",
                    crate::ScalarAffinity::ComplexNumber(_) => "ComplexNumber",
                    crate::ScalarAffinity::String(_) => "String",
                    crate::ScalarAffinity::Boolean(_) => "Boolean",
                    crate::ScalarAffinity::Empty(_) => "Empty",
                    crate::ScalarAffinity::SocketAddr(_) => "SocketAddr",
                    crate::ScalarAffinity::IpAddr(_) => "IpAddr",
                    crate::ScalarAffinity::Url(_) => "Url",
                    crate::ScalarAffinity::UUID(_) => "UUID",
                    crate::ScalarAffinity::ULID(_) => "ULID",
                    crate::ScalarAffinity::Time(_) => "Time",
                    crate::ScalarAffinity::Opaque(_) => "Opaque",
                    crate::ScalarAffinity::Other(_) => "Other",
                    crate::ScalarAffinity::Char(_) => "Char",
                    crate::ScalarAffinity::Path(_) => "Path",
                };
                write!(f, "Scalar({})", affinity_name)
            }
            Def::Map(map_def) => write!(f, "Map<{}>", (map_def.v)()),
            Def::Set(set_def) => write!(f, "Set<{}>", (set_def.t)()),
            Def::List(list_def) => write!(f, "List<{}>", (list_def.t)()),
            Def::Array(array_def) => write!(f, "Array<{}; {}>", array_def.t, array_def.n),
            Def::Slice(slice_def) => write!(f, "Slice<{}>", slice_def.t),
            Def::Option(option_def) => write!(f, "Option<{}>", option_def.t),
            Def::SmartPointer(smart_ptr_def) => {
                if let Some(pointee) = smart_ptr_def.pointee {
                    write!(f, "SmartPointer<{}>", pointee())
                } else {
                    write!(f, "SmartPointer<opaque>")
                }
            }
        }
    }
}

impl<'shape> Def<'shape> {
    /// Returns the `ScalarDef` wrapped in an `Ok` if this is a [`Def::Scalar`].
    pub fn into_scalar(self) -> Result<ScalarDef<'shape>, Self> {
        match self {
            Self::Scalar(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the `MapDef` wrapped in an `Ok` if this is a [`Def::Map`].
    pub fn into_map(self) -> Result<MapDef<'shape>, Self> {
        match self {
            Self::Map(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the `SetDef` wrapped in an `Ok` if this is a [`Def::Set`].
    pub fn into_set(self) -> Result<SetDef<'shape>, Self> {
        match self {
            Self::Set(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the `ListDef` wrapped in an `Ok` if this is a [`Def::List`].
    pub fn into_list(self) -> Result<ListDef<'shape>, Self> {
        match self {
            Self::List(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `ArrayDef` wrapped in an `Ok` if this is a [`Def::Array`].
    pub fn into_array(self) -> Result<ArrayDef<'shape>, Self> {
        match self {
            Self::Array(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `SliceDef` wrapped in an `Ok` if this is a [`Def::Slice`].
    pub fn into_slice(self) -> Result<SliceDef<'shape>, Self> {
        match self {
            Self::Slice(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `OptionDef` wrapped in an `Ok` if this is a [`Def::Option`].
    pub fn into_option(self) -> Result<OptionDef<'shape>, Self> {
        match self {
            Self::Option(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `SmartPointerDef` wrapped in an `Ok` if this is a [`Def::SmartPointer`].
    pub fn into_smart_pointer(self) -> Result<SmartPointerDef<'shape>, Self> {
        match self {
            Self::SmartPointer(def) => Ok(def),
            _ => Err(self),
        }
    }
}
