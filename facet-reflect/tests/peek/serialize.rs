use facet_testhelpers::{IPanic, test};

use facet::{Facet, Opaque};
use facet_reflect::{HasFields, Peek, ReflectError};

#[test]
fn peek_opaque_custom_serialize() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    fn serialize_with(val: &NotDerivingFacet) -> Result<u64, &'static str> {
        Ok(val.0)
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, serialize_with=serialize_with)]
        inner: NotDerivingFacet,
    }

    let container = Container {
        inner: NotDerivingFacet(35),
    };

    let peek_value = Peek::new(&container);

    let peek_struct = peek_value
        .into_struct()
        .expect("Should be convertible to struct");

    let inner_field = peek_struct
        .field_by_name("inner")
        .expect("Should have an inner field");

    let mut tested = false;
    for (field, peek) in peek_struct.fields_for_serialize() {
        tested = true;
        assert_eq!(inner_field, peek);
        assert!(field.vtable.serialize_with.is_some());
        let owned = peek
            .custom_serialization(field)
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<u64>().unwrap();
        assert_eq!(*proxy_value, 35);
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_shaped_custom_serialize() -> Result<(), IPanic> {
    #[derive(Facet, Copy, Clone, Debug, Eq, PartialEq)]
    pub struct Struct1 {
        val: u64,
    }

    #[derive(Facet)]
    pub struct Struct2 {
        sum: String,
    }

    fn serialize_with(val: &Struct1) -> Result<Struct2, &'static str> {
        Ok(Struct2 {
            sum: format!("0x{:x}", val.val),
        })
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, serialize_with=serialize_with)]
        inner: Struct1,
    }

    let container = Container {
        inner: Struct1 { val: 35 },
    };

    let peek_value = Peek::new(&container);

    let peek_struct = peek_value
        .into_struct()
        .expect("Should be convertible to struct");

    let inner_field = peek_struct
        .field_by_name("inner")
        .expect("Should have an inner field");

    let mut tested = false;
    for (field, peek) in peek_struct.fields_for_serialize() {
        tested = true;
        assert_eq!(inner_field, peek);
        assert!(field.vtable.serialize_with.is_some());
        let owned = peek
            .custom_serialization(field)
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<Struct2>().unwrap();
        assert_eq!(proxy_value.sum, "0x23");
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_opaque_custom_serialize_enum_tuple() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    fn serialize_with(val: &NotDerivingFacet) -> Result<u64, &'static str> {
        Ok(val.0)
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    pub enum Choices {
        Opaque(#[facet(opaque, serialize_with=serialize_with)] NotDerivingFacet),
    }

    let container = Choices::Opaque(NotDerivingFacet(35));

    let peek_value = Peek::new(&container);

    let peek_enum = peek_value
        .into_enum()
        .expect("Should be convertible to enum");

    assert_eq!(
        peek_enum
            .active_variant()
            .expect("should be an active variant")
            .name,
        "Opaque"
    );

    let inner_field = peek_enum
        .field(0)
        .expect("Should not be an error")
        .expect("Should have an field");

    let mut tested = false;
    for (field, peek) in peek_enum.fields_for_serialize() {
        tested = true;
        assert_eq!(inner_field, peek);
        assert!(field.vtable.serialize_with.is_some());
        let owned = peek
            .custom_serialization(field)
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<u64>().unwrap();
        assert_eq!(*proxy_value, 35);
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_opaque_custom_serialize_enum_feels() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    fn serialize_with(val: &NotDerivingFacet) -> Result<u64, &'static str> {
        Ok(val.0)
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    pub enum Choices {
        Opaque {
            #[facet(opaque, serialize_with=serialize_with)]
            f1: NotDerivingFacet,
        },
    }

    let container = Choices::Opaque {
        f1: NotDerivingFacet(35),
    };

    let peek_value = Peek::new(&container);

    let peek_enum = peek_value
        .into_enum()
        .expect("Should be convertible to enum");

    assert_eq!(
        peek_enum
            .active_variant()
            .expect("should be an active variant")
            .name,
        "Opaque"
    );

    let inner_field = peek_enum
        .field_by_name("f1")
        .expect("Should not be an error")
        .expect("Should have an field");

    let mut tested = false;
    for (field, peek) in peek_enum.fields_for_serialize() {
        tested = true;
        assert_eq!(inner_field, peek);
        assert!(field.vtable.serialize_with.is_some());
        let owned = peek
            .custom_serialization(field)
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<u64>().unwrap();
        assert_eq!(*proxy_value, 35);
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_shaped_custom_serialize_pointers() -> Result<(), IPanic> {
    #[derive(Facet, Copy, Clone, Debug, Eq, PartialEq)]
    pub struct Struct1 {
        val: u64,
    }

    #[derive(Facet)]
    pub struct Struct2 {
        sum: String,
    }

    fn serialize_with(val: &std::sync::Arc<Struct1>) -> Result<Struct2, &'static str> {
        Ok(Struct2 {
            sum: format!("0x{:x}", val.val),
        })
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, serialize_with=serialize_with)]
        inner: std::sync::Arc<Struct1>,
    }

    let container = Container {
        inner: std::sync::Arc::new(Struct1 { val: 35 }),
    };

    let peek_value = Peek::new(&container);

    let peek_struct = peek_value
        .into_struct()
        .expect("Should be convertible to struct");

    let inner_field = peek_struct
        .field_by_name("inner")
        .expect("Should have an inner field");

    let mut tested = false;
    for (field, peek) in peek_struct.fields_for_serialize() {
        tested = true;
        assert_eq!(inner_field, peek);
        assert!(field.vtable.serialize_with.is_some());
        let owned = peek
            .custom_serialization(field)
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<Struct2>().unwrap();
        assert_eq!(proxy_value.sum, "0x23");
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_custom_serialize_errors() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    fn serialize_with(val: &NotDerivingFacet) -> Result<u64, &'static str> {
        if val.0 == 35 {
            Err("35 is not allowed!")
        } else {
            Ok(val.0)
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, serialize_with=serialize_with)]
        inner: NotDerivingFacet,
    }

    let container = Container {
        inner: NotDerivingFacet(35),
    };

    let peek_value = Peek::new(&container);

    let peek_struct = peek_value
        .into_struct()
        .expect("Should be convertible to struct");

    let inner_field = peek_struct
        .field_by_name("inner")
        .expect("Should have an inner field");

    let mut tested = false;
    for (field, peek) in peek_struct.fields_for_serialize() {
        tested = true;
        assert_eq!(inner_field, peek);
        assert!(field.vtable.serialize_with.is_some());
        let cust_ser_result = peek.custom_serialization(field);
        if let Err(ReflectError::CustomDeserializationError {
            message,
            src_shape,
            dst_shape,
        }) = cust_ser_result
        {
            assert_eq!(message, "35 is not allowed!");
            assert_eq!(src_shape, Opaque::<NotDerivingFacet>::SHAPE);
            assert_eq!(dst_shape, u64::SHAPE);
        } else {
            assert!(false, "expected custom deserialization error");
        }
        break;
    }
    assert!(tested);
    Ok(())
}
