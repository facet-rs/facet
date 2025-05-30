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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::opaque().build() })
                    .build(),
            ))
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::c(),
                kind: StructKind::Struct,
                fields: &const { [field_in_type!(ConstTypeId, type_id_fn)] },
            })))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::opaque().build() })
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::empty().build() })
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::char().build() })
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::string().build() })
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::boolean().build() })
                    .build(),
            ))
            .ty(Type::Primitive(PrimitiveType::Boolean))
            .build()
    };
}

macro_rules! impl_facet_for_integer {
    ($type:ty, $affinity:expr, $nz_affinity:expr) => {
        unsafe impl<'a> Facet<'a> for $type {
            const VTABLE: &'static ValueVTable = &const {
                let mut vtable = value_vtable!($type, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));

                vtable.try_from = || {
                    Some(|source, source_shape, dest| {
                        if source_shape == Self::SHAPE {
                            return Ok(unsafe { dest.copy_from(source, source_shape)? });
                        }
                        if source_shape == u64::SHAPE {
                            let value: u64 = *unsafe { source.get::<u64>() };
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from u64 failed",
                                    ));
                                }
                            }
                        }
                        if source_shape == u32::SHAPE {
                            let value: u32 = *unsafe { source.get::<u32>() };
                            let value: u64 = value as u64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from u32 failed",
                                    ));
                                }
                            }
                        }
                        if source_shape == u16::SHAPE {
                            let value: u16 = *unsafe { source.get::<u16>() };
                            let value: u64 = value as u64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from u16 failed",
                                    ));
                                }
                            }
                        }
                        if source_shape == u8::SHAPE {
                            let value: u8 = *unsafe { source.get::<u8>() };
                            let value: u64 = value as u64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic("conversion from u8 failed"));
                                }
                            }
                        }
                        if source_shape == i64::SHAPE {
                            let value: i64 = *unsafe { source.get::<i64>() };
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from i64 failed",
                                    ));
                                }
                            }
                        }
                        if source_shape == i32::SHAPE {
                            let value: i32 = *unsafe { source.get::<i32>() };
                            let value: i64 = value as i64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from i32 failed",
                                    ));
                                }
                            }
                        }
                        if source_shape == i16::SHAPE {
                            let value: i16 = *unsafe { source.get::<i16>() };
                            let value: i64 = value as i64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from i16 failed",
                                    ));
                                }
                            }
                        }
                        if source_shape == i8::SHAPE {
                            let value: i8 = *unsafe { source.get::<i8>() };
                            let value: i64 = value as i64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic("conversion from i8 failed"));
                                }
                            }
                        }
                        if source_shape == f64::SHAPE {
                            let value: f64 = *unsafe { source.get::<f64>() };
                            let value = value as i64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from f64 failed",
                                    ));
                                }
                            }
                        }
                        if source_shape == f32::SHAPE {
                            let value: f32 = *unsafe { source.get::<f32>() };
                            let value = value as i64;
                            match <$type>::try_from(value) {
                                Ok(converted) => {
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(_) => {
                                    return Err(TryFromError::Generic(
                                        "conversion from f32 failed",
                                    ));
                                }
                            }
                        }
                        Err(TryFromError::UnsupportedSourceShape {
                            src_shape: source_shape,
                            expected: &[Self::SHAPE, u64::SHAPE, i64::SHAPE, f64::SHAPE],
                        })
                    })
                };

                vtable
            };

            const SHAPE: &'static Shape<'static> = &const {
                Shape::builder_for_sized::<Self>()
                    .type_identifier(stringify!($type))
                    .ty(Type::Primitive(PrimitiveType::Numeric(
                        NumericType::Integer {
                            signed: (1 as $type).checked_neg().is_some(),
                        },
                    )))
                    .def(Def::Scalar(
                        ScalarDef::builder().affinity($affinity).build(),
                    ))
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
                        let inner_try_from = (<$type as Facet>::SHAPE.vtable.try_from)().ok_or(
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
                vtable.try_from = || Some(try_from);
                vtable.try_into_inner = || Some(try_into_inner);
                vtable.try_borrow_inner = || Some(try_borrow_inner);

                vtable
            };

            const SHAPE: &'static Shape<'static> = &const {
                // Function to return inner type's shape
                fn inner_shape() -> &'static Shape<'static> {
                    <$type as Facet>::SHAPE
                }

                Shape::builder_for_sized::<Self>()
                    .type_identifier("NonZero")
                    .def(Def::Scalar(
                        ScalarDef::builder().affinity($nz_affinity).build(),
                    ))
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

static MIN_U8: u8 = u8::MIN;
static MAX_U8: u8 = u8::MAX;
static MIN_NZ_U8: NonZero<u8> = NonZero::<u8>::MIN;
static MAX_NZ_U8: NonZero<u8> = NonZero::<u8>::MAX;

static MIN_I8: i8 = i8::MIN;
static MAX_I8: i8 = i8::MAX;
static MIN_NZ_I8: NonZero<i8> = NonZero::<i8>::MIN;
static MAX_NZ_I8: NonZero<i8> = NonZero::<i8>::MAX;

static MIN_U16: u16 = u16::MIN;
static MAX_U16: u16 = u16::MAX;
static MIN_NZ_U16: NonZero<u16> = NonZero::<u16>::MIN;
static MAX_NZ_U16: NonZero<u16> = NonZero::<u16>::MAX;

static MIN_I16: i16 = i16::MIN;
static MAX_I16: i16 = i16::MAX;
static MIN_NZ_I16: NonZero<i16> = NonZero::<i16>::MIN;
static MAX_NZ_I16: NonZero<i16> = NonZero::<i16>::MAX;

static MIN_U32: u32 = u32::MIN;
static MAX_U32: u32 = u32::MAX;
static MIN_NZ_U32: NonZero<u32> = NonZero::<u32>::MIN;
static MAX_NZ_U32: NonZero<u32> = NonZero::<u32>::MAX;

static MIN_I32: i32 = i32::MIN;
static MAX_I32: i32 = i32::MAX;
static MIN_NZ_I32: NonZero<i32> = NonZero::<i32>::MIN;
static MAX_NZ_I32: NonZero<i32> = NonZero::<i32>::MAX;

static MIN_U64: u64 = u64::MIN;
static MAX_U64: u64 = u64::MAX;
static MIN_NZ_U64: NonZero<u64> = NonZero::<u64>::MIN;
static MAX_NZ_U64: NonZero<u64> = NonZero::<u64>::MAX;

static MIN_I64: i64 = i64::MIN;
static MAX_I64: i64 = i64::MAX;
static MIN_NZ_I64: NonZero<i64> = NonZero::<i64>::MIN;
static MAX_NZ_I64: NonZero<i64> = NonZero::<i64>::MAX;

static MIN_U128: u128 = u128::MIN;
static MAX_U128: u128 = u128::MAX;
static MIN_NZ_U128: NonZero<u128> = NonZero::<u128>::MIN;
static MAX_NZ_U128: NonZero<u128> = NonZero::<u128>::MAX;

static MIN_I128: i128 = i128::MIN;
static MAX_I128: i128 = i128::MAX;
static MIN_NZ_I128: NonZero<i128> = NonZero::<i128>::MIN;
static MAX_NZ_I128: NonZero<i128> = NonZero::<i128>::MAX;

static MIN_USIZE: usize = usize::MIN;
static MAX_USIZE: usize = usize::MAX;
static MIN_NZ_USIZE: NonZero<usize> = NonZero::<usize>::MIN;
static MAX_NZ_USIZE: NonZero<usize> = NonZero::<usize>::MAX;

static MIN_ISIZE: isize = isize::MIN;
static MAX_ISIZE: isize = isize::MAX;
static MIN_NZ_ISIZE: NonZero<isize> = NonZero::<isize>::MIN;
static MAX_NZ_ISIZE: NonZero<isize> = NonZero::<isize>::MAX;

impl_facet_for_integer!(
    u8,
    &const {
        ScalarAffinity::number()
            .unsigned_integer(8)
            .min(PtrConst::new(&raw const MIN_U8))
            .max(PtrConst::new(&raw const MAX_U8))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .unsigned_integer(8)
            .min(PtrConst::new(&raw const MIN_NZ_U8))
            .max(PtrConst::new(&raw const MAX_NZ_U8))
            .build()
    }
);

impl_facet_for_integer!(
    i8,
    &const {
        ScalarAffinity::number()
            .signed_integer(8)
            .min(PtrConst::new(&raw const MIN_I8))
            .max(PtrConst::new(&raw const MAX_I8))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .signed_integer(8)
            .min(PtrConst::new(&raw const MIN_NZ_I8))
            .max(PtrConst::new(&raw const MAX_NZ_I8))
            .build()
    }
);

impl_facet_for_integer!(
    u16,
    &const {
        ScalarAffinity::number()
            .unsigned_integer(16)
            .min(PtrConst::new(&raw const MIN_U16))
            .max(PtrConst::new(&raw const MAX_U16))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .unsigned_integer(16)
            .min(PtrConst::new(&raw const MIN_NZ_U16))
            .max(PtrConst::new(&raw const MAX_NZ_U16))
            .build()
    }
);

impl_facet_for_integer!(
    i16,
    &const {
        ScalarAffinity::number()
            .signed_integer(16)
            .min(PtrConst::new(&raw const MIN_I16))
            .max(PtrConst::new(&raw const MAX_I16))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .signed_integer(16)
            .min(PtrConst::new(&raw const MIN_NZ_I16))
            .max(PtrConst::new(&raw const MAX_NZ_I16))
            .build()
    }
);

impl_facet_for_integer!(
    u32,
    &const {
        ScalarAffinity::number()
            .unsigned_integer(32)
            .min(PtrConst::new(&raw const MIN_U32))
            .max(PtrConst::new(&raw const MAX_U32))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .unsigned_integer(32)
            .min(PtrConst::new(&raw const MIN_NZ_U32))
            .max(PtrConst::new(&raw const MAX_NZ_U32))
            .build()
    }
);

impl_facet_for_integer!(
    i32,
    &const {
        ScalarAffinity::number()
            .signed_integer(32)
            .min(PtrConst::new(&raw const MIN_I32))
            .max(PtrConst::new(&raw const MAX_I32))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .signed_integer(32)
            .min(PtrConst::new(&raw const MIN_NZ_I32))
            .max(PtrConst::new(&raw const MAX_NZ_I32))
            .build()
    }
);

impl_facet_for_integer!(
    u64,
    &const {
        ScalarAffinity::number()
            .unsigned_integer(64)
            .min(PtrConst::new(&raw const MIN_U64))
            .max(PtrConst::new(&raw const MAX_U64))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .unsigned_integer(64)
            .min(PtrConst::new(&raw const MIN_NZ_U64))
            .max(PtrConst::new(&raw const MAX_NZ_U64))
            .build()
    }
);

impl_facet_for_integer!(
    i64,
    &const {
        ScalarAffinity::number()
            .signed_integer(64)
            .min(PtrConst::new(&raw const MIN_I64))
            .max(PtrConst::new(&raw const MAX_I64))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .signed_integer(64)
            .min(PtrConst::new(&raw const MIN_NZ_I64))
            .max(PtrConst::new(&raw const MAX_NZ_I64))
            .build()
    }
);

impl_facet_for_integer!(
    u128,
    &const {
        ScalarAffinity::number()
            .unsigned_integer(128)
            .min(PtrConst::new(&raw const MIN_U128))
            .max(PtrConst::new(&raw const MAX_U128))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .unsigned_integer(128)
            .min(PtrConst::new(&raw const MIN_NZ_U128))
            .max(PtrConst::new(&raw const MAX_NZ_U128))
            .build()
    }
);

impl_facet_for_integer!(
    i128,
    &const {
        ScalarAffinity::number()
            .signed_integer(128)
            .min(PtrConst::new(&raw const MIN_I128))
            .max(PtrConst::new(&raw const MAX_I128))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .signed_integer(128)
            .min(PtrConst::new(&raw const MIN_NZ_I128))
            .max(PtrConst::new(&raw const MAX_NZ_I128))
            .build()
    }
);

impl_facet_for_integer!(
    usize,
    &const {
        ScalarAffinity::number()
            .unsigned_integer(core::mem::size_of::<usize>() * 8)
            .min(PtrConst::new(&raw const MIN_USIZE))
            .max(PtrConst::new(&raw const MAX_USIZE))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .unsigned_integer(core::mem::size_of::<usize>() * 8)
            .min(PtrConst::new(&raw const MIN_NZ_USIZE))
            .max(PtrConst::new(&raw const MAX_NZ_USIZE))
            .build()
    }
);

impl_facet_for_integer!(
    isize,
    &const {
        ScalarAffinity::number()
            .signed_integer(core::mem::size_of::<isize>() * 8)
            .min(PtrConst::new(&raw const MIN_ISIZE))
            .max(PtrConst::new(&raw const MAX_ISIZE))
            .build()
    },
    &const {
        ScalarAffinity::number()
            .signed_integer(core::mem::size_of::<isize>() * 8)
            .min(PtrConst::new(&raw const MIN_NZ_ISIZE))
            .max(PtrConst::new(&raw const MAX_NZ_ISIZE))
            .build()
    }
);
// Constants for f32
static MIN_F32: f32 = f32::MIN;
static MAX_F32: f32 = f32::MAX;
static POSITIVE_INFINITY_F32: f32 = f32::INFINITY;
static NEGATIVE_INFINITY_F32: f32 = f32::NEG_INFINITY;
static NAN_F32: f32 = f32::NAN;
static POSITIVE_ZERO_F32: f32 = 0.0f32;
static NEGATIVE_ZERO_F32: f32 = -0.0f32;
static EPSILON_F32: f32 = f32::EPSILON;

// Constants for f64
static MIN_F64: f64 = f64::MIN;
static MAX_F64: f64 = f64::MAX;
static POSITIVE_INFINITY_F64: f64 = f64::INFINITY;
static NEGATIVE_INFINITY_F64: f64 = f64::NEG_INFINITY;
static NAN_F64: f64 = f64::NAN;
static POSITIVE_ZERO_F64: f64 = 0.0f64;
static NEGATIVE_ZERO_F64: f64 = -0.0f64;
static EPSILON_F64: f64 = f64::EPSILON;

unsafe impl Facet<'_> for f32 {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable =
            value_vtable!(f32, |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier));

        vtable.try_from = || {
            Some(|source, source_shape, dest| {
                if source_shape == Self::SHAPE {
                    return Ok(unsafe { dest.copy_from(source, source_shape)? });
                }
                if source_shape == u64::SHAPE {
                    let value: u64 = *unsafe { source.get::<u64>() };
                    let converted: f32 = value as f32;
                    return Ok(unsafe { dest.put::<f32>(converted) });
                }
                if source_shape == i64::SHAPE {
                    let value: i64 = *unsafe { source.get::<i64>() };
                    let converted: f32 = value as f32;
                    return Ok(unsafe { dest.put::<f32>(converted) });
                }
                if source_shape == f64::SHAPE {
                    let value: f64 = *unsafe { source.get::<f64>() };
                    let converted: f32 = value as f32;
                    return Ok(unsafe { dest.put::<f32>(converted) });
                }
                Err(TryFromError::UnsupportedSourceShape {
                    src_shape: source_shape,
                    expected: &[Self::SHAPE, u64::SHAPE, i64::SHAPE, f64::SHAPE],
                })
            })
        };

        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("f32")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(
                        &const {
                            ScalarAffinity::number()
                                .float(1, 8, f32::MANTISSA_DIGITS as usize - 1, false)
                                .min(PtrConst::new(&raw const MIN_F32))
                                .max(PtrConst::new(&raw const MAX_F32))
                                .positive_infinity(PtrConst::new(&raw const POSITIVE_INFINITY_F32))
                                .negative_infinity(PtrConst::new(&raw const NEGATIVE_INFINITY_F32))
                                .nan_sample(PtrConst::new(&raw const NAN_F32))
                                .positive_zero(PtrConst::new(&raw const POSITIVE_ZERO_F32))
                                .negative_zero(PtrConst::new(&raw const NEGATIVE_ZERO_F32))
                                .epsilon(PtrConst::new(&raw const EPSILON_F32))
                                .build()
                        },
                    )
                    .build(),
            ))
            .build()
    };
}

