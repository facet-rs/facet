use core::{fmt::Debug, mem::offset_of};
use facet::{Facet, FieldFlags, SequenceType, Shape, StructKind, StructType, Type, UserType};

#[test]
fn unit_struct() {
    #[derive(Debug, Facet)]
    struct UnitStruct;

    let shape = UnitStruct::SHAPE;

    // Check the name using Display
    assert_eq!(format!("{}", shape), "UnitStruct");

    let layout = shape.layout.sized_layout().unwrap();
    assert_eq!(layout.size(), 0);
    assert_eq!(layout.align(), 1);

    if let Type::User(UserType::Struct(StructType { kind, fields, .. })) = shape.ty {
        assert_eq!(kind, StructKind::Unit);
        assert_eq!(fields.len(), 0);
    } else {
        panic!("Expected Struct innards");
    }
}

#[test]
fn simple_struct() {
    #[derive(Debug, Facet)]
    struct Blah {
        foo: u32,
        bar: String,
    }

    if !cfg!(miri) {
        let shape = Blah::SHAPE;

        // Check the name using Display
        assert_eq!(format!("{}", shape), "Blah");

        let layout = shape.layout.sized_layout().unwrap();

        assert_eq!(layout.size(), 32);
        assert_eq!(layout.align(), 8);

        if let Type::User(UserType::Struct(StructType { kind, fields, .. })) = shape.ty {
            assert_eq!(kind, StructKind::Struct);
            assert_eq!(fields.len(), 2);

            let foo_field = &fields[0];
            assert_eq!(foo_field.name, "foo");

            let foo_layout = foo_field.shape().layout.sized_layout().unwrap();
            assert_eq!(foo_layout.size(), 4);
            assert_eq!(foo_layout.align(), 4);
            assert_eq!(foo_field.offset, offset_of!(Blah, foo));

            let bar_field = &fields[1];
            assert_eq!(bar_field.name, "bar");

            let bar_layout = bar_field.shape().layout.sized_layout().unwrap();
            assert_eq!(bar_layout.size(), 24);
            assert_eq!(bar_layout.align(), 8);
            assert_eq!(bar_field.offset, offset_of!(Blah, bar));
        } else {
            panic!("Expected Struct innards");
        }
    }
}

#[test]
fn struct_with_sensitive_field() {
    #[derive(Debug, Facet)]
    struct Blah {
        foo: u32,
        #[facet(sensitive)]
        bar: String,
    }

    if !cfg!(miri) {
        let shape = Blah::SHAPE;

        if let Type::User(UserType::Struct(StructType { fields, .. })) = shape.ty {
            let bar_field = &fields[1];
            assert_eq!(bar_field.name, "bar");
            match shape.ty {
                Type::User(UserType::Struct(struct_kind)) => {
                    assert!(!struct_kind.fields[0].flags.contains(FieldFlags::SENSITIVE));
                    assert!(struct_kind.fields[1].flags.contains(FieldFlags::SENSITIVE));
                }
                _ => panic!("Expected struct"),
            }
        } else {
            panic!("Expected Struct innards");
        }
    }
}

#[test]
fn struct_repr_c() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    #[repr(C)]
    struct Blah {
        foo: u32,
        bar: String,
    }
}

#[test]
fn struct_doc_comment() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// yes
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[" yes"]);
}

#[test]
fn struct_doc_comment2() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// yes
    /// no
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[" yes", " no"]);
}

#[test]
fn struct_doc_comment3() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// yes 😄
    /// no
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[" yes 😄", " no"]);
}

#[test]
fn struct_doc_comment4() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// what about "quotes"
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[r#" what about "quotes""#]);
}

#[test]
fn struct_field_doc_comment() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    struct Foo {
        /// This field has a doc comment
        bar: u32,
    }

    if let Type::User(UserType::Struct(StructType { fields, .. })) = Foo::SHAPE.ty {
        assert_eq!(fields[0].doc, &[" This field has a doc comment"]);
    } else {
        panic!("Expected Struct innards");
    }
}

#[test]
fn tuple_struct_field_doc_comment_test() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    struct MyTupleStruct(
        /// This is a documented field
        u32,
        /// This is another documented field
        String,
    );

    let shape = MyTupleStruct::SHAPE;

    if let Type::User(UserType::Struct(StructType { fields, kind, .. })) = shape.ty {
        assert_eq!(kind, StructKind::TupleStruct);
        assert_eq!(fields[0].doc, &[" This is a documented field"]);
        assert_eq!(fields[1].doc, &[" This is another documented field"]);
    } else {
        panic!("Expected Struct innards");
    }
}

