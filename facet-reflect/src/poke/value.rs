use crate::{ScalarType, peek::Peek};
use facet_core::{Facet, Opaque, OpaqueConst, OpaqueUninit, Shape, TryFromError, ValueVTable};

/// A strongly-typed value writer that ensures type safety at compile-time
pub struct TypedPokeValueUninit<'mem, T: Facet> {
    poke_value: PokeValueUninit<'mem>,
    _phantom: core::marker::PhantomData<T>,
}

impl<'mem, T: Facet> TypedPokeValueUninit<'mem, T> {
    /// Create a new TypedPokeValue from a PokeValue
    fn new(poke_value: PokeValueUninit<'mem>) -> Self {
        Self {
            poke_value,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Place a value of type T in the space provided
    pub fn put(self, value: T) -> PokeValue<'mem> {
        // We already verified the shape matches T when we created this TypedPokeValue
        let data = unsafe { self.poke_value.data.put(value) };
        unsafe { PokeValue::new(data, self.poke_value.shape) }
    }
}

/// Lets you write to a value (implements write-only [`ValueVTable`] proxies)
pub struct PokeValueUninit<'mem> {
    data: OpaqueUninit<'mem>,
    shape: &'static Shape,
}

impl core::fmt::Debug for PokeValueUninit<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeValue")
            .field("shape", &self.shape)
            .finish_non_exhaustive()
    }
}

impl<'mem> PokeValueUninit<'mem> {
    #[inline(always)]
    /// Coerce back into a `PokeValue`
    pub fn into_value(self) -> Self {
        self
    }

    /// Converts to a type-checked [`TypedPokeValue<T>`] if the shape matches type `T`
    ///
    /// Returns `None` if the shape doesn't match the type `T`.
    pub fn typed<T: Facet>(self) -> Result<TypedPokeValueUninit<'mem, T>, Self> {
        if self.shape.is_type::<T>() {
            Ok(TypedPokeValueUninit::new(self))
        } else {
            Err(self)
        }
    }

    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Creates a value write-proxy from its essential components
    ///
    /// # Safety
    ///
    /// The data buffer must match the size and alignment of the shape.
    pub(crate) unsafe fn new(data: OpaqueUninit<'mem>, shape: &'static Shape) -> Self {
        Self { data, shape }
    }

    /// Gets the vtable for the value
    #[inline(always)]
    fn vtable(&self) -> &'static ValueVTable {
        self.shape.vtable
    }

    /// Exposes the internal data buffer as a mutable reference
    ///
    /// # Safety
    ///
    /// The caller must ensure that they don't violate any invariants of the underlying type.
    pub unsafe fn data(&mut self) -> OpaqueUninit<'mem> {
        self.data
    }

    /// Attempts to convert a value from another type into this one
    ///
    /// Returns `Ok(Opaque)` if the conversion was successful, `Err((Self, TryFromError))` otherwise.
    pub fn try_from<'src>(
        self,
        source: OpaqueConst<'src>,
    ) -> Result<Opaque<'mem>, (Self, TryFromError)> {
        if let Some(try_from_fn) = self.vtable().try_from {
            match unsafe { try_from_fn(source, self.data) } {
                Ok(built_val) => Ok(built_val),
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
    pub fn parse(self, s: &str) -> Result<Opaque<'mem>, Self> {
        if let Some(parse_fn) = self.vtable().parse {
            match unsafe { parse_fn(s, self.data) } {
                Ok(parsed_val) => Ok(parsed_val),
                Err(_) => Err(self),
            }
        } else {
            Err(self)
        }
    }

    /// Place a value in the space provided. See also [`Self::typed`], which
    /// is panic-free.
    ///
    /// This function places a value of type T into the destination space,
    /// checking that T exactly matches the expected shape.
    pub fn put<'src, T>(self, value: T) -> Opaque<'mem>
    where
        T: Facet + 'src,
    {
        self.shape.assert_type::<T>();
        unsafe { self.data.put(value) }
    }

    /// Attempts to set the value to its default
    ///
    /// Returns `Ok(Opaque)` if setting to default was successful, `Err(Self)` otherwise.
    pub fn default_in_place(self) -> Result<Opaque<'mem>, Self> {
        if let Some(default_in_place_fn) = self.vtable().default_in_place {
            let default_val = unsafe { default_in_place_fn(self.data) };
            Ok(default_val)
        } else {
            Err(self)
        }
    }

    /// Attempts to clone `source` into this value
    ///
    /// Returns `Ok(Peek)` if cloning was successful, `Err(Self)` otherwise.
    pub fn clone_from<'src>(self, source: Peek<'src>) -> Result<Peek<'mem>, Self> {
        if let Some(clone_fn) = self.vtable().clone_into {
            let cloned_val = unsafe { clone_fn(source.data(), self.data) };
            // Safe because the function will initialize our data if it returns Some
            Ok(unsafe { Peek::unchecked_new(cloned_val.as_const(), self.shape) })
        } else {
            Err(self)
        }
    }

    /// Get the scalar type if set.
    pub fn scalar_type(&self) -> Option<ScalarType> {
        ScalarType::try_from_shape(self.shape)
    }
}

