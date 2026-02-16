use core::{fmt::Debug, mem::offset_of};
use facet::{Facet, SequenceType, Shape, StructKind, StructType, Type, UserType};

#[test]
fn unit_struct() {
    #[derive(Debug, Facet)]
    struct UnitStruct;

    let shape = UnitStruct::SHAPE;

    // Check the name using Display
    assert_eq!(format!("{shape}"), "UnitStruct");

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
        assert_eq!(format!("{shape}"), "Blah");

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
                    assert!(!struct_kind.fields[0].is_sensitive());
                    assert!(struct_kind.fields[1].is_sensitive());
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
#[cfg(feature = "doc")]
fn struct_doc_comment() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// yes
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[" yes"]);
}

#[test]
#[cfg(feature = "doc")]
fn struct_doc_comment2() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// yes
    /// no
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[" yes", " no"]);
}

#[test]
#[cfg(feature = "doc")]
fn struct_doc_comment3() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// yes 😄
    /// no
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[" yes 😄", " no"]);
}

#[test]
#[cfg(feature = "doc")]
fn struct_doc_comment4() {
    #[derive(Clone, Hash, PartialEq, Eq, ::facet::Facet)]
    /// what about "quotes"
    struct Foo {}

    assert_eq!(Foo::SHAPE.doc, &[r#" what about "quotes""#]);
}

#[test]
#[cfg(feature = "doc")]
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
#[cfg(feature = "doc")]
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
#[cfg(feature = "doc")]
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
#[cfg(feature = "doc")]
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

    assert_eq!(format!("{shape}"), "MaybeFontStyle");

    if let Type::User(UserType::Enum(enum_kind)) = shape.ty {
        assert_eq!(enum_kind.variants.len(), 3);

        assert_eq!(enum_kind.variants[0].effective_name(), "regular");
        assert_eq!(enum_kind.variants[1].effective_name(), "italic");
        assert_eq!(enum_kind.variants[2].effective_name(), "bold");

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

#[test]
fn tests_enum_empty_tuple() {
    #[repr(u16)]
    #[derive(Facet)]
    #[allow(dead_code)]
    enum TestEnum {
        EmptyTuple(),
    }
}

#[test]
fn test_skip_field_in_derive() {
    #[allow(dead_code)]
    struct NotFacet {
        foo: i32,
        bar: String,
    }

    #[derive(Facet)]
    struct Foo {
        #[facet(opaque)]
        name: NotFacet,
    }
}

#[test]
fn test_transparent_newtype() {
    #[derive(Facet)]
    #[facet(transparent)]
    struct UserId(u32);

    let shape = UserId::SHAPE;

    // Check the name using Display
    assert_eq!(format!("{shape}"), "UserId");

    let layout = shape.layout.sized_layout().unwrap();
    assert_eq!(layout.size(), 4);
    assert_eq!(layout.align(), 4);

    if let Type::User(UserType::Struct(StructType { kind, fields, .. })) = shape.ty {
        assert_eq!(kind, StructKind::TupleStruct);
        assert_eq!(fields.len(), 1);

        let field = &fields[0];
        assert_eq!(field.name, "0");

        let field_layout = field.shape().layout.sized_layout().unwrap();
        assert_eq!(field_layout.size(), 4);
        assert_eq!(field_layout.align(), 4);
        assert_eq!(field.offset, offset_of!(UserId, 0));
    } else {
        panic!("Expected Struct innards");
    }
}

// ============================================================================
// Enum representation attribute tests
// ============================================================================

#[test]
fn enum_untagged() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum StringOrInt {
        Int(i64),
        String(String),
    }

    let shape = StringOrInt::SHAPE;
    assert!(shape.is_untagged());
    assert!(shape.get_tag_attr().is_none());
    assert!(shape.get_content_attr().is_none());
}

#[test]
fn enum_internally_tagged() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[facet(tag = "type")]
    #[allow(dead_code)]
    enum Message {
        Request { id: String },
        Response { id: String },
    }

    let shape = Message::SHAPE;
    assert!(!shape.is_untagged());
    assert_eq!(shape.get_tag_attr(), Some("type"));
    assert!(shape.get_content_attr().is_none());
}

#[test]
fn enum_adjacently_tagged() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[facet(tag = "t", content = "c")]
    #[allow(dead_code)]
    enum Block {
        Para(String),
        Code(String),
    }

    let shape = Block::SHAPE;
    assert!(!shape.is_untagged());
    assert_eq!(shape.get_tag_attr(), Some("t"));
    assert_eq!(shape.get_content_attr(), Some("c"));
}

