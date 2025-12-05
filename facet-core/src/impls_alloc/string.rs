use crate::{Facet, MarkerTraits, Shape, ShapeBuilder, ValueVTable};
use alloc::string::ToString;

#[cfg(feature = "alloc")]
unsafe impl Facet<'_> for alloc::string::String {
    // String implements: Display, Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<alloc::string::String>(|f, _opts| write!(f, "String"), "String")
            .drop_in_place(ValueVTable::drop_in_place_for::<alloc::string::String>())
            .default_in_place(|target| unsafe { target.put(alloc::string::String::new()) })
            .clone_into(|src, dst| unsafe { dst.put(src.get::<alloc::string::String>().clone()) })
            .parse(|s, target| {
                // For String, parsing from a string is just copying the string
                Ok(unsafe { target.put(s.to_string()) })
            })
            .display(|data, f| {
                let data = unsafe { data.get::<alloc::string::String>() };
                core::fmt::Display::fmt(data, f)
            })
            .debug(|data, f| {
                let data = unsafe { data.get::<alloc::string::String>() };
                core::fmt::Debug::fmt(data, f)
            })
            .partial_eq(|left, right| unsafe {
                *left.get::<alloc::string::String>() == *right.get::<alloc::string::String>()
            })
            .partial_ord(|left, right| unsafe {
                left.get::<alloc::string::String>()
                    .partial_cmp(right.get::<alloc::string::String>())
            })
            .ord(|left, right| unsafe {
                left.get::<alloc::string::String>()
                    .cmp(right.get::<alloc::string::String>())
            })
            .hash(|value, hasher| {
                use core::hash::Hash;
                let value = unsafe { value.get::<alloc::string::String>() };
                value.hash(&mut { hasher })
            })
            .markers(MarkerTraits::EMPTY.with_eq())
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::ptr::NonNull;

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
        let parse_fn = shape.vtable.parse.unwrap();

        // Allocate memory for the String
        let layout = shape.layout.sized_layout().unwrap();
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        let Some(ptr) = NonNull::new(ptr) else {
            alloc::alloc::handle_alloc_error(layout)
        };
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
            alloc::alloc::dealloc(ptr.as_ptr(), layout);
        }
    }
}
