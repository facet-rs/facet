use crate::value_vtable;
use crate::*;
use core::alloc::Layout;

unsafe impl Facet for () {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder().fully_qualified_type_name("()").build(),
            ))
            .vtable(value_vtable!((), |f, _opts| write!(f, "()")))
            .build()
    };
}

unsafe impl Facet for String {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .fully_qualified_type_name("alloc::string::String")
                    .build(),
            ))
            .vtable(value_vtable!(String, |f, _opts| write!(f, "String")))
            .build()
    };
}

unsafe impl Facet for &str {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .fully_qualified_type_name("&core::primitive::str")
                    .build(),
            ))
            .vtable(value_vtable!(&str, |f, _opts| write!(f, "&str")))
            .build()
    };
}

// FIXME: That's wrong. This is an enum, so it should be treated as an enum.
#[cfg(feature = "std")]
unsafe impl Facet for std::borrow::Cow<'_, str> {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .fully_qualified_type_name("std::borrow::Cow<'_, str>")
                    .build(),
            ))
            .vtable(value_vtable!(std::borrow::Cow<'_, str>, |f, _opts| write!(
                f,
                "Cow<'_, str>"
            )))
            .build()
    };
}

unsafe impl Facet for bool {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .fully_qualified_type_name("core::primitive::bool")
                    .build(),
            ))
            .vtable(value_vtable!(bool, |f, _opts| write!(f, "bool")))
            .build()
    };
}

macro_rules! impl_facet_for_integer {
    ($type:ty) => {
        unsafe impl Facet for $type {
            const SHAPE: &'static Shape = &const {
                Shape::builder()
                    .layout(Layout::new::<Self>())
                    .def(Def::Scalar(
                        ScalarDef::builder()
                            .fully_qualified_type_name(stringify!($type))
                            .build(),
                    ))
                    .vtable(value_vtable!($type, |f, _opts| write!(
                        f,
                        stringify!($type)
                    )))
                    .build()
            };
        }
    };
}

impl_facet_for_integer!(core::primitive::u8);
impl_facet_for_integer!(core::primitive::i8);
impl_facet_for_integer!(core::primitive::u16);
impl_facet_for_integer!(core::primitive::i16);
impl_facet_for_integer!(core::primitive::u32);
impl_facet_for_integer!(core::primitive::i32);
impl_facet_for_integer!(core::primitive::u64);
impl_facet_for_integer!(core::primitive::i64);
impl_facet_for_integer!(core::primitive::u128);
impl_facet_for_integer!(core::primitive::i128);
impl_facet_for_integer!(core::primitive::usize);
impl_facet_for_integer!(core::primitive::isize);

macro_rules! impl_facet_for_float {
    ($type:ty) => {
        unsafe impl Facet for $type {
            const SHAPE: &'static Shape = &const {
                Shape::builder()
                    .layout(Layout::new::<Self>())
                    .def(Def::Scalar(
                        ScalarDef::builder()
                            .fully_qualified_type_name(stringify!($type))
                            .build(),
                    ))
                    .vtable(value_vtable!($type, |f, _opts| write!(
                        f,
                        stringify!($type)
                    )))
                    .build()
            };
        }
    };
}

impl_facet_for_float!(core::primitive::f32);
impl_facet_for_float!(core::primitive::f64);

#[cfg(feature = "std")]
unsafe impl Facet for std::net::SocketAddr {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .fully_qualified_type_name("std::net::SocketAddr")
                    .build(),
            ))
            .vtable(value_vtable!(std::net::SocketAddr, |f, _opts| write!(
                f,
                "SocketAddr"
            )))
            .build()
    };
}