#[test]
fn enum_externally_tagged_default() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive,
    }

    let shape = Status::SHAPE;
    // Default is externally tagged - no attributes
    assert!(!shape.is_untagged());
    assert!(shape.get_tag_attr().is_none());
    assert!(shape.get_content_attr().is_none());
}

// ============================================================================
// Custom crate path tests
// ============================================================================

/// Tests that the `#[facet(crate = "...")]` attribute works correctly.
/// This allows re-exporters of facet (like plugcard) to use the derive macro
/// with their own path to the facet crate.
#[test]
fn struct_with_custom_crate_path() {
    // Use `::facet` explicitly via the crate attribute
    #[derive(Debug, Facet)]
    #[facet(crate = ::facet)]
    struct CustomPathStruct {
        foo: u32,
        bar: String,
    }

    let shape = CustomPathStruct::SHAPE;
    assert_eq!(format!("{shape}"), "CustomPathStruct");

    if let Type::User(UserType::Struct(StructType { kind, fields, .. })) = shape.ty {
        assert_eq!(kind, StructKind::Struct);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "foo");
        assert_eq!(fields[1].name, "bar");
    } else {
        panic!("Expected Struct innards");
    }
}

#[test]
fn enum_with_custom_crate_path() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[facet(crate = ::facet)]
    #[allow(dead_code)]
    enum CustomPathEnum {
        VariantA,
        VariantB(u32),
        VariantC { x: String },
    }

    let shape = CustomPathEnum::SHAPE;
    assert_eq!(format!("{shape}"), "CustomPathEnum");

    if let Type::User(UserType::Enum(enum_kind)) = shape.ty {
        assert_eq!(enum_kind.variants.len(), 3);
        assert_eq!(enum_kind.variants[0].name, "VariantA");
        assert_eq!(enum_kind.variants[1].name, "VariantB");
        assert_eq!(enum_kind.variants[2].name, "VariantC");
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn tuple_struct_with_custom_crate_path() {
    #[derive(Debug, Facet)]
    #[facet(crate = ::facet)]
    struct CustomPathTuple(u32, String);

    let shape = CustomPathTuple::SHAPE;
    assert_eq!(format!("{shape}"), "CustomPathTuple");

    if let Type::User(UserType::Struct(StructType { kind, fields, .. })) = shape.ty {
        assert_eq!(kind, StructKind::TupleStruct);
        assert_eq!(fields.len(), 2);
    } else {
        panic!("Expected Struct innards");
    }
}

#[test]
fn generic_struct_with_custom_crate_path() {
    #[derive(Debug, Facet)]
    #[facet(crate = ::facet)]
    struct GenericCustomPath<T: Debug> {
        value: T,
    }

    let shape = GenericCustomPath::<u32>::SHAPE;
    assert_eq!(format!("{shape}"), "GenericCustomPath<u32>");
}

#[test]
fn metadata_field_attribute() {
    // Test struct with metadata field
    #[derive(Debug, Facet)]
    struct Timestamped<T: Debug> {
        value: T,
        #[facet(metadata = timestamp)]
        created_at: u64,
    }

    let shape = Timestamped::<i32>::SHAPE;
    let Type::User(UserType::Struct(struct_type)) = shape.ty else {
        panic!("Expected struct type");
    };

    // Find the created_at field and verify it has metadata = "timestamp"
    let created_at_field = struct_type
        .fields
        .iter()
        .find(|f| f.name == "created_at")
        .expect("Should have created_at field");

    assert!(
        created_at_field.is_metadata(),
        "created_at field should be marked as metadata"
    );
    assert_eq!(
        created_at_field.metadata_kind(),
        Some("timestamp"),
        "created_at field should have metadata kind 'timestamp'"
    );

    // Verify the value field is NOT metadata
    let value_field = struct_type
        .fields
        .iter()
        .find(|f| f.name == "value")
        .expect("Should have value field");

    assert!(
        !value_field.is_metadata(),
        "value field should not be marked as metadata"
    );
}

#[test]
fn metadata_field_structural_hash() {
    use core::hash::Hasher;
    use facet_reflect::Peek;
    use std::hash::DefaultHasher;

    #[derive(Debug, Facet)]
    struct WithTimestamp {
        data: i32,
        #[facet(metadata = timestamp)]
        ts: u64,
    }

    // Two values with same data but different metadata
    let a = WithTimestamp { data: 42, ts: 1000 };
    let b = WithTimestamp { data: 42, ts: 9999 };

    // They should have the same structural hash (metadata is ignored)
    let mut hasher_a = DefaultHasher::new();
    Peek::new(&a).structural_hash(&mut hasher_a);
    let hash_a = hasher_a.finish();

    let mut hasher_b = DefaultHasher::new();
    Peek::new(&b).structural_hash(&mut hasher_b);
    let hash_b = hasher_b.finish();

    assert_eq!(
        hash_a, hash_b,
        "Values with same data but different metadata should have same structural hash"
    );

    // But different data should produce different hashes
    let c = WithTimestamp { data: 99, ts: 1000 };
    let mut hasher_c = DefaultHasher::new();
    Peek::new(&c).structural_hash(&mut hasher_c);
    let hash_c = hasher_c.finish();

    assert_ne!(
        hash_a, hash_c,
        "Values with different data should have different structural hash"
    );
}

// ============================================================================
// POD (Plain Old Data) attribute tests
// ============================================================================

#[test]
fn pod_struct_basic() {
    #[derive(Debug, Facet)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let shape = Point::SHAPE;
    assert!(shape.is_pod(), "Point should be marked as POD");
}

#[test]
fn pod_tuple_struct() {
    #[derive(Debug, Facet)]
    #[facet(pod)]
    struct Pair(i32, i32);

    let shape = Pair::SHAPE;
    assert!(shape.is_pod(), "Pair should be marked as POD");
}

#[test]
fn pod_enum() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[facet(pod)]
    #[allow(dead_code)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    let shape = Color::SHAPE;
    assert!(shape.is_pod(), "Color enum should be marked as POD");
}

