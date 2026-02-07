use core::marker::PhantomData;

use facet_core::{Def, Facet, PtrConst, PtrMut, Shape, Type, UserType};
use facet_path::{Path, PathAccessError, PathStep};

use crate::{ReflectError, ReflectErrorKind, peek::VariantError};

use super::{PokeList, PokeStruct};

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
    fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        ReflectError::new(kind, Path::new(self.shape))
    }

    /// Returns a mutable pointer to the underlying data.
    #[inline(always)]
    pub const fn data_mut(&mut self) -> PtrMut {
        self.data
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

    /// Converts this `Poke` into a read-only `Peek`.
    #[inline]
    pub fn as_peek(&self) -> crate::Peek<'_, 'facet> {
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
    ///
    /// Steps like `MapKey`, `MapValue`, `OptionSome`, `Deref`, `Inner`, and
    /// `Proxy` are not supported for mutable access and will return
    /// [`PathAccessError::WrongStepKind`].
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
                let inner_ptr_const = unsafe { (option_def.vtable.get_value)(data.as_const()) }
                    .expect("is_some was true but get_value returned None");
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

        PathStep::MapKey(_)
        | PathStep::MapValue(_)
        | PathStep::Deref
        | PathStep::Inner
        | PathStep::Proxy => Err(PathAccessError::WrongStepKind {
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
}
