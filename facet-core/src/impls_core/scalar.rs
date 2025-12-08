use crate::*;
#[cfg(feature = "nonzero")]
use core::num::NonZero;
use typeid::ConstTypeId;

unsafe impl Facet<'_> for ConstTypeId {
    const SHAPE: &'static Shape = &const {
        // ConstTypeId implements Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash
        // but NOT Display or Default
        ShapeBuilder::for_sized::<ConstTypeId>(|f, _opts| write!(f, "ConstTypeId"), "ConstTypeId")
            .clone_into(|src, dst| unsafe { dst.put(*src.get::<ConstTypeId>()) })
            .debug(|data, f| {
                let data = unsafe { data.get::<ConstTypeId>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe {
                *left.get::<ConstTypeId>() == *right.get::<ConstTypeId>()
            })
            .partial_ord(|left, right| unsafe {
                left.get::<ConstTypeId>()
                    .partial_cmp(right.get::<ConstTypeId>())
            })
            .ord(|left, right| unsafe { left.get::<ConstTypeId>().cmp(right.get::<ConstTypeId>()) })
            .hash(|value, hasher| {
                use core::hash::Hash;
                let value = unsafe { value.get::<ConstTypeId>() };
                value.hash(&mut { hasher })
            })
            .markers(MarkerTraits::EMPTY.with_eq().with_copy())
            .build()
    };
}

unsafe impl Facet<'_> for core::any::TypeId {
    const SHAPE: &'static Shape = &const {
        // TypeId implements Debug, Clone, Copy, PartialEq, Eq, Hash
        // but NOT Display, Default, PartialOrd, or Ord
        ShapeBuilder::for_sized::<core::any::TypeId>(|f, _opts| write!(f, "TypeId"), "TypeId")
            .clone_into(|src, dst| unsafe { dst.put(*src.get::<core::any::TypeId>()) })
            .debug(|data, f| {
                let data = unsafe { data.get::<core::any::TypeId>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe {
                *left.get::<core::any::TypeId>() == *right.get::<core::any::TypeId>()
            })
            // TypeId doesn't implement PartialOrd or Ord
            .hash(|value, hasher| {
                use core::hash::Hash;
                let value = unsafe { value.get::<core::any::TypeId>() };
                value.hash(&mut { hasher })
            })
            .markers(MarkerTraits::EMPTY.with_eq().with_copy())
            .build()
    };
}

unsafe impl Facet<'_> for () {
    const SHAPE: &'static Shape = &const {
        // () implements Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash
        // but NOT Display - no need for impls! checks
        ShapeBuilder::for_sized::<()>(|f, _opts| write!(f, "()"), "()")
            .def(Def::Undefined)
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::default(),
                kind: StructKind::Tuple,
                fields: &[],
            })))
            .default_in_place(|target| unsafe { target.put(()) })
            .clone_into(|_src, dst| unsafe { dst.put(()) })
            .parse(|s, target| {
                if s == "()" {
                    Ok(unsafe { target.put(()) })
                } else {
                    Err(crate::types::ParseError::Generic("failed to parse ()"))
                }
            })
            .debug(|_data, f| write!(f, "()"))
            .partial_eq(|_left, _right| true) // () == ()
            .partial_ord(|_left, _right| Some(core::cmp::Ordering::Equal))
            .ord(|_left, _right| core::cmp::Ordering::Equal)
            .hash(|_value, _hasher| {
                // () hashes to nothing, but we need to call the hasher for consistency
            })
            .markers(MarkerTraits::EMPTY.with_eq().with_copy())
            .build()
    };
}

unsafe impl<'a, T: ?Sized + 'a> Facet<'a> for core::marker::PhantomData<T> {
    const SHAPE: &'static Shape = &const {
        // PhantomData<T> implements Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash
        // unconditionally (not depending on T) - but NOT Display
        ShapeBuilder::for_sized::<core::marker::PhantomData<T>>(
            |f, _opts| write!(f, "PhantomData"),
            "PhantomData",
        )
        .ty(Type::User(UserType::Struct(StructType {
            repr: Repr::default(),
            kind: StructKind::Unit,
            fields: &[],
        })))
        .default_in_place(|target| unsafe { target.put(core::marker::PhantomData::<()>) })
        .clone_into(|_src, dst| unsafe { dst.put(core::marker::PhantomData::<()>) })
        .debug(|_data, f| write!(f, "PhantomData"))
        .partial_eq(|_left, _right| true) // All PhantomData are equal
        .partial_ord(|_left, _right| Some(core::cmp::Ordering::Equal))
        .ord(|_left, _right| core::cmp::Ordering::Equal)
        .hash(|_value, _hasher| {
            // PhantomData hashes to nothing
        })
        .markers(MarkerTraits::EMPTY.with_eq().with_copy())
        .build()
    };
}