#[test]
fn non_pod_struct() {
    #[derive(Debug, Facet)]
    struct NotPod {
        x: i32,
    }

    let shape = NotPod::SHAPE;
    assert!(!shape.is_pod(), "NotPod should not be marked as POD");
}

#[test]
fn primitives_are_pod() {
    assert!(u8::SHAPE.is_pod(), "u8 should be POD");
    assert!(u16::SHAPE.is_pod(), "u16 should be POD");
    assert!(u32::SHAPE.is_pod(), "u32 should be POD");
    assert!(u64::SHAPE.is_pod(), "u64 should be POD");
    assert!(i8::SHAPE.is_pod(), "i8 should be POD");
    assert!(i16::SHAPE.is_pod(), "i16 should be POD");
    assert!(i32::SHAPE.is_pod(), "i32 should be POD");
    assert!(i64::SHAPE.is_pod(), "i64 should be POD");
    assert!(f32::SHAPE.is_pod(), "f32 should be POD");
    assert!(f64::SHAPE.is_pod(), "f64 should be POD");
    assert!(bool::SHAPE.is_pod(), "bool should be POD");
    assert!(char::SHAPE.is_pod(), "char should be POD");
}

// Test for issue #1697 - generic transparent tuple struct
// This test verifies the fix works but requires T: 'static due to Rust's const eval limitations
// The original error "can't use generic parameters from outer item" is fixed by moving
// the try_borrow_inner function to an inherent impl
#[test]
fn transparent_generic_tuple_struct() {
    #[derive(Debug, Clone, PartialEq, Facet)]
    #[facet(transparent)]
    pub struct Document<T: 'static>(Vec<T>);

    let shape = Document::<i32>::SHAPE;
    assert_eq!(format!("{shape}"), "Document<i32>");

    // Verify it has the transparent inner shape
    assert!(shape.inner.is_some(), "Should have inner shape");
    let inner = shape.inner.unwrap();
    assert_eq!(format!("{inner}"), "Vec<i32>");
}

