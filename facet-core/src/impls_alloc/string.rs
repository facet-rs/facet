use crate::{Def, Facet, Shape, Type, UserType, ValueVTable, value_vtable};
use alloc::string::ToString;

#[cfg(feature = "alloc")]
unsafe impl Facet<'_> for alloc::string::String {
    const VTABLE: &'static ValueVTable = &const {
        let mut vtable = value_vtable!(alloc::string::String, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        ));

        // Add a custom parse function for String
        {
            let vtable_sized = vtable.sized_mut().unwrap();
            vtable_sized.parse = || {
                Some(|s, target| {
                    // For String, parsing from a string is just copying the string
                    Ok(unsafe { target.put(s.to_string()) })
                })
            };
        }

        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .def(Def::Scalar)
            .type_identifier("String")
            .ty(Type::User(UserType::Opaque))
            .build()
    };
}

unsafe impl<'a> Facet<'a> for alloc::borrow::Cow<'a, str> {
    const VTABLE: &'static ValueVTable = &const {
        value_vtable!(alloc::borrow::Cow<'_, str>, |f, _opts| write!(
            f,
            "Cow<'_, str>"
        ))
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .def(Def::Scalar)
            .type_identifier("Cow")
            .ty(Type::User(UserType::Opaque))
            .build()
    };
}

#[cfg(test)]
mod tests {
    use crate::Facet;
    use crate::ptr::PtrUninit;
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
        let parse_fn = (shape.vtable.sized().unwrap().parse)().unwrap();

        // Allocate memory for the String
        let layout = shape.layout.sized_layout().unwrap();
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        let uninit = PtrUninit::new(ptr);

        // Parse the string
        let result = unsafe { parse_fn("hello world", uninit) };
        assert!(result.is_ok());

        // Get the parsed value
        let ptr_mut = result.unwrap();
        let parsed = unsafe { ptr_mut.get::<String>() };
        assert_eq!(parsed, &String::from("hello world"));

        // Clean up
        unsafe {
            ptr_mut.drop_in_place::<String>();
            alloc::alloc::dealloc(ptr, layout);
        }
    }
}
