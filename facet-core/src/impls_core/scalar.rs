use crate::value_vtable;
use crate::*;
use core::alloc::Layout;
use core::num::NonZero;
use typeid::ConstTypeId;

unsafe impl Facet for ConstTypeId {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::opaque().build())
                    .build(),
            ))
            .vtable(value_vtable!((), |f, _opts| write!(f, "ConstTypeId")))
            .build()
    };
}

unsafe impl Facet for () {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::empty().build())
                    .build(),
            ))
            .vtable(value_vtable!((), |f, _opts| write!(f, "()")))
            .build()
    };
}

unsafe impl<T: ?Sized> Facet for core::marker::PhantomData<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::empty().build())
                    .build(),
            ))
            // TODO: we might be able to do something with specialization re: the shape of T?
            .vtable(value_vtable!((), |f, _opts| write!(f, "PhantomData")))
            .build()
    };
}

#[cfg(feature = "alloc")]
unsafe impl Facet for alloc::string::String {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    // `String` is always on the heap
                    .affinity(ScalarAffinity::string().max_inline_length(0).build())
                    .build(),
            ))
            .vtable(value_vtable!(alloc::string::String, |f, _opts| write!(
                f,
                "String"
            )))
            .build()
    };
}

unsafe impl Facet for char {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::char().build())
                    .build(),
            ))
            .vtable(value_vtable!(char, |f, _opts| write!(f, "char")))
            .build()
    };
}

unsafe impl Facet for &str {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::string().build())
                    .build(),
            ))
            .vtable(value_vtable!(&str, |f, _opts| write!(f, "&str")))
            .build()
    };
}

#[cfg(feature = "alloc")]
unsafe impl Facet for alloc::borrow::Cow<'_, str> {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::string().build())
                    .build(),
            ))
            .vtable(value_vtable!(
                alloc::borrow::Cow<'_, str>,
                |f, _opts| write!(f, "Cow<'_, str>")
            ))
            .build()
    };
}

unsafe impl Facet for bool {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::boolean().build())
                    .build(),
            ))
            .vtable(value_vtable!(bool, |f, _opts| write!(f, "bool")))
            .build()
    };
}