// Test for issue #1714 - module_path and source location in Shape
#[test]
fn shape_module_path_and_source_location() {
    #[derive(Debug, Facet)]
    struct TestModulePath {
        x: i32,
    }

    let shape = TestModulePath::SHAPE;

    // module_path should be set for derived types
    assert!(
        shape.module_path.is_some(),
        "module_path should be Some for derived types"
    );
    let module_path = shape.module_path.unwrap();
    // The module path should contain "derive" since this test is in derive.rs
    assert!(
        module_path.contains("derive"),
        "module_path should contain 'derive', got: {}",
        module_path
    );

    // Source location should be set when doc feature is enabled (which it is by default)
    assert!(
        shape.source_file.is_some(),
        "source_file should be Some for derived types"
    );
    let source_file = shape.source_file.unwrap();
    assert!(
        source_file.contains("derive.rs"),
        "source_file should contain 'derive.rs', got: {}",
        source_file
    );

    assert!(
        shape.source_line.is_some(),
        "source_line should be Some for derived types"
    );
    assert!(
        shape.source_column.is_some(),
        "source_column should be Some for derived types"
    );

    // Foreign types (like primitives) should have None for these fields
    let i32_shape = i32::SHAPE;
    assert!(
        i32_shape.module_path.is_none(),
        "module_path should be None for primitives"
    );
    assert!(
        i32_shape.source_file.is_none(),
        "source_file should be None for primitives"
    );
}

// Test module_path for enums
#[test]
fn enum_module_path_and_source_location() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum TestEnumPath {
        A,
        B,
    }

    let shape = TestEnumPath::SHAPE;

    assert!(
        shape.module_path.is_some(),
        "module_path should be Some for derived enums"
    );
    let module_path = shape.module_path.unwrap();
    assert!(
        module_path.contains("derive"),
        "module_path should contain 'derive', got: {}",
        module_path
    );

    assert!(
        shape.source_file.is_some(),
        "source_file should be Some for derived enums"
    );
}

// Test module_path for foreign (std/alloc/core) types
#[test]
fn foreign_type_module_paths() {
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

    // Vec - alloc::vec
    let vec_shape = Vec::<i32>::SHAPE;
    assert_eq!(
        vec_shape.module_path,
        Some("alloc::vec"),
        "Vec should have module_path 'alloc::vec'"
    );

    // String - alloc::string
    let string_shape = String::SHAPE;
    assert_eq!(
        string_shape.module_path,
        Some("alloc::string"),
        "String should have module_path 'alloc::string'"
    );

    // HashMap - std::collections::hash_map
    let hashmap_shape = HashMap::<String, i32>::SHAPE;
    assert_eq!(
        hashmap_shape.module_path,
        Some("std::collections::hash_map"),
        "HashMap should have module_path 'std::collections::hash_map'"
    );

    // HashSet - std::collections::hash_set
    let hashset_shape = HashSet::<String>::SHAPE;
    assert_eq!(
        hashset_shape.module_path,
        Some("std::collections::hash_set"),
        "HashSet should have module_path 'std::collections::hash_set'"
    );

    // BTreeMap - alloc::collections::btree_map
    let btreemap_shape = BTreeMap::<String, i32>::SHAPE;
    assert_eq!(
        btreemap_shape.module_path,
        Some("alloc::collections::btree_map"),
        "BTreeMap should have module_path 'alloc::collections::btree_map'"
    );

    // BTreeSet - alloc::collections::btree_set
    let btreeset_shape = BTreeSet::<String>::SHAPE;
    assert_eq!(
        btreeset_shape.module_path,
        Some("alloc::collections::btree_set"),
        "BTreeSet should have module_path 'alloc::collections::btree_set'"
    );

    // Option - core::option
    let option_shape = Option::<i32>::SHAPE;
    assert_eq!(
        option_shape.module_path,
        Some("core::option"),
        "Option should have module_path 'core::option'"
    );

    // Result - core::result
    let result_shape = Result::<i32, String>::SHAPE;
    assert_eq!(
        result_shape.module_path,
        Some("core::result"),
        "Result should have module_path 'core::result'"
    );

    // Box - alloc::boxed
    let box_shape = Box::<i32>::SHAPE;
    assert_eq!(
        box_shape.module_path,
        Some("alloc::boxed"),
        "Box should have module_path 'alloc::boxed'"
    );

    // Arc - alloc::sync
    let arc_shape = std::sync::Arc::<i32>::SHAPE;
    assert_eq!(
        arc_shape.module_path,
        Some("alloc::sync"),
        "Arc should have module_path 'alloc::sync'"
    );

    // Rc - alloc::rc
    let rc_shape = std::rc::Rc::<i32>::SHAPE;
    assert_eq!(
        rc_shape.module_path,
        Some("alloc::rc"),
        "Rc should have module_path 'alloc::rc'"
    );

    // PathBuf - std::path
    let pathbuf_shape = std::path::PathBuf::SHAPE;
    assert_eq!(
        pathbuf_shape.module_path,
        Some("std::path"),
        "PathBuf should have module_path 'std::path'"
    );

    // Cow - alloc::borrow
    let cow_shape = std::borrow::Cow::<str>::SHAPE;
    assert_eq!(
        cow_shape.module_path,
        Some("alloc::borrow"),
        "Cow should have module_path 'alloc::borrow'"
    );

    // All these foreign types should have None for source location
    assert!(
        vec_shape.source_file.is_none(),
        "Foreign types should not have source_file"
    );
    assert!(
        string_shape.source_line.is_none(),
        "Foreign types should not have source_line"
    );
}