unsafe impl Facet<'_> for char {
    const SHAPE: &'static Shape = &const {
        // char implements all standard traits - no need for impls! checks
        ShapeBuilder::for_sized::<char>(|f, _opts| write!(f, "char"), "char")
            .ty(Type::Primitive(PrimitiveType::Textual(TextualType::Char)))
            .default_in_place(|target| unsafe { target.put(<char as Default>::default()) })
            .clone_into(|src, dst| unsafe { dst.put(*src.get::<char>()) })
            .parse(|s, target| match s.parse::<char>() {
                Ok(value) => Ok(unsafe { target.put(value) }),
                Err(_) => Err(crate::types::ParseError::Generic("failed to parse char")),
            })
            .display(|data, f| {
                let data = unsafe { data.get::<char>() };
                core::fmt::Display::fmt(data, f)
            })
            .debug(|data, f| {
                let data = unsafe { data.get::<char>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe { *left.get::<char>() == *right.get::<char>() })
            .partial_ord(|left, right| unsafe {
                left.get::<char>().partial_cmp(right.get::<char>())
            })
            .ord(|left, right| unsafe { left.get::<char>().cmp(right.get::<char>()) })
            .hash(|value, hasher| {
                use core::hash::Hash;
                let value = unsafe { value.get::<char>() };
                value.hash(&mut { hasher })
            })
            .markers(MarkerTraits::EMPTY.with_eq().with_copy())
            .build()
    };
}

unsafe impl Facet<'_> for str {
    const SHAPE: &'static Shape = &const {
        // str implements Debug, Display, PartialEq, Eq, PartialOrd, Ord, Hash
        // but NOT Clone or Default (unsized type)
        ShapeBuilder::for_unsized::<str>(|f, _opts| write!(f, "str"), "str")
            .ty(Type::Primitive(PrimitiveType::Textual(TextualType::Str)))
            // str is unsized - no default, clone, or parse
            .display(|data, f| {
                let data = unsafe { data.get::<str>() };
                core::fmt::Display::fmt(data, f)
            })
            .debug(|data, f| {
                let data = unsafe { data.get::<str>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe { *left.get::<str>() == *right.get::<str>() })
            .partial_ord(|left, right| unsafe { left.get::<str>().partial_cmp(right.get::<str>()) })
            .ord(|left, right| unsafe { left.get::<str>().cmp(right.get::<str>()) })
            .hash(|value, hasher| {
                use core::hash::Hash;
                let value = unsafe { value.get::<str>() };
                value.hash(&mut { hasher })
            })
            .markers(MarkerTraits::EMPTY.with_eq())
            .build()
    };
}

unsafe impl Facet<'_> for bool {
    const SHAPE: &'static Shape = &const {
        // bool implements all standard traits - no need for impls! checks
        ShapeBuilder::for_sized::<bool>(|f, _opts| write!(f, "bool"), "bool")
            .ty(Type::Primitive(PrimitiveType::Boolean))
            .default_in_place(|target| unsafe { target.put(<bool as Default>::default()) })
            .clone_into(|src, dst| unsafe { dst.put(*src.get::<bool>()) })
            .parse(|s, target| match s.parse::<bool>() {
                Ok(value) => Ok(unsafe { target.put(value) }),
                Err(_) => Err(crate::types::ParseError::Generic("failed to parse bool")),
            })
            .display(|data, f| {
                let data = unsafe { data.get::<bool>() };
                core::fmt::Display::fmt(data, f)
            })
            .debug(|data, f| {
                let data = unsafe { data.get::<bool>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe { *left.get::<bool>() == *right.get::<bool>() })
            .partial_ord(|left, right| unsafe {
                left.get::<bool>().partial_cmp(right.get::<bool>())
            })
            .ord(|left, right| unsafe { left.get::<bool>().cmp(right.get::<bool>()) })
            .hash(|value, hasher| {
                use core::hash::Hash;
                let value = unsafe { value.get::<bool>() };
                value.hash(&mut { hasher })
            })
            .markers(MarkerTraits::EMPTY.with_eq().with_copy())
            .build()
    };
}

