use core::marker::PhantomData;

use facet_core::{Def, Facet, PtrConst, PtrMut, Shape, StructKind, Type, UserType, Variance};
use facet_path::{Path, PathAccessError, PathStep};

use crate::{
    ReflectError, ReflectErrorKind,
    peek::{ListLikeDef, TupleType, VariantError},
};

use super::{
    PokeDynamicValue, PokeList, PokeListLike, PokeMap, PokeNdArray, PokeOption, PokePointer,
    PokeResult, PokeSet, PokeStruct, PokeTuple,
};

/// A mutable view into a value with runtime type information.
///
/// `Poke` provides reflection capabilities for mutating values at runtime.
/// It is the mutable counterpart to [`Peek`](crate::Peek).
///
/// # Wholesale Replacement vs Field Mutation
///
/// `Poke` can be created for any type. Replacing a value wholesale with [`Poke::set`]
/// is always safe - it just drops the old value and writes the new one.
///
/// However, mutating individual struct fields via [`PokeStruct::set_field`] requires
/// the struct to be marked as POD (`#[facet(pod)]`). This is because field mutation
/// could violate struct-level invariants.
///
/// # Lifetime Parameters
///
/// - `'mem`: The memory lifetime - how long the underlying data is valid
/// - `'facet`: The type's lifetime parameter (for types like `&'a str`)
///
/// # Example
///
/// ```ignore
/// // Wholesale replacement works on any type
/// let mut s = String::from("hello");
/// let mut poke = Poke::new(&mut s);
/// poke.set(String::from("world")).unwrap();
///
/// // Field mutation requires #[facet(pod)]
/// #[derive(Facet)]
/// #[facet(pod)]
/// struct Point { x: i32, y: i32 }
///
/// let mut point = Point { x: 1, y: 2 };
/// let mut poke = Poke::new(&mut point);
/// poke.into_struct().unwrap().set_field_by_name("x", 10i32).unwrap();
/// assert_eq!(point.x, 10);
/// ```
pub struct Poke<'mem, 'facet> {
    /// Underlying data (mutable)
    pub(crate) data: PtrMut,

    /// Shape of the value
    pub(crate) shape: &'static Shape,

    /// Invariant with respect to 'facet (same reasoning as Peek)
    /// Covariant with respect to 'mem but with mutable access
    #[allow(clippy::type_complexity)]
    _marker: PhantomData<(&'mem mut (), fn(&'facet ()) -> &'facet ())>,
}