/// Test that declaration IDs work correctly:
/// - Same generic type with different type params → different id, same decl_id
/// - Different types → different decl_id
/// - Macro-generated types from same invocation → different decl_id (due to type name)
#[test]
fn test_decl_id() {
    use facet::Facet;

    // Test 1: Generic types with different type params have different id but same decl_id
    #[derive(Facet)]
    struct Wrapper<T> {
        inner: T,
    }

    let wrapper_u32 = <Wrapper<u32>>::SHAPE;
    let wrapper_string = <Wrapper<String>>::SHAPE;

    // Different types → different id
    assert_ne!(
        wrapper_u32.id, wrapper_string.id,
        "Different instantiations should have different type ids"
    );

    // Same declaration → same decl_id
    assert_eq!(
        wrapper_u32.decl_id, wrapper_string.decl_id,
        "Same generic type with different params should have same decl_id"
    );

    // Test 2: Different types have different decl_id
    #[derive(Facet)]
    struct OtherWrapper<T> {
        value: T,
    }

    let other_u32 = <OtherWrapper<u32>>::SHAPE;

    assert_ne!(
        wrapper_u32.decl_id, other_u32.decl_id,
        "Different type declarations should have different decl_id"
    );

    // Test 3: Macro that generates two types from one invocation
    // Both should have different decl_id because the type name is included in the hash
    macro_rules! make_two {
        () => {
            #[derive(Facet)]
            struct MacroFoo<T> {
                inner: T,
            }

            #[derive(Facet)]
            struct MacroBar<T> {
                inner: T,
            }
        };
    }

    make_two!();

    let macro_foo = <MacroFoo<u32>>::SHAPE;
    let macro_bar = <MacroBar<u32>>::SHAPE;

    assert_ne!(
        macro_foo.decl_id, macro_bar.decl_id,
        "Types from same macro invocation should have different decl_id (type name differs)"
    );

    // Test 4: Same macro invoked twice should produce different decl_id
    // (because the source location differs)
    macro_rules! make_wrapper {
        ($name:ident) => {
            #[derive(Facet)]
            struct $name<T> {
                inner: T,
            }
        };
    }

    make_wrapper!(WrapperA);
    make_wrapper!(WrapperB);

    let wrapper_a = <WrapperA<u32>>::SHAPE;
    let wrapper_b = <WrapperB<u32>>::SHAPE;

    assert_ne!(
        wrapper_a.decl_id, wrapper_b.decl_id,
        "Different macro invocations should have different decl_id"
    );
}