macro_rules! impl_facet_for_integer {
    ($type:ty) => {
        unsafe impl<'a> Facet<'a> for $type {
            const SHAPE: &'static Shape = &const {
                ShapeBuilder::for_sized::<$type>(
                    |f, _opts| write!(f, "{}", stringify!($type)),
                    stringify!($type),
                )
                .ty(Type::Primitive(PrimitiveType::Numeric(
                    NumericType::Integer {
                        signed: (1 as $type).checked_neg().is_some(),
                    },
                )))
                .default_in_place(|target| unsafe { target.put(<$type as Default>::default()) })
                .clone_into(|src, dst| unsafe { dst.put(*src.get::<$type>()) })
                .parse(|s, target| match s.parse::<$type>() {
                    Ok(value) => Ok(unsafe { target.put(value) }),
                    Err(_) => Err(crate::types::ParseError::Generic(concat!(
                        "failed to parse ",
                        stringify!($type)
                    ))),
                })
                .display(|data, f| {
                    let data = unsafe { data.get::<$type>() };
                    core::fmt::Display::fmt(data, f)
                })
                .debug(|data, f| {
                    let data = unsafe { data.get::<$type>() };
                    core::fmt::Debug::fmt(data, f)
                })
                .partial_eq(|left, right| unsafe { *left.get::<$type>() == *right.get::<$type>() })
                .partial_ord(|left, right| unsafe {
                    left.get::<$type>().partial_cmp(right.get::<$type>())
                })
                .ord(|left, right| unsafe { left.get::<$type>().cmp(right.get::<$type>()) })
                .hash(|value, hasher| {
                    use core::hash::Hash;
                    let value = unsafe { value.get::<$type>() };
                    value.hash(&mut { hasher })
                })
                .markers(MarkerTraits::EMPTY.with_eq().with_copy())
                .build()
            };
        }
    };
}

#[cfg(feature = "nonzero")]
macro_rules! impl_facet_for_nonzero {
    ($type:ty) => {
        unsafe impl<'a> Facet<'a> for NonZero<$type> {
            const SHAPE: &'static Shape = &const {
                // Define conversion functions for transparency
                unsafe fn try_from<'dst>(
                    src_ptr: PtrConst<'_>,
                    src_shape: &'static Shape,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryFromError> {
                    if src_shape == <$type as Facet>::SHAPE {
                        // Get the inner value and check that it's non-zero
                        let value = unsafe { *src_ptr.get::<$type>() };
                        let nz = NonZero::new(value)
                            .ok_or_else(|| TryFromError::Generic("value should be non-zero"))?;

                        // Put the NonZero value into the destination
                        Ok(unsafe { dst.put(nz) })
                    } else {
                        let inner_try_from = <$type as Facet>::SHAPE.vtable.try_from.ok_or(
                            TryFromError::UnsupportedSourceShape {
                                src_shape,
                                expected: &[<$type as Facet>::SHAPE],
                            },
                        )?;

                        // fallback to inner's try_from
                        // This relies on the fact that `dst` is the same size as `NonZero<$type>`
                        // which should be true because `NonZero` is `repr(transparent)`
                        let inner_result = unsafe { (inner_try_from)(src_ptr, src_shape, dst) };
                        match inner_result {
                            Ok(result) => {
                                // After conversion to inner type, wrap as NonZero
                                let value = unsafe { *result.get::<$type>() };
                                let nz = NonZero::new(value).ok_or_else(|| {
                                    TryFromError::Generic("value should be non-zero")
                                })?;
                                Ok(unsafe { dst.put(nz) })
                            }
                            Err(e) => Err(e),
                        }
                    }
                }

                unsafe fn try_into_inner<'dst>(
                    src_ptr: PtrMut<'_>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
                    // Get the NonZero value and extract the inner value
                    let nz = unsafe { *src_ptr.get::<NonZero<$type>>() };
                    // Put the inner value into the destination
                    Ok(unsafe { dst.put(nz.get()) })
                }

                unsafe fn try_borrow_inner(
                    src_ptr: PtrConst<'_>,
                ) -> Result<PtrConst<'_>, TryBorrowInnerError> {
                    // NonZero<T> has the same memory layout as T, so we can return the input pointer directly
                    Ok(src_ptr)
                }

                // NonZero implements Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash
                // but NOT Default - no need for impls! checks
                let mut vtable = ValueVTable {
                    type_name: |f, _opts| write!(f, "NonZero<{}>", stringify!($type)),
                    drop_in_place: None,    // Copy type
                    default_in_place: None, // NonZero has no Default
                    clone_into: Some(|src, dst| unsafe { dst.put(*src.get::<NonZero<$type>>()) }),
                    parse: Some(|s, target| match s.parse::<NonZero<$type>>() {
                        Ok(value) => Ok(unsafe { target.put(value) }),
                        Err(_) => Err(crate::types::ParseError::Generic(concat!(
                            "failed to parse NonZero<",
                            stringify!($type),
                            ">"
                        ))),
                    }),
                    invariants: None,
                    try_from: None,
                    try_into_inner: None,
                    try_borrow_inner: None,
                    format: FormatVTable {
                        display: Some(|data, f| {
                            let data = unsafe { data.get::<NonZero<$type>>() };
                            core::fmt::Display::fmt(data, f)
                        }),
                        debug: Some(|data, f| {
                            let data = unsafe { data.get::<NonZero<$type>>() };
                            core::fmt::Debug::fmt(data, f)
                        }),
                    },
                    cmp: CmpVTable {
                        partial_eq: Some(|left, right| unsafe {
                            *left.get::<NonZero<$type>>() == *right.get::<NonZero<$type>>()
                        }),
                        partial_ord: Some(|left, right| unsafe {
                            left.get::<NonZero<$type>>()
                                .partial_cmp(right.get::<NonZero<$type>>())
                        }),
                        ord: Some(|left, right| unsafe {
                            left.get::<NonZero<$type>>()
                                .cmp(right.get::<NonZero<$type>>())
                        }),
                    },
                    hash: HashVTable {
                        hash: Some(|value, hasher| {
                            use core::hash::Hash;
                            let value = unsafe { value.get::<NonZero<$type>>() };
                            value.hash(&mut { hasher })
                        }),
                    },
                    markers: MarkerTraits::EMPTY.with_eq().with_copy(),
                };

                // Add our new transparency functions
                {
                    vtable.try_from = Some(try_from);
                    vtable.try_into_inner = Some(try_into_inner);
                    vtable.try_borrow_inner = Some(try_borrow_inner);
                }

                Shape {
                    id: Shape::id_of::<Self>(),
                    layout: Shape::layout_of::<Self>(),
                    vtable,
                    type_identifier: "NonZero",
                    def: Def::Scalar,
                    ty: Type::User(UserType::Struct(StructType {
                        repr: Repr::transparent(),
                        kind: StructKind::TupleStruct,
                        fields: &const {
                            [Field {
                                // TODO: is it correct to represent $type here, when we, in
                                // fact, store $type::NonZeroInner.
                                name: "0",
                                shape: ShapeRef::Static(<$type>::SHAPE),
                                offset: 0,
                                flags: FieldFlags::empty(),
                                rename: None,
                                alias: None,
                                attributes: &[],
                                doc: &[],
                            }]
                        },
                    })),
                    type_params: &[],
                    doc: &[],
                    attributes: &[],
                    type_tag: None,
                    inner: Some(<$type as Facet>::SHAPE),
                    proxy: None,
                    variance: Variance::Invariant,
                }
            };
        }
    };
}