macro_rules! impl_facet_for_integer {
    ($type:ty, $affinity:expr, $nz_affinity:expr) => {
        unsafe impl Facet for $type {
            const SHAPE: &'static Shape = &const {
                Shape::builder()
                    .id(ConstTypeId::of::<Self>())
                    .layout(Layout::new::<Self>())
                    .def(Def::Scalar(
                        ScalarDef::builder().affinity($affinity).build(),
                    ))
                    .vtable(
                        &const {
                            let mut vtable = value_vtable_inner!($type, |f, _opts| write!(
                                f,
                                "{}",
                                stringify!($type)
                            ));

                            vtable.try_from = Some(|source, source_shape, dest| {
                                if source_shape == Self::SHAPE {
                                    return Ok(unsafe { dest.copy_from(source, source_shape) });
                                }
                                if source_shape == u64::SHAPE {
                                    let value: u64 = *unsafe { source.get::<u64>() };
                                    let converted: $type = value as $type;
                                    return Ok(unsafe { dest.put::<$type>(converted) });
                                }
                                Err(TryFromError::Incompatible)
                            });

                            vtable
                        },
                    )
                    .build()
            };
        }

        unsafe impl Facet for NonZero<$type> {
            const SHAPE: &'static Shape = &const {
                Shape::builder()
                    .id(ConstTypeId::of::<Self>())
                    .layout(Layout::new::<Self>())
                    .def(Def::Scalar(
                        ScalarDef::builder().affinity($nz_affinity).build(),
                    ))
                    .vtable(
                        &const {
                            let mut vtable = value_vtable_inner!($type, |f, _opts| write!(
                                f,
                                "NonZero<{}>",
                                stringify!($type)
                            ));

                            vtable.try_from = Some(|source, source_shape, dest| {
                                if source_shape == Self::SHAPE {
                                    return Ok(unsafe { dest.copy_from(source, source_shape) });
                                }
                                if source_shape == <$type>::SHAPE {
                                    let value: $type = *unsafe { source.get::<$type>() };
                                    let nz = NonZero::new(value).ok_or_else(|| {
                                        TryFromError::Generic("value must be non-zero")
                                    })?;
                                    return Ok(unsafe { dest.put::<NonZero<$type>>(nz) });
                                }
                                Err(TryFromError::Incompatible)
                            });

                            vtable
                        },
                    )
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
    ScalarAffinity::number()
        .unsigned_integer(8)
        .min(PtrConst::new(&raw const MIN_U8))
        .max(PtrConst::new(&raw const MAX_U8))
        .build(),
    ScalarAffinity::number()
        .unsigned_integer(8)
        .min(PtrConst::new(&raw const MIN_NZ_U8))
        .max(PtrConst::new(&raw const MAX_NZ_U8))
        .build()
);

impl_facet_for_integer!(
    i8,
    ScalarAffinity::number()
        .signed_integer(8)
        .min(PtrConst::new(&raw const MIN_I8))
        .max(PtrConst::new(&raw const MAX_I8))
        .build(),
    ScalarAffinity::number()
        .signed_integer(8)
        .min(PtrConst::new(&raw const MIN_NZ_I8))
        .max(PtrConst::new(&raw const MAX_NZ_I8))
        .build()
);

impl_facet_for_integer!(
    u16,
    ScalarAffinity::number()
        .unsigned_integer(16)
        .min(PtrConst::new(&raw const MIN_U16))
        .max(PtrConst::new(&raw const MAX_U16))
        .build(),
    ScalarAffinity::number()
        .unsigned_integer(16)
        .min(PtrConst::new(&raw const MIN_NZ_U16))
        .max(PtrConst::new(&raw const MAX_NZ_U16))
        .build()
);

impl_facet_for_integer!(
    i16,
    ScalarAffinity::number()
        .signed_integer(16)
        .min(PtrConst::new(&raw const MIN_I16))
        .max(PtrConst::new(&raw const MAX_I16))
        .build(),
    ScalarAffinity::number()
        .signed_integer(16)
        .min(PtrConst::new(&raw const MIN_NZ_I16))
        .max(PtrConst::new(&raw const MAX_NZ_I16))
        .build()
);

impl_facet_for_integer!(
    u32,
    ScalarAffinity::number()
        .unsigned_integer(32)
        .min(PtrConst::new(&raw const MIN_U32))
        .max(PtrConst::new(&raw const MAX_U32))
        .build(),
    ScalarAffinity::number()
        .unsigned_integer(32)
        .min(PtrConst::new(&raw const MIN_NZ_U32))
        .max(PtrConst::new(&raw const MAX_NZ_U32))
        .build()
);

impl_facet_for_integer!(
    i32,
    ScalarAffinity::number()
        .signed_integer(32)
        .min(PtrConst::new(&raw const MIN_I32))
        .max(PtrConst::new(&raw const MAX_I32))
        .build(),
    ScalarAffinity::number()
        .signed_integer(32)
        .min(PtrConst::new(&raw const MIN_NZ_I32))
        .max(PtrConst::new(&raw const MAX_NZ_I32))
        .build()
);

impl_facet_for_integer!(
    u64,
    ScalarAffinity::number()
        .unsigned_integer(64)
        .min(PtrConst::new(&raw const MIN_U64))
        .max(PtrConst::new(&raw const MAX_U64))
        .build(),
    ScalarAffinity::number()
        .unsigned_integer(64)
        .min(PtrConst::new(&raw const MIN_NZ_U64))
        .max(PtrConst::new(&raw const MAX_NZ_U64))
        .build()
);

impl_facet_for_integer!(
    i64,
    ScalarAffinity::number()
        .signed_integer(64)
        .min(PtrConst::new(&raw const MIN_I64))
        .max(PtrConst::new(&raw const MAX_I64))
        .build(),
    ScalarAffinity::number()
        .signed_integer(64)
        .min(PtrConst::new(&raw const MIN_NZ_I64))
        .max(PtrConst::new(&raw const MAX_NZ_I64))
        .build()
);

impl_facet_for_integer!(
    u128,
    ScalarAffinity::number()
        .unsigned_integer(128)
        .min(PtrConst::new(&raw const MIN_U128))
        .max(PtrConst::new(&raw const MAX_U128))
        .build(),
    ScalarAffinity::number()
        .unsigned_integer(128)
        .min(PtrConst::new(&raw const MIN_NZ_U128))
        .max(PtrConst::new(&raw const MAX_NZ_U128))
        .build()
);

impl_facet_for_integer!(
    i128,
    ScalarAffinity::number()
        .signed_integer(128)
        .min(PtrConst::new(&raw const MIN_I128))
        .max(PtrConst::new(&raw const MAX_I128))
        .build(),
    ScalarAffinity::number()
        .signed_integer(128)
        .min(PtrConst::new(&raw const MIN_NZ_I128))
        .max(PtrConst::new(&raw const MAX_NZ_I128))
        .build()
);

impl_facet_for_integer!(
    usize,
    ScalarAffinity::number()
        .unsigned_integer(core::mem::size_of::<usize>() * 8)
        .min(PtrConst::new(&raw const MIN_USIZE))
        .max(PtrConst::new(&raw const MAX_USIZE))
        .build(),
    ScalarAffinity::number()
        .unsigned_integer(core::mem::size_of::<usize>() * 8)
        .min(PtrConst::new(&raw const MIN_NZ_USIZE))
        .max(PtrConst::new(&raw const MAX_NZ_USIZE))
        .build()
);

impl_facet_for_integer!(
    isize,
    ScalarAffinity::number()
        .signed_integer(core::mem::size_of::<isize>() * 8)
        .min(PtrConst::new(&raw const MIN_ISIZE))
        .max(PtrConst::new(&raw const MAX_ISIZE))
        .build(),
    ScalarAffinity::number()
        .signed_integer(core::mem::size_of::<isize>() * 8)
        .min(PtrConst::new(&raw const MIN_NZ_ISIZE))
        .max(PtrConst::new(&raw const MAX_NZ_ISIZE))
        .build()
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

unsafe impl Facet for f32 {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<f32>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(
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
                            .build(),
                    )
                    .build(),
            ))
            .vtable(value_vtable!(f32, |f, _opts| write!(f, "f32")))
            .build()
    };
}