// Test for #[facet(from_ref)] attribute - infallible constructor from reference
#[test]
fn from_ref_infallible() {
    use std::borrow::Cow;

    #[derive(Debug, PartialEq, Facet)]
    #[facet(from_ref = BorrowedData::from_ref)]
    struct BorrowedData<'a> {
        text: Cow<'a, str>,
    }

    impl<'a> BorrowedData<'a> {
        fn from_ref(s: &'a str) -> Self {
            BorrowedData {
                text: Cow::Borrowed(s),
            }
        }
    }

    let shape = BorrowedData::SHAPE;

    // The VTable should have try_from set
    assert!(
        shape.vtable.has_try_from(),
        "BorrowedData should have try_from in VTable"
    );

    // Test that try_from works by calling it manually
    // Pass a wide pointer to str directly (not a pointer to the &str on the stack)
    let src_shape = <&str>::SHAPE;
    let src_str: &str = "hello world";
    let src_ptr = facet::PtrConst::new(src_str);

    let mut uninit = core::mem::MaybeUninit::<BorrowedData<'_>>::uninit();
    let dst_ptr = facet::PtrUninit::from_maybe_uninit(&mut uninit);

    let outcome = unsafe { shape.call_try_from(src_shape, src_ptr, dst_ptr) };
    assert!(
        matches!(outcome, Some(facet::TryFromOutcome::Converted)),
        "try_from should succeed with matching source shape"
    );

    let result = unsafe { uninit.assume_init() };
    assert_eq!(result.text, "hello world");
}

// Test for #[facet(try_from_ref)] attribute - fallible constructor from reference
#[test]
fn try_from_ref_fallible() {
    #[derive(Debug, PartialEq, Facet)]
    #[facet(try_from_ref = AsStr::try_from_ref)]
    struct AsStr<'a> {
        value: &'a str,
    }

    impl<'a> AsStr<'a> {
        fn try_from_ref(s: &'a [u8]) -> Result<Self, std::str::Utf8Error> {
            Ok(AsStr {
                value: std::str::from_utf8(s)?,
            })
        }
    }

    let shape = AsStr::SHAPE;

    // The VTable should have try_from set
    assert!(
        shape.vtable.has_try_from(),
        "ParsedInt should have try_from in VTable"
    );

    // Test successful conversion
    // Pass a wide pointer to [u8] directly (not a pointer to the &[u8] on the stack)
    let src_shape = <&[u8]>::SHAPE;
    let src_str: &[u8] = b"buh";
    let src_ptr = facet::PtrConst::new(src_str);

    let mut uninit = core::mem::MaybeUninit::<AsStr>::uninit();
    let dst_ptr = facet::PtrUninit::from_maybe_uninit(&mut uninit);

    let outcome = unsafe { shape.call_try_from(src_shape, src_ptr, dst_ptr) };
    assert!(
        matches!(outcome, Some(facet::TryFromOutcome::Converted)),
        "try_from should succeed with valid UTF-8 string"
    );

    let result = unsafe { uninit.assume_init() };
    assert_eq!(result.value, "buh");

    // Test failed conversion
    let invalid_str: &[u8] = b"\xFF";
    let invalid_ptr = facet::PtrConst::new(invalid_str);

    let mut uninit2 = core::mem::MaybeUninit::<AsStr>::uninit();
    let dst_ptr2 = facet::PtrUninit::from_maybe_uninit(&mut uninit2);

    let outcome2 = unsafe { shape.call_try_from(src_shape, invalid_ptr, dst_ptr2) };
    assert!(
        matches!(outcome2, Some(facet::TryFromOutcome::Failed(_))),
        "try_from should fail with invalid UTF-8 string"
    );
}

// Test that from_ref returns Unsupported for wrong source type
#[test]
fn from_ref_wrong_source_type() {
    use std::borrow::Cow;

    #[derive(Debug, PartialEq, Facet)]
    #[facet(from_ref = TextData::from_ref)]
    struct TextData<'a> {
        text: Cow<'a, str>,
    }

    impl<'a> TextData<'a> {
        fn from_ref(s: &'a str) -> Self {
            TextData {
                text: Cow::Borrowed(s),
            }
        }
    }

    let shape = TextData::SHAPE;

    // Try to convert from wrong type (i32 instead of &str)
    let wrong_shape = i32::SHAPE;
    let wrong_value: i32 = 42;
    let wrong_ptr = facet::PtrConst::new(&wrong_value as *const i32);

    let mut uninit = core::mem::MaybeUninit::<TextData<'_>>::uninit();
    let dst_ptr = facet::PtrUninit::from_maybe_uninit(&mut uninit);

    let outcome = unsafe { shape.call_try_from(wrong_shape, wrong_ptr, dst_ptr) };
    assert!(
        matches!(outcome, Some(facet::TryFromOutcome::Unsupported)),
        "try_from should return Unsupported for wrong source type"
    );
}