impl_facet_for_integer!(u8);
impl_facet_for_integer!(i8);
impl_facet_for_integer!(u16);
impl_facet_for_integer!(i16);
impl_facet_for_integer!(u32);
impl_facet_for_integer!(i32);
impl_facet_for_integer!(u64);
impl_facet_for_integer!(i64);
impl_facet_for_integer!(u128);
impl_facet_for_integer!(i128);
impl_facet_for_integer!(usize);
impl_facet_for_integer!(isize);

#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(u8);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(i8);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(u16);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(i16);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(u32);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(i32);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(u64);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(i64);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(u128);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(i128);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(usize);
#[cfg(feature = "nonzero")]
impl_facet_for_nonzero!(isize);

unsafe impl Facet<'_> for f32 {
    const SHAPE: &'static Shape = &const {
        // f32 implements Debug, Display, Clone, Copy, Default, PartialEq, PartialOrd
        // but NOT Eq, Ord, or Hash (because of NaN) - no need for impls! checks
        ShapeBuilder::for_sized::<f32>(|f, _opts| write!(f, "f32"), "f32")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .default_in_place(|target| unsafe { target.put(<f32 as Default>::default()) })
            .clone_into(|src, dst| unsafe { dst.put(*src.get::<f32>()) })
            .parse(|s, target| match s.parse::<f32>() {
                Ok(value) => Ok(unsafe { target.put(value) }),
                Err(_) => Err(crate::types::ParseError::Generic("failed to parse f32")),
            })
            .display(|data, f| {
                let data = unsafe { data.get::<f32>() };
                core::fmt::Display::fmt(data, f)
            })
            .debug(|data, f| {
                let data = unsafe { data.get::<f32>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe { *left.get::<f32>() == *right.get::<f32>() })
            .partial_ord(|left, right| unsafe { left.get::<f32>().partial_cmp(right.get::<f32>()) })
            // f32 does not implement Ord or Hash (because of NaN)
            .markers(MarkerTraits::EMPTY.with_copy())
            .build()
    };
}

