use crate::{
    Def, Facet, Shape, ShapeBuilder, Type, TypeOpsDirect, UserType, VTableDirect, type_ops_direct,
    vtable_direct,
};

// TypeOps lifted out - shared static
static STRING_TYPE_OPS: TypeOpsDirect = type_ops_direct!(alloc::string::String => Default, Clone);

unsafe impl Facet<'_> for alloc::string::String {
    // String implements: Display, Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, FromStr
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(alloc::string::String =>
            FromStr,
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_sized::<alloc::string::String>("String")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .type_ops_direct(&STRING_TYPE_OPS)
            .eq()
            .send()
            .sync()
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::ptr::NonNull;

    use crate::Facet;
    use alloc::string::String;

    #[test]
    fn test_string_has_parse() {
        // Check that String has a parse function in its vtable
        let shape = String::SHAPE;
        assert!(
            shape.vtable.has_parse(),
            "String should have parse function"
        );
    }

    #[test]
    fn test_string_parse() {
        // Test that we can parse a string into a String
        let shape = String::SHAPE;

        // Allocate memory for the String
        let layout = shape.layout.sized_layout().unwrap();
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        let Some(ptr) = NonNull::new(ptr) else {
            alloc::alloc::handle_alloc_error(layout)
        };
        let ptr_mut = crate::PtrMut::new(ptr.as_ptr());

        // Parse the string using the new API
        let result = unsafe { shape.call_parse("hello world", ptr_mut) };
        assert!(result.is_some(), "String should have parse function");
        assert!(result.unwrap().is_ok());

        // Get the parsed value
        let parsed = unsafe { ptr_mut.get::<String>() };
        assert_eq!(parsed, &String::from("hello world"));

        // Clean up
        unsafe {
            shape.call_drop_in_place(ptr_mut).unwrap();
            alloc::alloc::dealloc(ptr.as_ptr(), layout);
        }
    }
}
