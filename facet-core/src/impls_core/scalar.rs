use crate::value_vtable;
use crate::*;
use core::num::NonZero;
use typeid::ConstTypeId;

unsafe impl Facet<'_> for ConstTypeId {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(ConstTypeId, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("ConstTypeId")
            .def(Def::Scalar)
            .ty(Type::User(UserType::Opaque))
            .build()
    };
}

unsafe impl Facet<'_> for core::any::TypeId {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(core::any::TypeId, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("TypeId")
            .def(Def::Scalar)
            .ty(Type::User(UserType::Opaque))
            .build()
    };
}

unsafe impl Facet<'_> for () {
    const VTABLE: &'static ValueVTable = &const { value_vtable!((), |f, _opts| write!(f, "()")) };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("()")
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::default(),
                kind: StructKind::Tuple,
                fields: &[],
            })))
            .build()
    };
}

unsafe impl<'a, T: ?Sized + 'a> Facet<'a> for core::marker::PhantomData<T> {
    // TODO: we might be able to do something with specialization re: the shape of T?
    const VTABLE: &'static ValueVTable =
        &const { value_vtable!((), |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier)) };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("PhantomData")
            .def(Def::Scalar)
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::default(),
                kind: StructKind::Unit,
                fields: &[],
            })))
            .build()
    };
}

unsafe impl Facet<'_> for char {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(char, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("char")
            .def(Def::Scalar)
            .ty(Type::Primitive(PrimitiveType::Textual(TextualType::Char)))
            .build()
    };
}

unsafe impl Facet<'_> for str {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable_unsized!(str, |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_unsized::<Self>()
            .type_identifier("str")
            .ty(Type::Primitive(PrimitiveType::Textual(TextualType::Str)))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl Facet<'_> for bool {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(bool, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("bool")
            .def(Def::Scalar)
            .ty(Type::Primitive(PrimitiveType::Boolean))
            .build()
    };
}

macro_rules! impl_facet_for_integer {
    ($type:ty) => {
        unsafe impl<'a> Facet<'a> for $type {
            const VTABLE: &'static ValueVTable = &const {
                value_vtable!($type, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ))
            };

            const SHAPE: &'static Shape<'static> = &const {
                Shape::builder_for_sized::<Self>()
                    .type_identifier(stringify!($type))
                    .ty(Type::Primitive(PrimitiveType::Numeric(
                        NumericType::Integer {
                            signed: (1 as $type).checked_neg().is_some(),
                        },
                    )))
                    .def(Def::Scalar)
                    .build()
            };
        }

        unsafe impl<'a> Facet<'a> for NonZero<$type> {
            const VTABLE: &'static ValueVTable = &const {
                // Define conversion functions for transparency
                unsafe fn try_from<'shape, 'dst>(
                    src_ptr: PtrConst<'_>,
                    src_shape: &'shape Shape<'shape>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
                    if src_shape == <$type as Facet>::SHAPE {
                        // Get the inner value and check that it's non-zero
                        let value = unsafe { *src_ptr.get::<$type>() };
                        let nz = NonZero::new(value)
                            .ok_or_else(|| TryFromError::Generic("value should be non-zero"))?;

                        // Put the NonZero value into the destination
                        Ok(unsafe { dst.put(nz) })
                    } else {
                        let inner_try_from =
                            (<$type as Facet>::SHAPE.vtable.sized().unwrap().try_from)().ok_or(
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

                let mut vtable = value_vtable!($type, |f, _opts| write!(
                    f,
                    "{}<{}>",
                    Self::SHAPE.type_identifier,
                    stringify!($type)
                ));

                // Add our new transparency functions
                {
                    let vtable_sized = vtable.sized_mut().unwrap();
                    vtable_sized.try_from = || Some(try_from);
                    vtable_sized.try_into_inner = || Some(try_into_inner);
                    vtable_sized.try_borrow_inner = || Some(try_borrow_inner);
                }

                vtable
            };

            const SHAPE: &'static Shape<'static> = &const {
                // Function to return inner type's shape
                fn inner_shape() -> &'static Shape<'static> {
                    <$type as Facet>::SHAPE
                }

                Shape::builder_for_sized::<Self>()
                    .type_identifier("NonZero")
                    .def(Def::Scalar)
                    .ty(Type::User(UserType::Struct(StructType {
                        repr: Repr::transparent(),
                        kind: StructKind::TupleStruct,
                        fields: &const {
                            [Field::builder()
                                .name("0")
                                // TODO: is it correct to represent $type here, when we, in
                                // fact, store $type::NonZeroInner.
                                .shape(<$type>::SHAPE)
                                .offset(0)
                                .flags(FieldFlags::EMPTY)
                                .build()]
                        },
                    })))
                    .inner(inner_shape)
                    .build()
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

unsafe impl Facet<'_> for f32 {
    const VTABLE: &'static ValueVTable =
        &const { value_vtable!(f32, |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier)) };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("f32")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl Facet<'_> for f64 {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable =
            value_vtable!(f64, |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier));

        {
            let vtable_sized = vtable.sized_mut().unwrap();
            vtable_sized.try_from = || {
                Some(|source, source_shape, dest| {
                    if source_shape == Self::SHAPE {
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
                        expected: &[Self::SHAPE, u64::SHAPE, i64::SHAPE, f32::SHAPE],
                    })
                })
            };
        }

        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("f64")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl Facet<'_> for core::net::SocketAddr {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(core::net::SocketAddr, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("SocketAddr")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl Facet<'_> for core::net::IpAddr {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(core::net::IpAddr, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("IpAddr")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl Facet<'_> for core::net::Ipv4Addr {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(core::net::Ipv4Addr, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("Ipv4Addr")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}

unsafe impl Facet<'_> for core::net::Ipv6Addr {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(core::net::Ipv6Addr, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("Ipv6Addr")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .build()
    };
}
