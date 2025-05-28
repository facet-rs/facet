use core::{cmp::Ordering, marker::PhantomData, mem::transmute};
use facet_core::{
    Def, Facet, PointerType, PtrConst, PtrConstWide, PtrMut, SequenceType, Shape, Type,
    TypeNameOpts, UserType, ValueVTable,
};

use crate::{ReflectError, ScalarType};

use super::{
    ListLikeDef, PeekEnum, PeekList, PeekListLike, PeekMap, PeekSmartPointer, PeekStruct, PeekTuple,
};

/// A unique identifier for a peek value
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId<'shape> {
    pub(crate) shape: &'shape Shape<'shape>,
    pub(crate) ptr: *const u8,
}

impl<'shape> ValueId<'shape> {
    pub(crate) fn new(shape: &'shape Shape<'shape>, ptr: *const u8) -> Self {
        Self { shape, ptr }
    }
}

impl core::fmt::Display for ValueId<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}@{:p}", self.shape, self.ptr)
    }
}

impl core::fmt::Debug for ValueId<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

#[derive(Clone, Copy)]
pub enum GenericPtr<'mem> {
    Thin(PtrConst<'mem>),
    Wide(PtrConstWide<'mem>),
}

impl<'a> From<PtrConst<'a>> for GenericPtr<'a> {
    fn from(value: PtrConst<'a>) -> Self {
        GenericPtr::Thin(value)
    }
}

impl<'a> From<PtrConstWide<'a>> for GenericPtr<'a> {
    fn from(value: PtrConstWide<'a>) -> Self {
        GenericPtr::Wide(value)
    }
}

impl<'mem> GenericPtr<'mem> {
    fn new<T: ?Sized>(ptr: *const T) -> Self {
        if size_of_val(&ptr) == size_of::<PtrConst>() {
            GenericPtr::Thin(PtrConst::new(ptr.cast::<()>()))
        } else if size_of_val(&ptr) == size_of::<PtrConstWide>() {
            GenericPtr::Wide(PtrConstWide::new(ptr))
        } else {
            panic!("Couldn't determine if pointer to T is thin or wide");
        }
    }

    pub fn thin(self) -> Option<PtrConst<'mem>> {
        match self {
            GenericPtr::Thin(ptr) => Some(ptr),
            GenericPtr::Wide(ptr) => None,
        }
    }

    unsafe fn get<T: ?Sized>(self) -> &'mem T {
        match self {
            GenericPtr::Thin(ptr) => {
                let ptr = ptr.as_byte_ptr();
                let ptr_ref = &ptr;
                let v_ref_ref = unsafe { transmute::<&*const u8, &&T>(ptr_ref) };
                *v_ref_ref
            }
            GenericPtr::Wide(ptr) => unsafe { ptr.get() },
        }
    }

    fn as_byte_ptr(self) -> *const u8 {
        match self {
            GenericPtr::Thin(ptr) => ptr.as_byte_ptr(),
            GenericPtr::Wide(ptr) => ptr.as_byte_ptr(),
        }
    }
}

/// Lets you read from a value (implements read-only [`ValueVTable`] proxies)
#[derive(Clone, Copy)]
pub struct Peek<'mem, 'facet, 'shape> {
    /// Underlying data
    pub(crate) data: GenericPtr<'mem>,

    /// Shape of the value
    pub(crate) shape: &'shape Shape<'shape>,

    invariant: PhantomData<fn(&'facet ()) -> &'facet ()>,
}

