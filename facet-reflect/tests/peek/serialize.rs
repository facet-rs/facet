use facet_testhelpers::{IPanic, test};

use facet::{Facet, Opaque};
use facet_reflect::{HasFields, Peek, ReflectErrorKind};

#[test]
fn peek_opaque_custom_serialize() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    // Proxy type that derives Facet
    #[derive(Facet, Copy, Clone)]
    pub struct NotDerivingFacetProxy(u64);

    impl TryFrom<NotDerivingFacetProxy> for NotDerivingFacet {
        type Error = &'static str;
        fn try_from(val: NotDerivingFacetProxy) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacet(val.0))
        }
    }

    impl TryFrom<&NotDerivingFacet> for NotDerivingFacetProxy {
        type Error = &'static str;
        fn try_from(val: &NotDerivingFacet) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacetProxy(val.0))
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, proxy = NotDerivingFacetProxy)]
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
    if let Some((field, peek)) = peek_struct.fields_for_serialize().next() {
        tested = true;
        // Use id() for location equality - opaque types don't support value equality
        assert_eq!(inner_field.id(), peek.id());
        assert!(field.field.unwrap().has_proxy());
        let owned = peek
            .custom_serialization(field.field.unwrap())
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<NotDerivingFacetProxy>().unwrap();
        assert_eq!(proxy_value.0, 35);
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

    // Proxy type for serialization
    #[derive(Facet)]
    pub struct Struct1Proxy {
        sum: String,
    }

    impl TryFrom<Struct1Proxy> for Struct1 {
        type Error = &'static str;
        fn try_from(val: Struct1Proxy) -> Result<Self, Self::Error> {
            Ok(Struct1 {
                val: val.sum.parse().unwrap(),
            })
        }
    }

    impl TryFrom<&Struct1> for Struct1Proxy {
        type Error = &'static str;
        fn try_from(val: &Struct1) -> Result<Self, Self::Error> {
            Ok(Struct1Proxy {
                sum: format!("0x{:x}", val.val),
            })
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, proxy = Struct1Proxy)]
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
    if let Some((field, peek)) = peek_struct.fields_for_serialize().next() {
        tested = true;
        // Use id() for location equality - opaque types don't support value equality
        assert_eq!(inner_field.id(), peek.id());
        assert!(field.field.unwrap().has_proxy());
        let owned = peek
            .custom_serialization(field.field.unwrap())
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<Struct1Proxy>().unwrap();
        assert_eq!(proxy_value.sum, "0x23");
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_opaque_custom_serialize_enum_tuple() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    // Proxy type that derives Facet
    #[derive(Facet, Copy, Clone)]
    pub struct NotDerivingFacetProxy(u64);

    impl TryFrom<NotDerivingFacetProxy> for NotDerivingFacet {
        type Error = &'static str;
        fn try_from(val: NotDerivingFacetProxy) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacet(val.0))
        }
    }

    impl TryFrom<&NotDerivingFacet> for NotDerivingFacetProxy {
        type Error = &'static str;
        fn try_from(val: &NotDerivingFacet) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacetProxy(val.0))
        }
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    pub enum Choices {
        Opaque(#[facet(opaque, proxy = NotDerivingFacetProxy)] NotDerivingFacet),
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
    if let Some((field, peek)) = peek_enum.fields_for_serialize().next() {
        tested = true;
        // Use id() for location equality - opaque types don't support value equality
        assert_eq!(inner_field.id(), peek.id());
        assert!(field.field.unwrap().has_proxy());
        let owned = peek
            .custom_serialization(field.field.unwrap())
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<NotDerivingFacetProxy>().unwrap();
        assert_eq!(proxy_value.0, 35);
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_opaque_custom_serialize_enum_feels() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    // Proxy type that derives Facet
    #[derive(Facet, Copy, Clone)]
    pub struct NotDerivingFacetProxy(u64);

    impl TryFrom<NotDerivingFacetProxy> for NotDerivingFacet {
        type Error = &'static str;
        fn try_from(val: NotDerivingFacetProxy) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacet(val.0))
        }
    }

    impl TryFrom<&NotDerivingFacet> for NotDerivingFacetProxy {
        type Error = &'static str;
        fn try_from(val: &NotDerivingFacet) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacetProxy(val.0))
        }
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    pub enum Choices {
        Opaque {
            #[facet(opaque, proxy = NotDerivingFacetProxy)]
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
    if let Some((field, peek)) = peek_enum.fields_for_serialize().next() {
        tested = true;
        // Use id() for location equality - opaque types don't support value equality
        assert_eq!(inner_field.id(), peek.id());
        assert!(field.field.unwrap().has_proxy());
        let owned = peek
            .custom_serialization(field.field.unwrap())
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<NotDerivingFacetProxy>().unwrap();
        assert_eq!(proxy_value.0, 35);
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

    // Proxy type for serialization
    #[derive(Facet)]
    pub struct ArcStruct1Proxy {
        sum: String,
    }

    impl TryFrom<ArcStruct1Proxy> for std::sync::Arc<Struct1> {
        type Error = &'static str;
        fn try_from(val: ArcStruct1Proxy) -> Result<Self, Self::Error> {
            Ok(std::sync::Arc::new(Struct1 {
                val: val.sum.parse().unwrap(),
            }))
        }
    }

    impl TryFrom<&std::sync::Arc<Struct1>> for ArcStruct1Proxy {
        type Error = &'static str;
        fn try_from(val: &std::sync::Arc<Struct1>) -> Result<Self, Self::Error> {
            Ok(ArcStruct1Proxy {
                sum: format!("0x{:x}", val.val),
            })
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, proxy = ArcStruct1Proxy)]
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
    if let Some((field, peek)) = peek_struct.fields_for_serialize().next() {
        tested = true;
        // Use id() for location equality - opaque types don't support value equality
        assert_eq!(inner_field.id(), peek.id());
        assert!(field.field.unwrap().has_proxy());
        let owned = peek
            .custom_serialization(field.field.unwrap())
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<ArcStruct1Proxy>().unwrap();
        assert_eq!(proxy_value.sum, "0x23");
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_custom_serialize_errors() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    // Proxy type that derives Facet
    #[derive(Facet, Copy, Clone)]
    pub struct NotDerivingFacetProxy(u64);

    impl TryFrom<NotDerivingFacetProxy> for NotDerivingFacet {
        type Error = &'static str;
        fn try_from(val: NotDerivingFacetProxy) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacet(val.0))
        }
    }

    impl TryFrom<&NotDerivingFacet> for NotDerivingFacetProxy {
        type Error = &'static str;
        fn try_from(val: &NotDerivingFacet) -> Result<Self, Self::Error> {
            if val.0 == 35 {
                Err("35 is not allowed!")
            } else {
                Ok(NotDerivingFacetProxy(val.0))
            }
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, proxy = NotDerivingFacetProxy)]
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
    if let Some((field, peek)) = peek_struct.fields_for_serialize().next() {
        tested = true;
        // Use id() for location equality - opaque types don't support value equality
        assert_eq!(inner_field.id(), peek.id());
        assert!(field.field.unwrap().has_proxy());
        let cust_ser_result = peek.custom_serialization(field.field.unwrap());
        if let Err(err) = cust_ser_result {
            if let ReflectErrorKind::CustomSerializationError {
                message,
                src_shape,
                dst_shape,
            } = err.kind
            {
                assert_eq!(message, "35 is not allowed!");
                assert_eq!(src_shape, Opaque::<NotDerivingFacet>::SHAPE);
                assert_eq!(dst_shape, NotDerivingFacetProxy::SHAPE);
            } else {
                panic!("expected custom serialization error, got: {err:?}");
            }
        } else {
            panic!("expected custom serialization error");
        }
    }
    assert!(tested);
    Ok(())
}

#[test]
fn peek_custom_serialize_zst() -> Result<(), IPanic> {
    // Proxy type for () (ZST)
    #[derive(Facet)]
    pub struct UnitProxy(u64);

    impl TryFrom<UnitProxy> for () {
        type Error = &'static str;
        fn try_from(_: UnitProxy) -> Result<Self, Self::Error> {
            Ok(())
        }
    }

    impl TryFrom<&()> for UnitProxy {
        type Error = &'static str;
        fn try_from(_: &()) -> Result<Self, Self::Error> {
            Ok(UnitProxy(35))
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(proxy = UnitProxy)]
        inner: (),
    }

    let container = Container { inner: () };

    let peek_value = Peek::new(&container);

    let peek_struct = peek_value
        .into_struct()
        .expect("Should be convertible to struct");

    let inner_field = peek_struct
        .field_by_name("inner")
        .expect("Should have an inner field");

    let mut tested = false;
    if let Some((field, peek)) = peek_struct.fields_for_serialize().next() {
        tested = true;
        assert_eq!(inner_field, peek);
        assert!(field.field.unwrap().has_proxy());
        let owned = peek
            .custom_serialization(field.field.unwrap())
            .expect("should return owned peek");
        // Test field values
        let peek = owned.as_peek();
        let proxy_value = peek.get::<UnitProxy>().unwrap();
        assert_eq!(proxy_value.0, 35);
    }
    assert!(tested);

    Ok(())
}