/// A strongly-typed value writer for initialized values that ensures type safety at compile-time
pub struct TypedPokeValue<'mem, T: Facet> {
    poke_value: PokeValue<'mem>,
    _phantom: core::marker::PhantomData<T>,
}

impl<'mem, T: Facet> TypedPokeValue<'mem, T> {
    /// Create a new TypedPokeValue from a PokeValue
    fn new(poke_value: PokeValue<'mem>) -> Self {
        Self {
            poke_value,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Replace the existing value with a new one of type T
    pub fn replace(self, value: T) -> Opaque<'mem> {
        // We already verified the shape matches T when we created this TypedPokeValue
        unsafe { self.poke_value.data.replace(value) }
    }
}

/// Lets you modify an initialized value (implements read-write [`ValueVTable`] proxies)
pub struct PokeValue<'mem> {
    data: Opaque<'mem>,
    shape: &'static Shape,
}

impl core::fmt::Debug for PokeValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeValue")
            .field("shape", &self.shape)
            .field("data", &self.as_peek())
            .finish()
    }
}

impl<'mem> PokeValue<'mem> {
    /// Creates a value write-proxy from its essential components
    ///
    /// # Safety
    ///
    /// The data must be a valid, initialized instance of the type described by shape.
    pub unsafe fn unchecked_new(data: Opaque<'mem>, shape: &'static Shape) -> Self {
        Self { data, shape }
    }

    #[inline(always)]
    /// Coerce back into a `PokeValue`
    pub fn into_value(self) -> Self {
        self
    }

    /// Converts to a type-checked [`TypedPokeValue<T>`] if the shape matches type `T`
    ///
    /// Returns `None` if the shape doesn't match the type `T`.
    pub fn typed<T: Facet>(self) -> Result<TypedPokeValue<'mem, T>, Self> {
        if self.shape.is_type::<T>() {
            Ok(TypedPokeValue::new(self))
        } else {
            Err(self)
        }
    }

    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Creates a value write-proxy from its essential components
    ///
    /// # Safety
    ///
    /// The data must be a valid, initialized instance of the type described by shape.
    pub(crate) unsafe fn new(data: Opaque<'mem>, shape: &'static Shape) -> Self {
        Self { data, shape }
    }

    /// Gets the vtable for the value
    #[inline(always)]
    fn vtable(&self) -> &'static ValueVTable {
        self.shape.vtable
    }

    /// Gets a read-only view of the value
    pub fn as_peek(&self) -> Peek<'_> {
        unsafe { Peek::unchecked_new(self.data.as_const(), self.shape) }
    }

    /// Exposes the internal data buffer as a mutable reference
    ///
    /// # Safety
    ///
    /// The caller must ensure that they don't violate any invariants of the underlying type.
    pub unsafe fn data(&mut self) -> Opaque<'mem> {
        self.data
    }

    /// Replace the current value with a new one of the same type
    ///
    /// This function replaces the existing value with a new one of type T,
    /// checking that T exactly matches the expected shape.
    pub fn replace<'src, T>(self, value: T) -> Opaque<'mem>
    where
        T: Facet + 'src,
    {
        self.shape.assert_type::<T>();
        unsafe { self.data.replace(value) }
    }

    /// Format the value using its Debug implementation
    pub fn debug_fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = self.vtable().debug {
            unsafe { debug_fn(self.data.as_const(), f) }
        } else {
            f.write_str("<no debug impl>")
        }
    }

    /// Format the value using its Display implementation
    pub fn display_fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(display_fn) = self.vtable().display {
            unsafe { display_fn(self.data.as_const(), f) }
        } else {
            f.write_str("<no display impl>")
        }
    }

    /// Get the scalar type if set.
    pub fn scalar_type(&self) -> Option<ScalarType> {
        ScalarType::try_from_shape(self.shape)
    }
}