unsafe impl Facet for f64 {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<f64>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(
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
                            .build(),
                    )
                    .build(),
            ))
            .vtable(value_vtable!(f64, |f, _opts| write!(f, "f64")))
            .build()
    };
}

unsafe impl Facet for core::net::SocketAddr {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::socket_addr().build())
                    .build(),
            ))
            .vtable(value_vtable!(core::net::SocketAddr, |f, _opts| write!(
                f,
                "SocketAddr"
            )))
            .build()
    };
}

unsafe impl Facet for core::net::IpAddr {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::ip_addr().build())
                    .build(),
            ))
            .vtable(value_vtable!(core::net::IpAddr, |f, _opts| write!(
                f,
                "IpAddr"
            )))
            .build()
    };
}

unsafe impl Facet for core::net::Ipv4Addr {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::ip_addr().build())
                    .build(),
            ))
            .vtable(value_vtable!(core::net::Ipv4Addr, |f, _opts| write!(
                f,
                "Ipv4Addr"
            )))
            .build()
    };
}

unsafe impl Facet for core::net::Ipv6Addr {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::ip_addr().build())
                    .build(),
            ))
            .vtable(value_vtable!(core::net::Ipv6Addr, |f, _opts| write!(
                f,
                "Ipv6Addr"
            )))
            .build()
    };
}