#[test]
fn enum_variants_with_comments() {
    #[derive(Clone, Hash, PartialEq, Eq, Facet)]
    #[repr(u8)]
    enum CommentedEnum {
        /// This is variant A
        #[allow(dead_code)]
        A,
        /// This is variant B
        /// with multiple lines
        #[allow(dead_code)]
        B(u32),
        /// This is variant C
        /// which has named fields
        #[allow(dead_code)]
        C {
            /// This is field x
            x: u32,
            /// This is field y
            y: String,
        },
    }

    let shape = CommentedEnum::SHAPE;

    if let Type::User(UserType::Enum(enum_kind)) = shape.ty {
        assert_eq!(enum_kind.variants.len(), 3);

        // Check variant A
        let variant_a = &enum_kind.variants[0];
        assert_eq!(variant_a.name, "A");
        assert_eq!(variant_a.doc, &[" This is variant A"]);

        // Check variant B
        let variant_b = &enum_kind.variants[1];
        assert_eq!(variant_b.name, "B");
        assert_eq!(
            variant_b.doc,
            &[" This is variant B", " with multiple lines"]
        );

        // Check variant C
        let variant_c = &enum_kind.variants[2];
        assert_eq!(variant_c.name, "C");
        assert_eq!(
            variant_c.doc,
            &[" This is variant C", " which has named fields"]
        );

        // Check fields of variant C
        let fields = variant_c.data.fields;
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].doc, &[" This is field x"]);
        assert_eq!(fields[1].name, "y");
        assert_eq!(fields[1].doc, &[" This is field y"]);
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn struct_with_pub_field() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    struct Foo {
        /// This is a public field
        pub bar: u32,
    }
}

#[test]
fn tuple_struct_repr_transparent() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    #[repr(transparent)]
    struct Blah(u32);
}

#[test]
fn tuple_struct_doc_comment() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    #[repr(transparent)]
    /// This is a struct for sure
    struct Blah(u32);

    assert_eq!(Blah::SHAPE.doc, &[" This is a struct for sure"]);
}

#[test]
fn tuple_struct_field_doc_comment() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    #[repr(transparent)]
    /// This is a struct for sure
    struct Blah(
        /// and this is a field
        u32,
    );
}

#[test]
fn record_struct_generic() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    struct Blah<'a, T, const C: usize = 3>
    where
        T: core::hash::Hash,
    {
        field: core::marker::PhantomData<&'a T>,
    }
}

#[test]
fn tuple_struct_generic() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    #[repr(transparent)]
    struct Blah<'a, T, const C: usize = 3>(T, core::marker::PhantomData<&'a ()>)
    where
        T: core::hash::Hash;
}

#[test]
fn unit_struct_generic() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    struct Blah<const C: usize = 3>
    where
        (): core::hash::Hash;
}

#[test]
fn enum_generic() {
    #[allow(dead_code)]
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    #[repr(u8)]
    enum E<'a, T, const C: usize = 3>
    where
        T: core::hash::Hash,
    {
        Unit,
        Tuple(T, core::marker::PhantomData<&'a ()>),
        Record {
            field: T,
            phantom: core::marker::PhantomData<&'a ()>,
        },
    }
}

#[test]
fn enum_generic_partial() {
    #[allow(dead_code)]
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    #[repr(u8)]
    enum E<'a, T, const C: usize = 3>
    where
        T: core::hash::Hash,
    {
        Unit,
        Tuple(i32),
        Record {
            field: T,
            phantom: core::marker::PhantomData<&'a ()>,
        },
    }
}

#[test]
fn tuple_struct_with_pub_field() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// This is a struct for sure
    struct Blah(
        /// and this is a public field
        pub u32,
        /// and this is a crate public field
        pub(crate) u32,
    );
}

#[test]
fn cfg_attrs() {
    #[derive(Facet)]
    #[cfg_attr(feature = "testfeat", derive(Debug))]
    #[cfg_attr(feature = "testfeat", facet(deny_unknown_fields))]
    pub struct CubConfig {}
}

#[test]
fn struct_with_std_string() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    struct FileInfo {
        path: std::string::String,
        size: u64,
    }
}

