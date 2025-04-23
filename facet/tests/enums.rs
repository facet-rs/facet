use facet::{Def, Facet};

#[test]
fn enum_doc_comment() {
    #[derive(Clone, Hash, PartialEq, Eq, Facet)]
    #[repr(u8)]
    /// This is an enum
    enum MyEnum {
        #[allow(dead_code)]
        A,
        #[allow(dead_code)]
        B,
    }

    assert_eq!(MyEnum::SHAPE.doc, &[" This is an enum"]);
}

#[test]
fn enum_with_unit_variants_u8() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum UnitVariantEnum {
        A,
        B,
        C,
    }

    let shape = UnitVariantEnum::SHAPE;

    assert_eq!(format!("{}", shape), "UnitVariantEnum");

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        assert_eq!(enum_def.variants[0].name, "A");
        assert_eq!(enum_def.variants[1].name, "B");
        assert_eq!(enum_def.variants[2].name, "C");

        for variant in enum_def.variants {
            assert_eq!(variant.data.fields.len(), 0);
        }
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_unit_variants_c() {
    #[derive(Debug, Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum UnitVariantEnum {
        A,
        B,
        C,
    }

    let shape = UnitVariantEnum::SHAPE;

    assert_eq!(format!("{}", shape), "UnitVariantEnum");

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        assert_eq!(enum_def.variants[0].name, "A");
        assert_eq!(enum_def.variants[1].name, "B");
        assert_eq!(enum_def.variants[2].name, "C");

        for variant in enum_def.variants {
            assert_eq!(variant.data.fields.len(), 0);
        }
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_tuple_variants_u16() {
    #[derive(Debug, Facet)]
    #[repr(u16)]
    #[allow(dead_code)]
    enum TupleVariantEnum {
        A(u32),
        B(u32, String),
        C(bool, u64, String),
    }

    let shape = TupleVariantEnum::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        let variant_a = &enum_def.variants[0];
        assert_eq!(variant_a.name, "A");
        assert_eq!(variant_a.data.fields.len(), 1);

        let variant_b = &enum_def.variants[1];
        assert_eq!(variant_b.name, "B");
        assert_eq!(variant_b.data.fields.len(), 2);

        let variant_c = &enum_def.variants[2];
        assert_eq!(variant_c.name, "C");
        assert_eq!(variant_c.data.fields.len(), 3);
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_tuple_variants_c() {
    #[derive(Debug, Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum TupleVariantEnum {
        A(u32),
        B(u32, String),
        C(bool, u64, String),
    }

    let shape = TupleVariantEnum::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        let variant_a = &enum_def.variants[0];
        assert_eq!(variant_a.name, "A");
        assert_eq!(variant_a.data.fields.len(), 1);

        let variant_b = &enum_def.variants[1];
        assert_eq!(variant_b.name, "B");
        assert_eq!(variant_b.data.fields.len(), 2);

        let variant_c = &enum_def.variants[2];
        assert_eq!(variant_c.name, "C");
        assert_eq!(variant_c.data.fields.len(), 3);
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_struct_variants_u16() {
    #[derive(Debug, Facet)]
    #[repr(u16)]
    #[allow(dead_code)]
    enum StructVariantEnum {
        A {
            value: u32,
        },
        B {
            x: u32,
            y: String,
        },
        C {
            flag: bool,
            count: u64,
            name: String,
        },
    }

    let shape = StructVariantEnum::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        let variant_a = &enum_def.variants[0];
        assert_eq!(variant_a.name, "A");
        assert_eq!(variant_a.data.fields.len(), 1);
        assert_eq!(variant_a.data.fields[0].name, "value");

        let variant_b = &enum_def.variants[1];
        assert_eq!(variant_b.name, "B");
        assert_eq!(variant_b.data.fields.len(), 2);
        assert_eq!(variant_b.data.fields[0].name, "x");
        assert_eq!(variant_b.data.fields[1].name, "y");

        let variant_c = &enum_def.variants[2];
        assert_eq!(variant_c.name, "C");
        assert_eq!(variant_c.data.fields.len(), 3);
        assert_eq!(variant_c.data.fields[0].name, "flag");
        assert_eq!(variant_c.data.fields[1].name, "count");
        assert_eq!(variant_c.data.fields[2].name, "name");
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_struct_variants_c() {
    #[derive(Debug, Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum StructVariantEnum {
        A {
            value: u32,
        },
        B {
            x: u32,
            y: String,
        },
        C {
            flag: bool,
            count: u64,
            name: String,
        },
    }

    let shape = StructVariantEnum::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        let variant_a = &enum_def.variants[0];
        assert_eq!(variant_a.name, "A");
        assert_eq!(variant_a.data.fields.len(), 1);
        assert_eq!(variant_a.data.fields[0].name, "value");

        let variant_b = &enum_def.variants[1];
        assert_eq!(variant_b.name, "B");
        assert_eq!(variant_b.data.fields.len(), 2);
        assert_eq!(variant_b.data.fields[0].name, "x");
        assert_eq!(variant_b.data.fields[1].name, "y");

        let variant_c = &enum_def.variants[2];
        assert_eq!(variant_c.name, "C");
        assert_eq!(variant_c.data.fields.len(), 3);
        assert_eq!(variant_c.data.fields[0].name, "flag");
        assert_eq!(variant_c.data.fields[1].name, "count");
        assert_eq!(variant_c.data.fields[2].name, "name");
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_mixed_variants_u32() {
    #[derive(Debug, Facet)]
    #[repr(u32)]
    #[allow(dead_code)]
    enum MixedVariantEnum {
        Unit,
        Tuple(u32, String),
        Struct { value: u64, name: String },
    }

    let shape = MixedVariantEnum::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        let unit_variant = &enum_def.variants[0];
        assert_eq!(unit_variant.name, "Unit");
        assert_eq!(unit_variant.data.fields.len(), 0);

        let tuple_variant = &enum_def.variants[1];
        assert_eq!(tuple_variant.name, "Tuple");
        assert_eq!(tuple_variant.data.fields.len(), 2);

        let struct_variant = &enum_def.variants[2];
        assert_eq!(struct_variant.name, "Struct");
        assert_eq!(struct_variant.data.fields.len(), 2);
        assert_eq!(struct_variant.data.fields[0].name, "value");
        assert_eq!(struct_variant.data.fields[1].name, "name");
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_mixed_variants_c() {
    #[derive(Debug, Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum MixedVariantEnum {
        Unit,
        Tuple(u32, String),
        Struct { value: u64, name: String },
    }

    let shape = MixedVariantEnum::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 3);

        let unit_variant = &enum_def.variants[0];
        assert_eq!(unit_variant.name, "Unit");
        assert_eq!(unit_variant.data.fields.len(), 0);

        let tuple_variant = &enum_def.variants[1];
        assert_eq!(tuple_variant.name, "Tuple");
        assert_eq!(tuple_variant.data.fields.len(), 2);

        let struct_variant = &enum_def.variants[2];
        assert_eq!(struct_variant.name, "Struct");
        assert_eq!(struct_variant.data.fields.len(), 2);
        assert_eq!(struct_variant.data.fields[0].name, "value");
        assert_eq!(struct_variant.data.fields[1].name, "name");
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_generic_u8() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum MyOption<T> {
        Some(T),
        None,
    }

    let shape = MyOption::<u32>::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 2);

        let some_variant = &enum_def.variants[0];
        assert_eq!(some_variant.name, "Some");
        assert_eq!(some_variant.data.fields.len(), 1);

        let none_variant = &enum_def.variants[1];
        assert_eq!(none_variant.name, "None");
        assert_eq!(none_variant.data.fields.len(), 0);
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_generic_c() {
    #[derive(Debug, Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum MyOption<T> {
        Some(T),
        None,
    }

    let shape = MyOption::<String>::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 2);

        let some_variant = &enum_def.variants[0];
        assert_eq!(some_variant.name, "Some");
        assert_eq!(some_variant.data.fields.len(), 1);

        let none_variant = &enum_def.variants[1];
        assert_eq!(none_variant.name, "None");
        assert_eq!(none_variant.data.fields.len(), 0);
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_multiple_generics_u8() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum MyResult<T, E> {
        Ok(T),
        Err(E),
    }

    let shape = MyResult::<u32, String>::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 2);

        let ok_variant = &enum_def.variants[0];
        assert_eq!(ok_variant.name, "Ok");
        assert_eq!(ok_variant.data.fields.len(), 1);

        let err_variant = &enum_def.variants[1];
        assert_eq!(err_variant.name, "Err");
        assert_eq!(err_variant.data.fields.len(), 1);
    } else {
        panic!("Expected Enum definition");
    }
}

#[test]
fn enum_with_multiple_generics_c() {
    #[derive(Debug, Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum MyResult<T, E> {
        Ok(T),
        Err(E),
    }

    let shape = MyResult::<bool, u64>::SHAPE;

    if let Def::Enum(enum_def) = shape.def {
        assert_eq!(enum_def.variants.len(), 2);

        let ok_variant = &enum_def.variants[0];
        assert_eq!(ok_variant.name, "Ok");
        assert_eq!(ok_variant.data.fields.len(), 1);

        let err_variant = &enum_def.variants[1];
        assert_eq!(err_variant.name, "Err");
        assert_eq!(err_variant.data.fields.len(), 1);
    } else {
        panic!("Expected Enum definition");
    }
}
