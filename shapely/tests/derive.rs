use shapely::Shapely;
use std::mem::offset_of;

#[test]
fn simple_struct() {
    #[derive(Debug, Shapely)]
    struct Blah {
        foo: u32,
        bar: String,
    }

    if !cfg!(miri) {
        let shape = Blah::shape();

        // Check the name using Display
        assert_eq!(format!("{}", shape), "Blah");

        assert_eq!(shape.layout.size(), 32);
        assert_eq!(shape.layout.align(), 8);

        if let shapely::Innards::Struct { fields } = shape.innards {
            assert_eq!(fields.len(), 2);

            let foo_field = &fields[0];
            assert_eq!(foo_field.name, "foo");
            assert_eq!(foo_field.shape.get().layout.size(), 4);
            assert_eq!(foo_field.shape.get().layout.align(), 4);
            assert_eq!(foo_field.offset, offset_of!(Blah, foo));

            let bar_field = &fields[1];
            assert_eq!(bar_field.name, "bar");
            assert_eq!(bar_field.shape.get().layout.size(), 24);
            assert_eq!(bar_field.shape.get().layout.align(), 8);
            assert_eq!(bar_field.offset, offset_of!(Blah, bar));
        } else {
            panic!("Expected Struct innards");
        }
    }
}