unsafe impl Facet<'_> for f64 {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable =
            value_vtable!(f64, |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier));

        vtable.try_from = || {
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

        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("f64")
            .ty(Type::Primitive(PrimitiveType::Numeric(NumericType::Float)))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(
                        &const {
                            ScalarAffinity::number()
                                .float(1, 11, f64::MANTISSA_DIGITS as usize - 1, false)
                                .min(PtrConst::new(&raw const MIN_F64))
                                .max(PtrConst::new(&raw const MAX_F64))
                                .positive_infinity(PtrConst::new(&raw const POSITIVE_INFINITY_F64))
                                .negative_infinity(PtrConst::new(&raw const NEGATIVE_INFINITY_F64))
                                .nan_sample(PtrConst::new(&raw const NAN_F64))
                                .positive_zero(PtrConst::new(&raw const POSITIVE_ZERO_F64))
                                .negative_zero(PtrConst::new(&raw const NEGATIVE_ZERO_F64))
                                .epsilon(PtrConst::new(&raw const EPSILON_F64))
                                .build()
                        },
                    )
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::socket_addr().build() })
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::ip_addr().build() })
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::ip_addr().build() })
                    .build(),
            ))
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
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::ip_addr().build() })
                    .build(),
            ))
            .build()
    };
}