#[test]
fn from_ref_enum() {
    use std::borrow::Cow;

    #[derive(Debug, PartialEq, Facet)]
    #[facet(from_ref = Status::from_ref)]
    #[repr(u8)]
    enum Status<'a> {
        Active(Cow<'a, str>),
        Inactive,
    }

    impl<'a> Status<'a> {
        fn from_ref(s: &'a str) -> Self {
            if s == "inactive" {
                Status::Inactive
            } else {
                Status::Active(Cow::Borrowed(s))
            }
        }
    }

    let shape = Status::SHAPE;

    // The VTable should have try_from set
    assert!(
        shape.vtable.has_try_from(),
        "Status enum should have try_from in VTable"
    );

    // Test with "active" status
    // Pass a wide pointer to str directly (not a pointer to the &str on the stack)
    let src_shape = <&str>::SHAPE;
    let src_str: &str = "hello";
    let src_ptr = facet::PtrConst::new(src_str);

    let mut uninit = core::mem::MaybeUninit::<Status<'_>>::uninit();
    let dst_ptr = facet::PtrUninit::from_maybe_uninit(&mut uninit);

    let outcome = unsafe { shape.call_try_from(src_shape, src_ptr, dst_ptr) };
    assert!(
        matches!(outcome, Some(facet::TryFromOutcome::Converted)),
        "try_from should succeed with matching source shape"
    );

    let result = unsafe { uninit.assume_init() };
    assert_eq!(result, Status::Active(Cow::Borrowed("hello")));

    // Test with "inactive" status
    let src_str2: &str = "inactive";
    let src_ptr2 = facet::PtrConst::new(src_str2);

    let mut uninit2 = core::mem::MaybeUninit::<Status<'_>>::uninit();
    let dst_ptr2 = facet::PtrUninit::from_maybe_uninit(&mut uninit2);

    let outcome2 = unsafe { shape.call_try_from(src_shape, src_ptr2, dst_ptr2) };
    assert!(
        matches!(outcome2, Some(facet::TryFromOutcome::Converted)),
        "try_from should succeed for inactive status"
    );

    let result2 = unsafe { uninit2.assume_init() };
    assert_eq!(result2, Status::Inactive);
}

#[cfg(feature = "yoke")]
mod yoke {
    use super::*;

    #[test]
    fn test_yoke() {
        #[derive(Debug, PartialEq, Facet)]
        #[facet(try_from_ref = AsStr::try_from_ref)]
        struct AsStr<'a> {
            value: &'a str,
        }

        impl<'a> AsStr<'a> {
            fn try_from_ref(s: &'a [u8]) -> Result<Self, std::str::Utf8Error> {
                Ok(AsStr {
                    value: std::str::from_utf8(s)?,
                })
            }
        }
    }
}

#[test]
fn metadata_container_basic() {
    // Basic metadata container with doc metadata
    #[derive(Facet)]
    #[facet(metadata_container)]
    struct Documented<T> {
        value: T,
        #[facet(metadata = "doc")]
        doc: Option<Vec<String>>,
    }

    let shape = <Documented<String>>::SHAPE;
    assert!(shape.is_metadata_container());

    // Check that the struct has the right fields
    if let Type::User(UserType::Struct(StructType { fields, .. })) = shape.ty {
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "value");
        assert_eq!(fields[1].name, "doc");
        // The doc field should have metadata kind "doc"
        assert_eq!(fields[1].metadata_kind(), Some("doc"));
    } else {
        panic!("Expected struct type");
    }
}