impl<'mem, 'facet, 'shape> Peek<'mem, 'facet, 'shape> {
    /// Creates a new `PeekValue` instance for a value of type `T`.
    pub fn new<T: Facet<'facet> + ?Sized>(t: &'mem T) -> Self {
        Self {
            data: GenericPtr::new(t),
            shape: T::SHAPE,
            invariant: PhantomData,
        }
    }

    /// Creates a new `PeekValue` instance without checking the type.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it doesn't check if the provided data
    /// and shape are compatible. The caller must ensure that the data is valid
    /// for the given shape.
    pub unsafe fn unchecked_new(
        data: impl Into<GenericPtr<'mem>>,
        shape: &'shape Shape<'shape>,
    ) -> Self {
        Self {
            data: data.into(),
            shape,
            invariant: PhantomData,
        }
    }

    /// Returns the vtable
    #[inline(always)]
    pub fn vtable(&self) -> &'shape ValueVTable {
        self.shape.vtable
    }

    /// Returns a unique identifier for this value, usable for cycle detection
    pub fn id(&self) -> ValueId<'shape> {
        ValueId::new(self.shape, self.data.as_byte_ptr())
    }

    /// Returns true if the two values are pointer-equal
    #[inline]
    pub fn ptr_eq(&self, other: &Peek<'_, '_, '_>) -> bool {
        self.data.as_byte_ptr() == other.data.as_byte_ptr()
    }

    /// Returns true if this scalar is equal to the other scalar
    ///
    /// # Returns
    ///
    /// `false` if equality comparison is not supported for this scalar type
    #[inline]
    pub fn partial_eq(&self, other: &Peek<'_, '_, '_>) -> Option<bool> {
        match (self.data, other.data) {
            (GenericPtr::Thin(a), GenericPtr::Thin(b)) => unsafe {
                (self.vtable().sized().unwrap().partial_eq)().map(|f| f(a, b))
            },
            (GenericPtr::Wide(a), GenericPtr::Wide(b)) => unsafe {
                (self.vtable().r#unsized().unwrap().partial_eq)().map(|f| f(a, b))
            },
            _ => None,
        }
    }

    /// Compares this scalar with another and returns their ordering
    ///
    /// # Returns
    ///
    /// `None` if comparison is not supported for this scalar type
    #[inline]
    pub fn partial_cmp(&self, other: &Peek<'_, '_, '_>) -> Option<Option<Ordering>> {
        match (self.data, other.data) {
            (GenericPtr::Thin(a), GenericPtr::Thin(b)) => unsafe {
                (self.vtable().sized().unwrap().partial_ord)().map(|f| f(a, b))
            },
            (GenericPtr::Wide(a), GenericPtr::Wide(b)) => unsafe {
                (self.vtable().r#unsized().unwrap().partial_ord)().map(|f| f(a, b))
            },
            _ => None,
        }
    }

    /// Hashes this scalar
    ///
    /// # Returns
    ///
    /// `Err` if hashing is not supported for this scalar type, `Ok` otherwise
    #[inline(always)]
    pub fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) -> Result<(), ()> {
        match self.data {
            GenericPtr::Thin(ptr) => {
                if let Some(hash_fn) = (self.vtable().sized().unwrap().hash)() {
                    let hasher_opaque = PtrMut::new(hasher);
                    unsafe {
                        hash_fn(ptr, hasher_opaque, |opaque, bytes| {
                            opaque.as_mut::<H>().write(bytes)
                        })
                    };
                    return Ok(());
                }
            }
            GenericPtr::Wide(ptr) => {
                if let Some(hash_fn) = (self.vtable().r#unsized().unwrap().hash)() {
                    let hasher_opaque = PtrMut::new(hasher);
                    unsafe {
                        hash_fn(ptr, hasher_opaque, |opaque, bytes| {
                            opaque.as_mut::<H>().write(bytes)
                        })
                    };
                    return Ok(());
                }
            }
        }
        Err(())
    }

    /// Returns the type name of this scalar
    ///
    /// # Arguments
    ///
    /// * `f` - A mutable reference to a `core::fmt::Formatter`
    /// * `opts` - The `TypeNameOpts` to use for formatting
    ///
    /// # Returns
    ///
    /// The result of the type name formatting
    #[inline(always)]
    pub fn type_name(
        &self,
        f: &mut core::fmt::Formatter<'_>,
        opts: TypeNameOpts,
    ) -> core::fmt::Result {
        (self.vtable().type_name())(f, opts)
    }

    /// Returns the shape
    #[inline(always)]
    pub const fn shape(&self) -> &'shape Shape<'shape> {
        self.shape
    }

    /// Returns the data
    #[inline(always)]
    pub const fn data(&self) -> GenericPtr<'mem> {
        self.data
    }

    /// Get the scalar type if set.
    pub fn scalar_type(&self) -> Option<ScalarType> {
        ScalarType::try_from_shape(self.shape)
    }

    /// Read the value from memory into a Rust value.
    ///
    /// # Panics
    ///
    /// Panics if the shape doesn't match the type `T`.
    pub fn get<T: Facet<'facet> + ?Sized>(&self) -> Result<&T, ReflectError<'shape>> {
        if self.shape != T::SHAPE {
            Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            })
        } else {
            Ok(unsafe { self.data.get::<T>() })
        }
    }

    /// Try to get the value as a string if it's a string type
    /// Returns None if the value is not a string or couldn't be extracted
    pub fn as_str(&self) -> Option<&'mem str> {
        let peek = self.innermost_peek();
        if let Some(ScalarType::Str) = peek.scalar_type() {
            unsafe { Some(peek.data.get::<&str>()) }
        } else if let Some(ScalarType::String) = peek.scalar_type() {
            unsafe { Some(peek.data.get::<alloc::string::String>().as_str()) }
        } else if let Type::Pointer(PointerType::Reference(vpt)) = peek.shape.ty {
            let target_shape = (vpt.target)();
            if let Some(ScalarType::Str) = ScalarType::try_from_shape(target_shape) {
                unsafe { Some(peek.data.get::<&str>()) }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Tries to identify this value as a struct
    pub fn into_struct(self) -> Result<PeekStruct<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        if let Type::User(UserType::Struct(ty)) = self.shape.ty {
            Ok(PeekStruct { value: self, ty })
        } else {
            Err(ReflectError::WasNotA {
                expected: "struct",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as an enum
    pub fn into_enum(self) -> Result<PeekEnum<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        if let Type::User(UserType::Enum(ty)) = self.shape.ty {
            Ok(PeekEnum { value: self, ty })
        } else {
            Err(ReflectError::WasNotA {
                expected: "enum",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a map
    pub fn into_map(self) -> Result<PeekMap<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        if let Def::Map(def) = self.shape.def {
            Ok(PeekMap { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                expected: "map",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a list
    pub fn into_list(self) -> Result<PeekList<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        if let Def::List(def) = self.shape.def {
            return Ok(PeekList { value: self, def });
        }

        Err(ReflectError::WasNotA {
            expected: "list",
            actual: self.shape,
        })
    }

    /// Tries to identify this value as a list, array or slice
    pub fn into_list_like(
        self,
    ) -> Result<PeekListLike<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        match self.shape.def {
            Def::List(def) => Ok(PeekListLike::new(self, ListLikeDef::List(def))),
            Def::Array(def) => Ok(PeekListLike::new(self, ListLikeDef::Array(def))),
            _ => {
                // &[i32] is actually a _pointer_ to a slice.
                match self.shape.ty {
                    Type::Pointer(ptr) => match ptr {
                        PointerType::Reference(vpt) | PointerType::Raw(vpt) => {
                            let target = (vpt.target)();
                            match target.def {
                                Def::Slice(def) => {
                                    return Ok(PeekListLike::new(self, ListLikeDef::Slice(def)));
                                }
                                _ => {
                                    // well it's not list-like then
                                }
                            }
                        }
                        PointerType::Function(_) => {
                            // well that's not a list-like
                        }
                    },
                    _ => {
                        // well that's not a list-like either
                    }
                }

                Err(ReflectError::WasNotA {
                    expected: "list, array or slice",
                    actual: self.shape,
                })
            }
        }
    }

    /// Tries to identify this value as a smart pointer
    pub fn into_smart_pointer(
        self,
    ) -> Result<PeekSmartPointer<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        if let Def::SmartPointer(def) = self.shape.def {
            Ok(PeekSmartPointer { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                expected: "smart pointer",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as an option
    pub fn into_option(
        self,
    ) -> Result<super::PeekOption<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        if let Def::Option(def) = self.shape.def {
            Ok(super::PeekOption { value: self, def })
        } else {
            Err(ReflectError::WasNotA {
                expected: "option",
                actual: self.shape,
            })
        }
    }

    /// Tries to identify this value as a tuple
    pub fn into_tuple(self) -> Result<PeekTuple<'mem, 'facet, 'shape>, ReflectError<'shape>> {
        if let Type::Sequence(SequenceType::Tuple(ty)) = self.shape.ty {
            Ok(PeekTuple { value: self, ty })
        } else {
            Err(ReflectError::WasNotA {
                expected: "tuple",
                actual: self.shape,
            })
        }
    }

    /// Tries to return the innermost value — useful for serialization. For example, we serialize a `NonZero<u8>` the same
    /// as a `u8`. Similarly, we serialize a `Utf8PathBuf` the same as a `String.
    ///
    /// Returns a `Peek` to the innermost value, unwrapping transparent wrappers recursively.
    /// For example, this will peel through newtype wrappers or smart pointers that have an `inner`.
    pub fn innermost_peek(self) -> Self {
        let mut current_peek = self;
        while let (Some(try_borrow_inner_fn), Some(inner_shape)) = (
            current_peek
                .vtable()
                .sized()
                .and_then(|s| (s.try_borrow_inner)()),
            current_peek.shape.inner,
        ) {
            unsafe {
                let inner_data = try_borrow_inner_fn(current_peek.data.thin().unwrap()).unwrap_or_else(|e| {
                    panic!("innermost_peek: try_borrow_inner returned an error! was trying to go from {} to {}. error: {e}", current_peek.shape,
                        inner_shape())
                });

                current_peek = Peek {
                    data: inner_data.into(),
                    shape: inner_shape(),
                    invariant: PhantomData,
                };
            }
        }
        current_peek
    }
}

impl<'mem, 'facet, 'shape> core::fmt::Display for Peek<'mem, 'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.data {
            GenericPtr::Thin(ptr) => {
                if let Some(display_fn) = (self.vtable().sized().unwrap().display)() {
                    return unsafe { display_fn(ptr, f) };
                }
            }
            GenericPtr::Wide(ptr) => {
                if let Some(display_fn) = (self.vtable().r#unsized().unwrap().display)() {
                    return unsafe { display_fn(ptr, f) };
                }
            }
        }
        write!(f, "⟨{}⟩", self.shape)
    }
}

impl<'mem, 'facet, 'shape> core::fmt::Debug for Peek<'mem, 'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.data {
            GenericPtr::Thin(ptr) => {
                if let Some(debug_fn) = (self.vtable().sized().unwrap().debug)() {
                    return unsafe { debug_fn(ptr, f) };
                }
            }
            GenericPtr::Wide(ptr) => {
                if let Some(debug_fn) = (self.vtable().r#unsized().unwrap().debug)() {
                    return unsafe { debug_fn(ptr, f) };
                }
            }
        }
        write!(f, "⟨{}⟩", self.shape)
    }
}

impl<'mem, 'facet, 'shape> core::cmp::PartialEq for Peek<'mem, 'facet, 'shape> {
    fn eq(&self, other: &Self) -> bool {
        self.partial_eq(other).unwrap_or(false)
    }
}

impl<'mem, 'facet, 'shape> core::cmp::PartialOrd for Peek<'mem, 'facet, 'shape> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.partial_cmp(other).unwrap_or(None)
    }
}

impl<'mem, 'facet, 'shape> core::hash::Hash for Peek<'mem, 'facet, 'shape> {
    fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        self.hash(hasher)
            .expect("Hashing is not supported for this shape");
    }
}
