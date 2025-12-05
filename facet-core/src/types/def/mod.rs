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

mod result;
pub use result::*;

mod pointer;
pub use pointer::*;

mod function;
pub use function::*;

mod ndarray;
pub use ndarray::*;

mod dynamic_value;
pub use dynamic_value::*;

/// The semantic definition of a shape: is it more like a scalar, a map, a list?
#[derive(Clone, Copy)]
#[repr(C)]
// this enum is only ever going to be owned in static space,
// right?
#[non_exhaustive]
pub enum Def {
    /// Undefined - you can interact with the type through [`Type`] and [`ValueVTable`].
    Undefined,

    /// Scalar — those don't have a def, they're not composed of other things.
    /// You can interact with them through [`ValueVTable`].
    ///
    /// e.g. `u32`, `String`, `bool`, `SocketAddr`, etc.
    Scalar,

    /// Map — keys are dynamic (and strings, sorry), values are homogeneous
    ///
    /// e.g. `HashMap<String, T>`
    Map(MapDef),

    /// Unique set of homogenous values
    ///
    /// e.g. `HashSet<T>`
    Set(SetDef),

    /// Ordered list of homogenous values, variable size
    ///
    /// e.g. `Vec<T>`
    List(ListDef),

    /// Fixed-size array of homogeneous values, fixed size
    ///
    /// e.g. `[T; 3]`
    Array(ArrayDef),

    /// n-dimensional array of homogeneous values, fixed size
    ///
    /// e.g. `Vector<T>, Matrix<T>, Tensor<T>`
    NdArray(NdArrayDef),

    /// Slice - a reference to a contiguous sequence of elements
    ///
    /// e.g. `[T]`
    Slice(SliceDef),

    /// Option
    ///
    /// e.g. `Option<T>`
    Option(OptionDef),

    /// Result
    ///
    /// e.g. `Result<T, E>`
    Result(ResultDef),

    /// Pointer types like `Arc<T>`, `Rc<T>`, etc.
    Pointer(PointerDef),

    /// Dynamic value that can hold any type at runtime.
    ///
    /// e.g. `facet_value::Value`, `serde_json::Value`
    DynamicValue(DynamicValueDef),
}

impl core::fmt::Debug for Def {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Def::Undefined => write!(f, "Undefined"),
            Def::Scalar => {
                write!(f, "Scalar")
            }
            Def::Map(map_def) => write!(f, "Map<{}>", map_def.v),
            Def::Set(set_def) => write!(f, "Set<{}>", set_def.t),
            Def::List(list_def) => write!(f, "List<{}>", list_def.t),
            Def::NdArray(list_def) => write!(f, "NdArray<{}>", list_def.t),
            Def::Array(array_def) => write!(f, "Array<{}; {}>", array_def.t, array_def.n),
            Def::Slice(slice_def) => write!(f, "Slice<{}>", slice_def.t),
            Def::Option(option_def) => write!(f, "Option<{}>", option_def.t),
            Def::Result(result_def) => write!(f, "Result<{}, {}>", result_def.t, result_def.e),
            Def::Pointer(smart_ptr_def) => {
                if let Some(pointee) = smart_ptr_def.pointee {
                    write!(f, "SmartPointer<{pointee}>")
                } else {
                    write!(f, "SmartPointer<opaque>")
                }
            }
            Def::DynamicValue(_) => write!(f, "DynamicValue"),
        }
    }
}

impl Def {
    /// Returns the `ScalarDef` wrapped in an `Ok` if this is a [`Def::Scalar`].
    pub fn into_scalar(self) -> Result<(), Self> {
        match self {
            Self::Scalar => Ok(()),
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

    /// Returns the `SetDef` wrapped in an `Ok` if this is a [`Def::Set`].
    pub fn into_set(self) -> Result<SetDef, Self> {
        match self {
            Self::Set(def) => Ok(def),
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
    /// Returns the `NdArrayDef` wrapped in an `Ok` if this is a [`Def::NdArray`].
    pub fn into_ndarray(self) -> Result<NdArrayDef, Self> {
        match self {
            Self::NdArray(def) => Ok(def),
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
    /// Returns the `ResultDef` wrapped in an `Ok` if this is a [`Def::Result`].
    pub fn into_result(self) -> Result<ResultDef, Self> {
        match self {
            Self::Result(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `PointerDef` wrapped in an `Ok` if this is a [`Def::Pointer`].
    pub fn into_pointer(self) -> Result<PointerDef, Self> {
        match self {
            Self::Pointer(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the `DynamicValueDef` wrapped in an `Ok` if this is a [`Def::DynamicValue`].
    pub fn into_dynamic_value(self) -> Result<DynamicValueDef, Self> {
        match self {
            Self::DynamicValue(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the default `Type` for this `Def`.
    ///
    /// This is used by `ShapeBuilder` to infer the `ty` field from `def`.
    /// For most `Def` variants, this returns `Type::User(UserType::Opaque)`.
    /// Array and Slice have corresponding `Type::Sequence` variants.
    pub const fn default_type(&self) -> Type {
        match self {
            Self::Array(arr) => {
                Type::Sequence(SequenceType::Array(ArrayType { t: arr.t, n: arr.n }))
            }
            Self::Slice(slice) => Type::Sequence(SequenceType::Slice(SliceType { t: slice.t })),
            _ => Type::User(UserType::Opaque),
        }
    }
}