#[test]
fn macroed_type() {
    fn validate_shape(shape: &Shape) {
        match shape.ty {
            Type::User(UserType::Struct(sk)) => {
                assert_eq!(sk.fields.len(), 1);
                let field = sk.fields[0];
                let shape_name = format!("{}", field.shape());
                assert_eq!(shape_name, "u32");
                eprintln!("Shape {shape} looks correct");
            }
            _ => unreachable!(),
        }
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Manual {
        // NOTICE type is variable here
        value: u32,
    }
    validate_shape(Manual::SHAPE);

    macro_rules! declare_struct {
        ($type:ty) => {
            #[derive(Debug, Facet, PartialEq)]
            struct Macroed {
                // NOTICE type is variable here
                value: $type,
            }
        };
    }

    declare_struct!(u32);
    validate_shape(Macroed::SHAPE);
}

#[test]
#[allow(dead_code)]
fn array_field() {
    /// Network packet types
    #[derive(Facet)]
    #[repr(u8)]
    pub enum Packet {
        /// Array of bytes representing the header
        Header([u8; 4]),
    }

    let shape = Packet::SHAPE;
    match shape.ty {
        Type::User(UserType::Enum(e)) => {
            let variant = &e.variants[0];
            let fields = &variant.data.fields;
            let field = &fields[0];
            match field.shape().ty {
                Type::Sequence(SequenceType::Array(ak)) => {
                    assert_eq!(ak.n, 4);
                    eprintln!("Shape {shape} looks correct");
                }
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}

#[test]
fn struct_impls_drop() {
    #[derive(Facet)]
    struct BarFoo {
        bar: u32,
        foo: String,
    }

    // this makes it impossible to "partially move out" of barfoo, see
    // code below. it's the reason why `shape_of` takes a &TStruct and returns a &TField.
    impl Drop for BarFoo {
        fn drop(&mut self) {
            eprintln!("Dropping BarFoo");
        }
    }

    // let bf = BarFoo {
    //     bar: 42,
    //     foo: "Hello".to_string(),
    // };
    // let bar = bf.bar;
    // drop(bf.foo);
}

#[test]
fn opaque_arc() {
    #[allow(dead_code)]
    pub struct NotDerivingFacet(u64);

    #[derive(Facet)]
    pub struct Handle(#[facet(opaque)] std::sync::Arc<NotDerivingFacet>);

    let shape = Handle::SHAPE;
    match shape.ty {
        Type::User(UserType::Struct(sk)) => {
            assert_eq!(sk.fields.len(), 1);
            let field = sk.fields[0];
            let shape_name = format!("{}", field.shape());
            assert_eq!(shape_name, "Opaque");
            eprintln!("Shape {shape} looks correct");
        }
        _ => unreachable!(),
    }
}

#[test]
fn enum_rename_all_snake_case() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[facet(rename_all = "snake_case")]
    #[allow(dead_code)]
    enum MaybeFontStyle {
        Regular,
        Italic,
        Bold,
    }

    let shape = MaybeFontStyle::SHAPE;

    assert_eq!(format!("{}", shape), "MaybeFontStyle");

    if let Type::User(UserType::Enum(enum_kind)) = shape.ty {
        assert_eq!(enum_kind.variants.len(), 3);

        assert_eq!(enum_kind.variants[0].name, "regular");
        assert_eq!(enum_kind.variants[1].name, "italic");
        assert_eq!(enum_kind.variants[2].name, "bold");

        for variant in enum_kind.variants {
            assert_eq!(variant.data.fields.len(), 0);
        }
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn core_ops_range() {
    let shape = core::ops::Range::<usize>::SHAPE;
    let Type::User(UserType::Struct(struct_type)) = shape.ty else {
        panic!("expected struct type");
    };

    assert_eq!(shape.type_params.len(), 1);
    assert_eq!(shape.type_params[0].name, "Idx");
    assert_eq!(shape.type_params[0].shape(), usize::SHAPE);

    assert_eq!(struct_type.fields.len(), 2);
    assert_eq!(struct_type.fields[0].name, "start");
    assert_eq!(struct_type.fields[0].shape(), usize::SHAPE);
    assert_eq!(
        struct_type.fields[0].offset,
        offset_of!(core::ops::Range::<usize>, start)
    );

    assert_eq!(struct_type.fields[1].name, "end");
    assert_eq!(struct_type.fields[1].shape(), usize::SHAPE);
    assert_eq!(
        struct_type.fields[1].offset,
        offset_of!(core::ops::Range::<usize>, end)
    );
}

#[test]
fn struct_with_default_field_that_has_lifetime() {
    #[derive(Facet)]
    struct Foo<'a> {
        #[facet(default)]
        name: Option<std::borrow::Cow<'a, str>>,
    }
}

#[test]
fn plain_tuple() {
    let _value = (42, "hello", true);
    let shape = <(i32, &str, bool) as Facet>::SHAPE;

    // Verify it's a struct with Tuple kind
    match shape.ty {
        Type::User(UserType::Struct(s)) => {
            assert_eq!(s.kind, StructKind::Tuple);
            assert_eq!(s.fields.len(), 3);

            assert_eq!(s.fields[0].name, "0");
            assert_eq!(s.fields[1].name, "1");
            assert_eq!(s.fields[2].name, "2");
        }
        _ => panic!("Expected tuple to be a UserType::Struct"),
    }
}

#[test]
fn test_macro_u16() {
    macro_rules! test_macro_u16 {
        () => {
            242u16
        };
    }

    const CONST_VALUE_U16: u16 = 142;

    #[repr(u16)]
    #[derive(Facet)]
    #[allow(dead_code)]
    enum TestEnum {
        Value1 = 42u16,
        Value2 = CONST_VALUE_U16,
        Value3 = test_macro_u16!(),
    }
}