unsafe impl Facet<'_> for f64 {
    const SHAPE: &'static Shape = &const {
        // f64 implements Debug, Display, Clone, Copy, Default, PartialEq, PartialOrd
        // but NOT Eq, Ord, or Hash (because of NaN) - no need for impls! checks
        ShapeBuilder::for_sized::<f64>(|f, _opts| write!(f, "f64"), "f64")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .default_in_place(|target| unsafe { target.put(<f64 as Default>::default()) })
            .clone_into(|src, dst| unsafe { dst.put(*src.get::<f64>()) })
            .parse(|s, target| match s.parse::<f64>() {
                Ok(value) => Ok(unsafe { target.put(value) }),
                Err(_) => Err(crate::types::ParseError::Generic("failed to parse f64")),
            })
            .try_from(|source, source_shape, dest| {
                if source_shape == f64::SHAPE {
                    return Ok(unsafe { dest.copy_from(source, source_shape)? });
                }
                if source_shape == u64::SHAPE {
                    let value: u64 = *unsafe { source.get::<u64>() };
                    let converted: f64 = value as f64;
                    return Ok(unsafe { dest.put::<f64>(converted) });
                }
                if source_shape == i64::SHAPE {
                    let value: i64 = *unsafe { source.get::<i64>() };
                    let converted: f64 = value as f64;
                    return Ok(unsafe { dest.put::<f64>(converted) });
                }
                if source_shape == f32::SHAPE {
                    let value: f32 = *unsafe { source.get::<f32>() };
                    let converted: f64 = value as f64;
                    return Ok(unsafe { dest.put::<f64>(converted) });
                }
                Err(TryFromError::UnsupportedSourceShape {
                    src_shape: source_shape,
                    expected: &[f64::SHAPE, u64::SHAPE, i64::SHAPE, f32::SHAPE],
                })
            })
            .display(|data, f| {
                let data = unsafe { data.get::<f64>() };
                core::fmt::Display::fmt(data, f)
            })
            .debug(|data, f| {
                let data = unsafe { data.get::<f64>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe { *left.get::<f64>() == *right.get::<f64>() })
            .partial_ord(|left, right| unsafe { left.get::<f64>().partial_cmp(right.get::<f64>()) })
            // f64 does not implement Ord or Hash (because of NaN)
            .markers(MarkerTraits::EMPTY.with_copy())
            .build()
    };
}

// Macro for network types - they all implement Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash
// but NOT Default - no need for impls! checks
#[cfg(feature = "net")]
macro_rules! impl_facet_for_net_type {
    ($type:ty, $name:literal) => {
        #[cfg(feature = "net")]
        unsafe impl Facet<'_> for $type {
            const SHAPE: &'static Shape = &const {
                ShapeBuilder::for_sized::<$type>(|f, _opts| write!(f, $name), $name)
                    .clone_into(|src, dst| unsafe { dst.put(*src.get::<$type>()) })
                    .parse(|s, target| match s.parse::<$type>() {
                        Ok(value) => Ok(unsafe { target.put(value) }),
                        Err(_) => Err(crate::types::ParseError::Generic(concat!(
                            "failed to parse ",
                            $name
                        ))),
                    })
                    .display(|data, f| {
                        let data = unsafe { data.get::<$type>() };
                        core::fmt::Display::fmt(data, f)
                    })
                    .debug(|data, f| {
                        let data = unsafe { data.get::<$type>() };
                        core::fmt::Debug::fmt(data, f)
                    })
                    .partial_eq(|left, right| unsafe {
                        *left.get::<$type>() == *right.get::<$type>()
                    })
                    .partial_ord(|left, right| unsafe {
                        left.get::<$type>().partial_cmp(right.get::<$type>())
                    })
                    .ord(|left, right| unsafe { left.get::<$type>().cmp(right.get::<$type>()) })
                    .hash(|value, hasher| {
                        use core::hash::Hash;
                        let value = unsafe { value.get::<$type>() };
                        value.hash(&mut { hasher })
                    })
                    .markers(MarkerTraits::EMPTY.with_eq().with_copy())
                    .build()
            };
        }
    };
}

#[cfg(feature = "net")]
impl_facet_for_net_type!(core::net::SocketAddr, "SocketAddr");
#[cfg(feature = "net")]
impl_facet_for_net_type!(core::net::IpAddr, "IpAddr");
#[cfg(feature = "net")]
impl_facet_for_net_type!(core::net::Ipv4Addr, "Ipv4Addr");

#[cfg(feature = "net")]
impl_facet_for_net_type!(core::net::Ipv6Addr, "Ipv6Addr");