#[test]
fn metadata_container_with_span() {
    // A span type for testing
    #[derive(Facet, Clone, Copy)]
    struct Span {
        start: usize,
        end: usize,
    }

    // Metadata container with span metadata
    #[derive(Facet)]
    #[facet(metadata_container)]
    struct Spanned<T> {
        value: T,
        #[facet(metadata = "span")]
        span: Option<Span>,
    }

    let shape = <Spanned<u32>>::SHAPE;
    assert!(shape.is_metadata_container());

    if let Type::User(UserType::Struct(StructType { fields, .. })) = shape.ty {
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "value");
        assert_eq!(fields[1].name, "span");
        assert_eq!(fields[1].metadata_kind(), Some("span"));
    } else {
        panic!("Expected struct type");
    }
}

#[test]
fn non_metadata_container_flag() {
    // A regular struct should NOT have the metadata_container flag
    #[derive(Facet)]
    struct Regular {
        foo: u32,
        bar: String,
    }

    let shape = Regular::SHAPE;
    assert!(!shape.is_metadata_container());
}

// Test for issue #1941 - macro_rules_attribute visibility parsing
// When $vis:vis is used in macro_rules, it gets wrapped in a Group with Delimiter::None
// This test verifies that the derive macro handles this correctly
#[test]
fn macro_rules_visibility_fragment() {
    // This simulates what macro_rules_attribute does
    macro_rules! with_vis {
        ($vis:vis struct $name:ident;) => {
            #[derive(Facet)]
            $vis struct $name;
        };
    }

    with_vis!(
        pub struct VisibleCat;
    );
    with_vis!(
        struct PrivateCat;
    );

    // Just verify these compile and have correct shapes
    let shape = VisibleCat::SHAPE;
    assert_eq!(format!("{shape}"), "VisibleCat");

    let shape = PrivateCat::SHAPE;
    assert_eq!(format!("{shape}"), "PrivateCat");
}

// Test macro_rules visibility with struct fields
#[test]
fn macro_rules_visibility_fragment_with_fields() {
    macro_rules! with_vis_fields {
        ($vis:vis struct $name:ident { $field_vis:vis $field:ident: $ty:ty $(,)? }) => {
            #[derive(Facet)]
            $vis struct $name {
                $field_vis $field: $ty,
            }
        };
    }

    with_vis_fields!(
        pub struct VisibleDog {
            pub name: String,
        }
    );

    let shape = VisibleDog::SHAPE;
    assert_eq!(format!("{shape}"), "VisibleDog");

    if let Type::User(UserType::Struct(StructType { fields, .. })) = shape.ty {
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "name");
    } else {
        panic!("Expected struct type");
    }
}

// Test macro_rules visibility with enum
#[test]
fn macro_rules_visibility_fragment_enum() {
    macro_rules! with_vis_enum {
        ($vis:vis enum $name:ident { $($variant:ident),* $(,)? }) => {
            #[derive(Facet)]
            #[repr(u8)]
            #[allow(dead_code)]
            $vis enum $name {
                $($variant),*
            }
        };
    }

    with_vis_enum!(
        pub enum VisibleColor {
            Red,
            Green,
            Blue,
        }
    );

    let shape = VisibleColor::SHAPE;
    assert_eq!(format!("{shape}"), "VisibleColor");

    if let Type::User(UserType::Enum(enum_kind)) = shape.ty {
        assert_eq!(enum_kind.variants.len(), 3);
        assert_eq!(enum_kind.variants[0].name, "Red");
        assert_eq!(enum_kind.variants[1].name, "Green");
        assert_eq!(enum_kind.variants[2].name, "Blue");
    } else {
        panic!("Expected enum type");
    }
}

#[test]
fn skip_unless_truthy_on_option_ref_with_lifetime_compiles() {
    #[derive(Facet)]
    struct CommandAttrs<'a> {
        pub program: &'a str,
        #[facet(skip_unless_truthy)]
        pub cwd: Option<&'a str>,
    }

    if let Type::User(UserType::Struct(StructType { fields, .. })) = CommandAttrs::SHAPE.ty {
        assert_eq!(fields.len(), 2);
        assert!(fields[1].skip_serializing_if.is_some());
    } else {
        panic!("Expected struct type");
    }
}
