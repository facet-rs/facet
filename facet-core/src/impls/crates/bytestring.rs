#![cfg(feature = "bytestring")]

use bytestring::ByteString;

use crate::{
    Def, Facet, OxPtrConst, OxPtrMut, ParseError, PtrConst, Shape, ShapeBuilder, Type, UserType,
    VTableIndirect,
};

/// Try to convert from &str or String to ByteString
///
/// # Safety
/// `dst` must be valid for writes, `src` must point to valid data of type described by `src_shape`
unsafe fn bytestring_try_from(
    target: OxPtrMut,
    src_shape: &'static Shape,
    src: PtrConst,
) -> Option<Result<(), alloc::string::String>> {
    unsafe {
        let dst = target.ptr.as_ptr::<ByteString>() as *mut ByteString;

        // Check if source is &str
        if src_shape.id == <&str as Facet>::SHAPE.id {
            let str_ref: &str = src.get::<&str>();
            // Use ptr::write to avoid dropping uninitialized memory
            dst.write(ByteString::from(str_ref));
            return Some(Ok(()));
        }

        // Check if source is String
        if src_shape.id == <alloc::string::String as Facet>::SHAPE.id {
            let string: alloc::string::String = src.read::<alloc::string::String>();
            // Use ptr::write to avoid dropping uninitialized memory
            dst.write(ByteString::from(string));
            return Some(Ok(()));
        }

        Some(Err(alloc::format!(
            "cannot convert {} to ByteString",
            src_shape.type_identifier
        )))
    }
}

/// Parse a string into ByteString (always succeeds since any valid UTF-8 is valid)
unsafe fn bytestring_parse(s: &str, target: OxPtrMut) -> Option<Result<(), ParseError>> {
    unsafe {
        let dst = target.ptr.as_ptr::<ByteString>() as *mut ByteString;
        // Use ptr::write to avoid dropping uninitialized memory
        dst.write(ByteString::from(s));
        Some(Ok(()))
    }
}

unsafe fn bytestring_display(
    source: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let bs = source.get::<ByteString>();
        Some(write!(f, "{bs}"))
    }
}

unsafe fn bytestring_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    unsafe {
        let a = a.get::<ByteString>();
        let b = b.get::<ByteString>();
        Some(a == b)
    }
}

const BYTESTRING_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(bytestring_display),
    try_from: Some(bytestring_try_from),
    parse: Some(bytestring_parse),
    partial_eq: Some(bytestring_partial_eq),
    ..VTableIndirect::EMPTY
};

unsafe impl Facet<'_> for ByteString {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<ByteString>("ByteString")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&BYTESTRING_VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