impl<'mem, 'facet> Poke<'mem, 'facet> {
    /// Creates a mutable view over a `T` value.
    ///
    /// This always succeeds - wholesale replacement via [`Poke::set`] is safe for any type.
    /// The POD check happens when you try to mutate individual struct fields.
    pub fn new<T: Facet<'facet>>(t: &'mem mut T) -> Self {
        Self {
            data: PtrMut::new(t as *mut T as *mut u8),
            shape: T::SHAPE,
            _marker: PhantomData,
        }
    }

    /// Creates a mutable view from raw parts without any validation.
    ///
    /// # Safety
    ///
    /// - `data` must point to a valid, initialized value of the type described by `shape`
    /// - `data` must be valid for the lifetime `'mem`
    pub unsafe fn from_raw_parts(data: PtrMut, shape: &'static Shape) -> Self {
        Self {
            data,
            shape,
            _marker: PhantomData,
        }
    }

    /// Returns the shape of the value.
    #[inline(always)]
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Returns a const pointer to the underlying data.
    #[inline(always)]
    pub const fn data(&self) -> PtrConst {
        self.data.as_const()
    }

    /// Construct a ReflectError with this poke's shape as the root path.
    #[inline]
    pub(crate) fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        ReflectError::new(kind, Path::new(self.shape))
    }

    /// Returns a mutable pointer to the underlying data.
    #[inline(always)]
    pub const fn data_mut(&mut self) -> PtrMut {
        self.data
    }

    /// Returns the computed variance of the underlying type.
    #[inline]
    pub fn variance(&self) -> Variance {
        self.shape.computed_variance()
    }

    /// Attempts to reborrow this mutable view as an owned `Poke`.
    ///
    /// This is useful when only `&mut Poke` is available (e.g. through `DerefMut`)
    /// but an API requires ownership of `Poke`.
    ///
    /// Returns `Some` if the underlying type can shrink the `'facet` lifetime
    /// (covariant or bivariant), or `None` otherwise.
    #[inline]
    pub fn try_reborrow<'shorter>(&mut self) -> Option<Poke<'_, 'shorter>>
    where
        'facet: 'shorter,
    {
        if self.variance().can_shrink() {
            Some(Poke {
                data: self.data,
                shape: self.shape,
                _marker: PhantomData,
            })
        } else {
            None
        }
    }

    /// Returns true if this value is a struct.
    #[inline]
    pub const fn is_struct(&self) -> bool {
        matches!(self.shape.ty, Type::User(UserType::Struct(_)))
    }

    /// Returns true if this value is an enum.
    #[inline]
    pub const fn is_enum(&self) -> bool {
        matches!(self.shape.ty, Type::User(UserType::Enum(_)))
    }

    /// Returns true if this value is a scalar (primitive type).
    #[inline]
    pub const fn is_scalar(&self) -> bool {
        matches!(self.shape.def, Def::Scalar)
    }

    /// Returns true if this value is a tuple (anonymous `(A, B, ...)`).
    ///
    /// Note: tuple structs (named `struct Foo(A, B);`) report `false` here; they match
    /// [`is_struct`](Self::is_struct).
    #[inline]
    pub const fn is_tuple(&self) -> bool {
        matches!(
            self.shape.ty,
            Type::User(UserType::Struct(s)) if matches!(s.kind, StructKind::Tuple)
        )
    }

    /// Returns true if this value is a list (variable-length, homogeneous, e.g. `Vec<T>`).
    #[inline]
    pub const fn is_list(&self) -> bool {
        matches!(self.shape.def, Def::List(_))
    }

    /// Returns true if this value is a fixed-size array (e.g. `[T; N]`).
    #[inline]
    pub const fn is_array(&self) -> bool {
        matches!(self.shape.def, Def::Array(_))
    }

    /// Returns true if this value is a slice (e.g. `[T]`).
    #[inline]
    pub const fn is_slice(&self) -> bool {
        matches!(self.shape.def, Def::Slice(_))
    }

    /// Returns true if this value is a list, array, or slice
    /// (the set accepted by [`into_list_like`](Self::into_list_like)).
    #[inline]
    pub const fn is_list_like(&self) -> bool {
        matches!(self.shape.def, Def::List(_) | Def::Array(_) | Def::Slice(_))
    }

    /// Returns true if this value is a map.
    #[inline]
    pub const fn is_map(&self) -> bool {
        matches!(self.shape.def, Def::Map(_))
    }

    /// Returns true if this value is a set.
    #[inline]
    pub const fn is_set(&self) -> bool {
        matches!(self.shape.def, Def::Set(_))
    }

    /// Returns true if this value is an option.
    #[inline]
    pub const fn is_option(&self) -> bool {
        matches!(self.shape.def, Def::Option(_))
    }

    /// Returns true if this value is a result.
    #[inline]
    pub const fn is_result(&self) -> bool {
        matches!(self.shape.def, Def::Result(_))
    }

    /// Returns true if this value is a (smart) pointer.
    #[inline]
    pub const fn is_pointer(&self) -> bool {
        matches!(self.shape.def, Def::Pointer(_))
    }

    /// Returns true if this value is an n-dimensional array.
    #[inline]
    pub const fn is_ndarray(&self) -> bool {
        matches!(self.shape.def, Def::NdArray(_))
    }

    /// Returns true if this value is a dynamic value
    /// (e.g. `facet_value::Value` — runtime-kind-dispatched).
    #[inline]
    pub const fn is_dynamic_value(&self) -> bool {
        matches!(self.shape.def, Def::DynamicValue(_))
    }

    /// Converts this into a `PokeStruct` if the value is a struct.
    pub fn into_struct(self) -> Result<PokeStruct<'mem, 'facet>, ReflectError> {
        match self.shape.ty {
            Type::User(UserType::Struct(struct_type)) => Ok(PokeStruct {
                value: self,
                ty: struct_type,
            }),
            _ => Err(self.err(ReflectErrorKind::WasNotA {
                expected: "struct",
                actual: self.shape,
            })),
        }
    }

    /// Converts this into a `PokeEnum` if the value is an enum.
    pub fn into_enum(self) -> Result<super::PokeEnum<'mem, 'facet>, ReflectError> {
        match self.shape.ty {
            Type::User(UserType::Enum(enum_type)) => Ok(super::PokeEnum {
                value: self,
                ty: enum_type,
            }),
            _ => Err(self.err(ReflectErrorKind::WasNotA {
                expected: "enum",
                actual: self.shape,
            })),
        }
    }

    /// Converts this into a `PokeList` if the value is a list.
    #[inline]
    pub fn into_list(self) -> Result<PokeList<'mem, 'facet>, ReflectError> {
        if let Def::List(def) = self.shape.def {
            // SAFETY: The ListDef comes from self.shape.def, where self.shape is obtained
            // from a trusted source (either T::SHAPE from the Facet trait, or validated
            // through other safe constructors). The vtable is therefore trusted.
            return Ok(unsafe { PokeList::new(self, def) });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "list",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokeListLike` if the value is a list, array, or slice.
    #[inline]
    pub fn into_list_like(self) -> Result<PokeListLike<'mem, 'facet>, ReflectError> {
        match self.shape.def {
            // SAFETY: The defs come from self.shape.def, where self.shape is obtained from
            // a trusted source. The vtables are therefore trusted.
            Def::List(def) => Ok(unsafe { PokeListLike::new(self, ListLikeDef::List(def)) }),
            Def::Array(def) => Ok(unsafe { PokeListLike::new(self, ListLikeDef::Array(def)) }),
            Def::Slice(def) => Ok(unsafe { PokeListLike::new(self, ListLikeDef::Slice(def)) }),
            _ => Err(self.err(ReflectErrorKind::WasNotA {
                expected: "list, array or slice",
                actual: self.shape,
            })),
        }
    }

    /// Converts this into a `PokeMap` if the value is a map.
    #[inline]
    pub fn into_map(self) -> Result<PokeMap<'mem, 'facet>, ReflectError> {
        if let Def::Map(def) = self.shape.def {
            return Ok(unsafe { PokeMap::new(self, def) });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "map",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokeSet` if the value is a set.
    #[inline]
    pub fn into_set(self) -> Result<PokeSet<'mem, 'facet>, ReflectError> {
        if let Def::Set(def) = self.shape.def {
            return Ok(unsafe { PokeSet::new(self, def) });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "set",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokeOption` if the value is an option.
    #[inline]
    pub fn into_option(self) -> Result<PokeOption<'mem, 'facet>, ReflectError> {
        if let Def::Option(def) = self.shape.def {
            return Ok(unsafe { PokeOption::new(self, def) });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "option",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokeResult` if the value is a result.
    #[inline]
    pub fn into_result(self) -> Result<PokeResult<'mem, 'facet>, ReflectError> {
        if let Def::Result(def) = self.shape.def {
            return Ok(unsafe { PokeResult::new(self, def) });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "result",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokeTuple` if the value is a tuple (or tuple struct).
    #[inline]
    pub fn into_tuple(self) -> Result<PokeTuple<'mem, 'facet>, ReflectError> {
        if let Type::User(UserType::Struct(struct_type)) = self.shape.ty
            && struct_type.kind == StructKind::Tuple
        {
            return Ok(PokeTuple {
                value: self,
                ty: TupleType {
                    fields: struct_type.fields,
                },
            });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "tuple",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokePointer` if the value is a pointer.
    #[inline]
    pub fn into_pointer(self) -> Result<PokePointer<'mem, 'facet>, ReflectError> {
        if let Def::Pointer(def) = self.shape.def {
            return Ok(PokePointer { value: self, def });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "smart pointer",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokeNdArray` if the value is an n-dimensional array.
    #[inline]
    pub fn into_ndarray(self) -> Result<PokeNdArray<'mem, 'facet>, ReflectError> {
        if let Def::NdArray(def) = self.shape.def {
            return Ok(unsafe { PokeNdArray::new(self, def) });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "ndarray",
            actual: self.shape,
        }))
    }

    /// Converts this into a `PokeDynamicValue` if the value is a dynamic value.
    #[inline]
    pub fn into_dynamic_value(self) -> Result<PokeDynamicValue<'mem, 'facet>, ReflectError> {
        if let Def::DynamicValue(def) = self.shape.def {
            return Ok(PokeDynamicValue { value: self, def });
        }

        Err(self.err(ReflectErrorKind::WasNotA {
            expected: "dynamic value",
            actual: self.shape,
        }))
    }

    /// Gets a reference to the underlying value.
    ///
    /// Returns an error if the shape doesn't match `T`.
    pub fn get<T: Facet<'facet>>(&self) -> Result<&T, ReflectError> {
        if self.shape != T::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            }));
        }
        Ok(unsafe { self.data.as_const().get::<T>() })
    }

    /// Gets a mutable reference to the underlying value.
    ///
    /// Returns an error if the shape doesn't match `T`.
    pub fn get_mut<T: Facet<'facet>>(&mut self) -> Result<&mut T, ReflectError> {
        if self.shape != T::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            }));
        }
        Ok(unsafe { self.data.as_mut::<T>() })
    }

    /// Sets the value to a new value.
    ///
    /// This replaces the entire value. The new value must have the same shape.
    pub fn set<T: Facet<'facet>>(&mut self, value: T) -> Result<(), ReflectError> {
        if self.shape != T::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            }));
        }
        unsafe {
            // Drop the old value and write the new one
            self.shape.call_drop_in_place(self.data);
            core::ptr::write(self.data.as_mut_byte_ptr() as *mut T, value);
        }
        Ok(())
    }

    /// Borrows this `Poke` as a read-only `Peek`.
    #[inline]
    pub fn as_peek(&self) -> crate::Peek<'_, 'facet> {
        unsafe { crate::Peek::unchecked_new(self.data.as_const(), self.shape) }
    }

    /// Consumes this `Poke`, returning a read-only `Peek` with the same `'mem` lifetime.
    #[inline]
    pub fn into_peek(self) -> crate::Peek<'mem, 'facet> {
        unsafe { crate::Peek::unchecked_new(self.data.as_const(), self.shape) }
    }

    /// Navigate to a nested value by following a [`Path`], returning a mutable view.
    ///
    /// Each [`PathStep`] in the path is applied in order, descending into
    /// structs, enums, and lists. If any step cannot be applied, a
    /// [`PathAccessError`] is returned with the step index and context.
    ///
    /// # Supported steps
    ///
    /// - `Field` — struct fields and enum variant fields (after `Variant`)
    /// - `Variant` — verify enum variant matches, then allow `Field` access
    /// - `Index` — list/array element access
    /// - `OptionSome` — navigate into `Some(T)` or return `OptionIsNone`
    ///
    /// `MapKey`, `MapValue`, `Deref`, `Inner`, and `Proxy` are currently not
    /// supported for mutable access and return
    /// [`PathAccessError::MissingTarget`].
    ///
    /// # Errors
    ///
    /// Returns [`PathAccessError`] if:
    /// - The path's root shape doesn't match this value's shape
    /// - A step kind doesn't apply to the current shape
    /// - A field/list index is out of bounds
    /// - An enum variant doesn't match the runtime variant
    pub fn at_path_mut(self, path: &Path) -> Result<Poke<'mem, 'facet>, PathAccessError> {
        if self.shape != path.shape {
            return Err(PathAccessError::RootShapeMismatch {
                expected: path.shape,
                actual: self.shape,
            });
        }

        let mut data = self.data;
        let mut shape: &'static Shape = self.shape;

        for (step_index, step) in path.steps().iter().enumerate() {
            let (new_data, new_shape) = apply_step_mut(data, shape, *step, step_index)?;
            data = new_data;
            shape = new_shape;
        }

        Ok(unsafe { Poke::from_raw_parts(data, shape) })
    }
}

/// Apply a single [`PathStep`] to mutable data, returning the new pointer and shape.
///
/// This is a free function rather than a method to avoid lifetime issues with
/// chaining mutable borrows through `Poke`.
fn apply_step_mut(
    data: PtrMut,
    shape: &'static Shape,
    step: PathStep,
    step_index: usize,
) -> Result<(PtrMut, &'static Shape), PathAccessError> {
    match step {
        PathStep::Field(idx) => {
            let idx = idx as usize;
            match shape.ty {
                Type::User(UserType::Struct(sd)) => {
                    if idx >= sd.fields.len() {
                        return Err(PathAccessError::IndexOutOfBounds {
                            step,
                            step_index,
                            shape,
                            index: idx,
                            bound: sd.fields.len(),
                        });
                    }
                    let field = &sd.fields[idx];
                    let field_data = unsafe { data.field(field.offset) };
                    Ok((field_data, field.shape()))
                }
                Type::User(UserType::Enum(enum_type)) => {
                    // Determine active variant to get field layout
                    let variant_idx = variant_index_from_raw(data.as_const(), shape, enum_type)
                        .map_err(|_| PathAccessError::WrongStepKind {
                            step,
                            step_index,
                            shape,
                        })?;
                    let variant = &enum_type.variants[variant_idx];
                    if idx >= variant.data.fields.len() {
                        return Err(PathAccessError::IndexOutOfBounds {
                            step,
                            step_index,
                            shape,
                            index: idx,
                            bound: variant.data.fields.len(),
                        });
                    }
                    let field = &variant.data.fields[idx];
                    let field_data = unsafe { data.field(field.offset) };
                    Ok((field_data, field.shape()))
                }
                _ => Err(PathAccessError::WrongStepKind {
                    step,
                    step_index,
                    shape,
                }),
            }
        }

        PathStep::Variant(expected_idx) => {
            let expected_idx = expected_idx as usize;
            let enum_type = match shape.ty {
                Type::User(UserType::Enum(et)) => et,
                _ => {
                    return Err(PathAccessError::WrongStepKind {
                        step,
                        step_index,
                        shape,
                    });
                }
            };

            if expected_idx >= enum_type.variants.len() {
                return Err(PathAccessError::IndexOutOfBounds {
                    step,
                    step_index,
                    shape,
                    index: expected_idx,
                    bound: enum_type.variants.len(),
                });
            }

            let actual_idx =
                variant_index_from_raw(data.as_const(), shape, enum_type).map_err(|_| {
                    PathAccessError::WrongStepKind {
                        step,
                        step_index,
                        shape,
                    }
                })?;

            if actual_idx != expected_idx {
                return Err(PathAccessError::VariantMismatch {
                    step_index,
                    shape,
                    expected_variant: expected_idx,
                    actual_variant: actual_idx,
                });
            }

            // Stay at the same location — next Field step reads variant fields
            Ok((data, shape))
        }

        PathStep::Index(idx) => {
            let idx = idx as usize;
            match shape.def {
                Def::List(def) => {
                    let get_mut_fn = def.vtable.get_mut.ok_or(PathAccessError::WrongStepKind {
                        step,
                        step_index,
                        shape,
                    })?;
                    let len = unsafe { (def.vtable.len)(data.as_const()) };
                    let item = unsafe { get_mut_fn(data, idx, shape) };
                    item.map(|ptr| (ptr, def.t()))
                        .ok_or(PathAccessError::IndexOutOfBounds {
                            step,
                            step_index,
                            shape,
                            index: idx,
                            bound: len,
                        })
                }
                Def::Array(def) => {
                    // Arrays have a fixed element type and contiguous layout
                    let elem_shape = def.t();
                    let layout = elem_shape.layout.sized_layout().map_err(|_| {
                        PathAccessError::WrongStepKind {
                            step,
                            step_index,
                            shape,
                        }
                    })?;
                    let len = def.n;
                    if idx >= len {
                        return Err(PathAccessError::IndexOutOfBounds {
                            step,
                            step_index,
                            shape,
                            index: idx,
                            bound: len,
                        });
                    }
                    let elem_data = unsafe { data.field(layout.size() * idx) };
                    Ok((elem_data, elem_shape))
                }
                _ => Err(PathAccessError::WrongStepKind {
                    step,
                    step_index,
                    shape,
                }),
            }
        }

        PathStep::OptionSome => {
            if let Def::Option(option_def) = shape.def {
                // Check if the option is Some
                let is_some = unsafe { (option_def.vtable.is_some)(data.as_const()) };
                if !is_some {
                    return Err(PathAccessError::OptionIsNone { step_index, shape });
                }
                // Option is Some — get the inner value pointer.
                // Use get_value to find the PtrConst, then compute the offset
                // from the Option base to construct a PtrMut.
                let inner_raw_ptr = unsafe { (option_def.vtable.get_value)(data.as_const()) };
                assert!(
                    !inner_raw_ptr.is_null(),
                    "is_some was true but get_value returned null"
                );
                let inner_ptr_const = facet_core::PtrConst::new_sized(inner_raw_ptr);
                // Compute offset from option base to inner value
                let offset = unsafe {
                    inner_ptr_const
                        .as_byte_ptr()
                        .offset_from(data.as_const().as_byte_ptr())
                } as usize;
                let inner_data = unsafe { data.field(offset) };
                Ok((inner_data, option_def.t()))
            } else {
                Err(PathAccessError::WrongStepKind {
                    step,
                    step_index,
                    shape,
                })
            }
        }

        PathStep::MapKey(_) | PathStep::MapValue(_) => {
            if matches!(shape.def, Def::Map(_)) {
                Err(PathAccessError::MissingTarget {
                    step,
                    step_index,
                    shape,
                })
            } else {
                Err(PathAccessError::WrongStepKind {
                    step,
                    step_index,
                    shape,
                })
            }
        }

        PathStep::Deref => {
            if matches!(shape.def, Def::Pointer(_)) {
                Err(PathAccessError::MissingTarget {
                    step,
                    step_index,
                    shape,
                })
            } else {
                Err(PathAccessError::WrongStepKind {
                    step,
                    step_index,
                    shape,
                })
            }
        }

        PathStep::Inner => Err(PathAccessError::MissingTarget {
            step,
            step_index,
            shape,
        }),

        PathStep::Proxy => Err(PathAccessError::MissingTarget {
            step,
            step_index,
            shape,
        }),
    }
}

/// Determine the active variant index from raw data, replicating the logic
/// from `PeekEnum::variant_index` without constructing a `Peek`.
fn variant_index_from_raw(
    data: PtrConst,
    shape: &'static Shape,
    enum_type: facet_core::EnumType,
) -> Result<usize, VariantError> {
    use facet_core::EnumRepr;

    // For Option<T>, use the OptionVTable
    if let Def::Option(option_def) = shape.def {
        let is_some = unsafe { (option_def.vtable.is_some)(data) };
        return Ok(enum_type
            .variants
            .iter()
            .position(|variant| {
                let has_fields = !variant.data.fields.is_empty();
                has_fields == is_some
            })
            .expect("No variant found matching Option state"));
    }

    if enum_type.enum_repr == EnumRepr::RustNPO {
        let layout = shape
            .layout
            .sized_layout()
            .map_err(|_| VariantError::Unsized)?;
        let slice = unsafe { core::slice::from_raw_parts(data.as_byte_ptr(), layout.size()) };
        let all_zero = slice.iter().all(|v| *v == 0);

        Ok(enum_type
            .variants
            .iter()
            .position(|variant| {
                let mut max_offset = 0;
                for field in variant.data.fields {
                    let offset = field.offset
                        + field
                            .shape()
                            .layout
                            .sized_layout()
                            .map(|v| v.size())
                            .unwrap_or(0);
                    max_offset = core::cmp::max(max_offset, offset);
                }
                if all_zero {
                    max_offset == 0
                } else {
                    max_offset != 0
                }
            })
            .expect("No variant found with matching discriminant"))
    } else {
        let discriminant = match enum_type.enum_repr {
            EnumRepr::Rust => {
                panic!("cannot read discriminant from Rust enum with unspecified layout")
            }
            EnumRepr::RustNPO => 0,
            EnumRepr::U8 => unsafe { data.read::<u8>() as i64 },
            EnumRepr::U16 => unsafe { data.read::<u16>() as i64 },
            EnumRepr::U32 => unsafe { data.read::<u32>() as i64 },
            EnumRepr::U64 => unsafe { data.read::<u64>() as i64 },
            EnumRepr::USize => unsafe { data.read::<usize>() as i64 },
            EnumRepr::I8 => unsafe { data.read::<i8>() as i64 },
            EnumRepr::I16 => unsafe { data.read::<i16>() as i64 },
            EnumRepr::I32 => unsafe { data.read::<i32>() as i64 },
            EnumRepr::I64 => unsafe { data.read::<i64>() },
            EnumRepr::ISize => unsafe { data.read::<isize>() as i64 },
        };

        Ok(enum_type
            .variants
            .iter()
            .position(|variant| variant.discriminant == Some(discriminant))
            .expect("No variant found with matching discriminant"))
    }
}

impl core::fmt::Debug for Poke<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Poke<{}>", self.shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poke_primitive_get_set() {
        let mut x: i32 = 42;
        let mut poke = Poke::new(&mut x);

        assert_eq!(*poke.get::<i32>().unwrap(), 42);

        poke.set(100i32).unwrap();
        assert_eq!(x, 100);
    }

    #[test]
    fn poke_primitive_get_mut() {
        let mut x: i32 = 42;
        let mut poke = Poke::new(&mut x);

        *poke.get_mut::<i32>().unwrap() = 99;
        assert_eq!(x, 99);
    }

    #[test]
    fn poke_wrong_type_fails() {
        let mut x: i32 = 42;
        let poke = Poke::new(&mut x);

        let result = poke.get::<u32>();
        assert!(matches!(
            result,
            Err(ReflectError {
                kind: ReflectErrorKind::WrongShape { .. },
                ..
            })
        ));
    }

    #[test]
    fn poke_set_wrong_type_fails() {
        let mut x: i32 = 42;
        let mut poke = Poke::new(&mut x);

        let result = poke.set(42u32);
        assert!(matches!(
            result,
            Err(ReflectError {
                kind: ReflectErrorKind::WrongShape { .. },
                ..
            })
        ));
    }

    #[test]
    fn poke_string_drop_and_replace() {
        // Wholesale replacement works on any type, including String
        let mut s = String::from("hello");
        let mut poke = Poke::new(&mut s);

        poke.set(String::from("world")).unwrap();
        assert_eq!(s, "world");
    }

    #[test]
    fn poke_is_predicates() {
        let mut v: alloc::vec::Vec<i32> = alloc::vec![1, 2, 3];
        let poke = Poke::new(&mut v);
        assert!(poke.is_list());
        assert!(poke.is_list_like());
        assert!(!poke.is_map());
        assert!(!poke.is_set());
        assert!(!poke.is_option());
        assert!(!poke.is_result());
        assert!(!poke.is_tuple());
        assert!(!poke.is_scalar());

        let mut x: Option<i32> = Some(1);
        let poke = Poke::new(&mut x);
        assert!(poke.is_option());
        assert!(!poke.is_list());

        let mut r: Result<i32, i32> = Ok(1);
        let poke = Poke::new(&mut r);
        assert!(poke.is_result());

        let mut t: (i32, i32) = (1, 2);
        let poke = Poke::new(&mut t);
        assert!(poke.is_tuple());

        let mut n: i32 = 42;
        let poke = Poke::new(&mut n);
        assert!(poke.is_scalar());

        let mut a: [i32; 3] = [1, 2, 3];
        let poke = Poke::new(&mut a);
        assert!(poke.is_array());
        assert!(poke.is_list_like());
    }
}
