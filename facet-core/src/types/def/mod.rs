/// Generates a VTable struct and its builder.
///
/// # Example
///
/// ```ignore
/// vtable_def! {
///     /// Documentation for the vtable
///     #[derive(Clone, Copy, Debug)]
///     pub struct FooVTable + FooVTableBuilder {
///         /// Required field
///         pub field1: Field1Fn,
///         /// Optional field
///         pub field2: Option<Field2Fn>,
///     }
/// }
/// ```
///
/// This generates:
/// - The struct as written
/// - `FooVTable::builder() -> FooVTableBuilder`
/// - `FooVTableBuilder` with setter methods for each field
/// - `FooVTableBuilder::build() -> FooVTable`
///
/// Fields with type `Option<T>` are optional and don't panic if not set.
/// Required fields panic in `build()` if not set.
macro_rules! vtable_def {
    (
        $(#[$struct_meta:meta])*
        $vis:vis struct $name:ident + $builder:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident : $field_ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$struct_meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field: $field_ty,
            )*
        }

        impl $name {
            /// Creates a new builder for this vtable
            pub const fn builder() -> $builder {
                $builder {
                    $(
                        $field: None,
                    )*
                }
            }
        }

        /// Builder for the vtable
        #[derive(Clone, Copy, Debug)]
        $vis struct $builder {
            $(
                $field: Option<$field_ty>,
            )*
        }

        impl $builder {
            $(
                #[doc = concat!("Set the `", stringify!($field), "` field")]
                pub const fn $field(mut self, value: $field_ty) -> Self {
                    self.$field = Some(value);
                    self
                }
            )*

            /// Build the vtable
            ///
            /// # Panics
            ///
            /// Panics if any required field was not set.
            pub const fn build(self) -> $name {
                $name {
                    $(
                        $field: vtable_def!(@unwrap self.$field, stringify!($name), stringify!($field), $field_ty),
                    )*
                }
            }
        }
    };

    // Helper to unwrap Option fields - if the field type is Option<T>, pass through; otherwise panic if None
    (@unwrap $value:expr, $vtable:expr, $field:expr, Option<$inner:ty>) => {
        $value
    };
    (@unwrap $value:expr, $vtable:expr, $field:expr, $ty:ty) => {
        match $value {
            Some(v) => v,
            None => panic!(concat!($vtable, "::builder().", $field, "() must be called")),
        }
    };
}

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
    /// Undefined - you can interact with the type through [`Type`] and `VTableView`.
    Undefined,

    /// Scalar — those don't have a def, they're not composed of other things.
    /// You can interact with them through `VTableView`.
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
    /// Create a builder for a Map definition.
    ///
    /// # Example
    /// ```ignore
    /// let def = Def::map_builder(&MAP_VTABLE, K::SHAPE, V::SHAPE).build();
    /// ```
    #[inline]
    pub const fn map_builder(
        vtable: &'static MapVTable,
        k: &'static Shape,
        v: &'static Shape,
    ) -> DefMapBuilder {
        DefMapBuilder(MapDef::new(vtable, k, v))
    }

    /// Create a builder for a Set definition.
    ///
    /// # Example
    /// ```ignore
    /// let def = Def::set_builder(&SET_VTABLE, T::SHAPE).build();
    /// ```
    #[inline]
    pub const fn set_builder(vtable: &'static SetVTable, t: &'static Shape) -> DefSetBuilder {
        DefSetBuilder(SetDef::new(vtable, t))
    }

    /// Create a builder for a List definition.
    ///
    /// # Example
    /// ```ignore
    /// let def = Def::list_builder(&LIST_VTABLE, T::SHAPE).build();
    /// ```
    #[inline]
    pub const fn list_builder(vtable: &'static ListVTable, t: &'static Shape) -> DefListBuilder {
        DefListBuilder(ListDef::new(vtable, t))
    }

    /// Create a builder for an Array definition.
    ///
    /// # Example
    /// ```ignore
    /// let def = Def::array_builder(&ARRAY_VTABLE, T::SHAPE, 10).build();
    /// ```
    #[inline]
    pub const fn array_builder(
        vtable: &'static ArrayVTable,
        t: &'static Shape,
        n: usize,
    ) -> DefArrayBuilder {
        DefArrayBuilder(ArrayDef::new(vtable, t, n))
    }

    /// Create a builder for an Option definition.
    ///
    /// # Example
    /// ```ignore
    /// let def = Def::option_builder(&OPTION_VTABLE, T::SHAPE).build();
    /// ```
    #[inline]
    pub const fn option_builder(
        vtable: &'static OptionVTable,
        t: &'static Shape,
    ) -> DefOptionBuilder {
        DefOptionBuilder(OptionDef::new(vtable, t))
    }

    /// Create a builder for a Result definition.
    ///
    /// # Example
    /// ```ignore
    /// let def = Def::result_builder(&RESULT_VTABLE, T::SHAPE, E::SHAPE).build();
    /// ```
    #[inline]
    pub const fn result_builder(
        vtable: &'static ResultVTable,
        t: &'static Shape,
        e: &'static Shape,
    ) -> DefResultBuilder {
        DefResultBuilder(ResultDef::new(vtable, t, e))
    }

    /// Returns the `ScalarDef` wrapped in an `Ok` if this is a [`Def::Scalar`].
    pub const fn into_scalar(self) -> Result<(), Self> {
        match self {
            Self::Scalar => Ok(()),
            _ => Err(self),
        }
    }

    /// Returns the `MapDef` wrapped in an `Ok` if this is a [`Def::Map`].
    pub const fn into_map(self) -> Result<MapDef, Self> {
        match self {
            Self::Map(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the `SetDef` wrapped in an `Ok` if this is a [`Def::Set`].
    pub const fn into_set(self) -> Result<SetDef, Self> {
        match self {
            Self::Set(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the `ListDef` wrapped in an `Ok` if this is a [`Def::List`].
    pub const fn into_list(self) -> Result<ListDef, Self> {
        match self {
            Self::List(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `ArrayDef` wrapped in an `Ok` if this is a [`Def::Array`].
    pub const fn into_array(self) -> Result<ArrayDef, Self> {
        match self {
            Self::Array(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `NdArrayDef` wrapped in an `Ok` if this is a [`Def::NdArray`].
    pub const fn into_ndarray(self) -> Result<NdArrayDef, Self> {
        match self {
            Self::NdArray(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `SliceDef` wrapped in an `Ok` if this is a [`Def::Slice`].
    pub const fn into_slice(self) -> Result<SliceDef, Self> {
        match self {
            Self::Slice(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `OptionDef` wrapped in an `Ok` if this is a [`Def::Option`].
    pub const fn into_option(self) -> Result<OptionDef, Self> {
        match self {
            Self::Option(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `ResultDef` wrapped in an `Ok` if this is a [`Def::Result`].
    pub const fn into_result(self) -> Result<ResultDef, Self> {
        match self {
            Self::Result(def) => Ok(def),
            _ => Err(self),
        }
    }
    /// Returns the `PointerDef` wrapped in an `Ok` if this is a [`Def::Pointer`].
    pub const fn into_pointer(self) -> Result<PointerDef, Self> {
        match self {
            Self::Pointer(def) => Ok(def),
            _ => Err(self),
        }
    }

    /// Returns the `DynamicValueDef` wrapped in an `Ok` if this is a [`Def::DynamicValue`].
    pub const fn into_dynamic_value(self) -> Result<DynamicValueDef, Self> {
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

/// Builder that produces `Def::Map(...)`.
#[derive(Clone, Copy, Debug)]
pub struct DefMapBuilder(MapDef);

impl DefMapBuilder {
    /// Build the final `Def`.
    #[inline]
    pub const fn build(self) -> Def {
        Def::Map(self.0)
    }
}

/// Builder that produces `Def::Set(...)`.
#[derive(Clone, Copy, Debug)]
pub struct DefSetBuilder(SetDef);

impl DefSetBuilder {
    /// Build the final `Def`.
    #[inline]
    pub const fn build(self) -> Def {
        Def::Set(self.0)
    }
}

/// Builder that produces `Def::List(...)`.
#[derive(Clone, Copy, Debug)]
pub struct DefListBuilder(ListDef);

impl DefListBuilder {
    /// Build the final `Def`.
    #[inline]
    pub const fn build(self) -> Def {
        Def::List(self.0)
    }
}

/// Builder that produces `Def::Array(...)`.
#[derive(Clone, Copy, Debug)]
pub struct DefArrayBuilder(ArrayDef);

impl DefArrayBuilder {
    /// Build the final `Def`.
    #[inline]
    pub const fn build(self) -> Def {
        Def::Array(self.0)
    }
}

/// Builder that produces `Def::Option(...)`.
#[derive(Clone, Copy, Debug)]
pub struct DefOptionBuilder(OptionDef);

impl DefOptionBuilder {
    /// Build the final `Def`.
    #[inline]
    pub const fn build(self) -> Def {
        Def::Option(self.0)
    }
}

/// Builder that produces `Def::Result(...)`.
#[derive(Clone, Copy, Debug)]
pub struct DefResultBuilder(ResultDef);

impl DefResultBuilder {
    /// Build the final `Def`.
    #[inline]
    pub const fn build(self) -> Def {
        Def::Result(self.0)
    }
}
