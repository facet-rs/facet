use core::marker::PhantomData;

use facet_core::{Def, Facet, PtrConst, PtrMut, Shape, Type, UserType};

use crate::ReflectError;

use super::PokeStruct;

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

    /// Invariant over 'facet (same reasoning as Peek)
    /// Covariant over 'mem but with mutable access
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
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Returns a const pointer to the underlying data.
    #[inline(always)]
    pub fn data(&self) -> PtrConst {
        self.data.as_const()
    }

    /// Returns a mutable pointer to the underlying data.
    #[inline(always)]
    pub fn data_mut(&mut self) -> PtrMut {
        self.data
    }

    /// Returns true if this value is a struct.
    #[inline]
    pub fn is_struct(&self) -> bool {
        matches!(self.shape.ty, Type::User(UserType::Struct(_)))
    }

    /// Returns true if this value is an enum.
    #[inline]
    pub fn is_enum(&self) -> bool {
        matches!(self.shape.ty, Type::User(UserType::Enum(_)))
    }

    /// Returns true if this value is a scalar (primitive type).
    #[inline]
    pub fn is_scalar(&self) -> bool {
        matches!(self.shape.def, Def::Scalar)
    }

    /// Converts this into a `PokeStruct` if the value is a struct.
    pub fn into_struct(self) -> Result<PokeStruct<'mem, 'facet>, ReflectError> {
        match self.shape.ty {
            Type::User(UserType::Struct(struct_type)) => Ok(PokeStruct {
                value: self,
                ty: struct_type,
            }),
            _ => Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: self.shape,
            }),
        }
    }

    /// Gets a reference to the underlying value.
    ///
    /// Returns an error if the shape doesn't match `T`.
    pub fn get<T: Facet<'facet>>(&self) -> Result<&T, ReflectError> {
        if self.shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
        }
        Ok(unsafe { self.data.as_const().get::<T>() })
    }

    /// Gets a mutable reference to the underlying value.
    ///
    /// Returns an error if the shape doesn't match `T`.
    pub fn get_mut<T: Facet<'facet>>(&mut self) -> Result<&mut T, ReflectError> {
        if self.shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
        }
        Ok(unsafe { self.data.as_mut::<T>() })
    }

    /// Sets the value to a new value.
    ///
    /// This replaces the entire value. The new value must have the same shape.
    pub fn set<T: Facet<'facet>>(&mut self, value: T) -> Result<(), ReflectError> {
        if self.shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
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
        assert!(matches!(result, Err(ReflectError::WrongShape { .. })));
    }

    #[test]
    fn poke_set_wrong_type_fails() {
        let mut x: i32 = 42;
        let mut poke = Poke::new(&mut x);

        let result = poke.set(42u32);
        assert!(matches!(result, Err(ReflectError::WrongShape { .. })));
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
