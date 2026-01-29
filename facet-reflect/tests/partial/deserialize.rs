use facet_testhelpers::{IPanic, test};

use facet::{Facet, Opaque};
use facet_reflect::{Partial, ReflectErrorKind};

#[test]
fn wip_opaque_custom_deserialize() -> Result<(), IPanic> {
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

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.set(Opaque(NotDerivingFacet(35)))?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.inner, NotDerivingFacet(35));

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), NotDerivingFacetProxy::SHAPE);
    partial = partial.set(NotDerivingFacetProxy(35))?;
    partial = partial.end()?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.inner, NotDerivingFacet(35));

    Ok(())
}

#[test]
fn wip_shaped_custom_deserialize() -> Result<(), IPanic> {
    #[derive(Facet, Copy, Clone, Debug, Eq, PartialEq)]
    pub struct Struct1 {
        val: u64,
    }

    // Proxy type for Struct1
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
                sum: val.val.to_string(),
            })
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(proxy = Struct1Proxy)]
        inner: Struct1,
    }

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.set(Struct1 { val: 10 })?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.inner.val, 10);

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), Struct1Proxy::SHAPE);
    partial = partial.set(Struct1Proxy { sum: "10".into() })?;
    partial = partial.end()?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.inner, Struct1 { val: 10 });

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_custom_deserialization()?;
    partial = partial.begin_field("sum")?;
    partial = partial.set::<String>("10".into())?;
    partial = partial.end()?;
    partial = partial.end()?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.inner, Struct1 { val: 10 });

    // skipping using the proxy and building the target struct directly instead
    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("val")?;
    partial = partial.set(10u64)?;
    partial = partial.end()?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.inner, Struct1 { val: 10 });

    Ok(())
}

#[test]
fn wip_opaque_custom_deserialize_enum_tuple() -> Result<(), IPanic> {
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

    let mut partial: Partial<'_, '_> = Partial::alloc::<Choices>()?;
    partial = partial.select_variant_named("Opaque")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(Opaque(NotDerivingFacet(35)))?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Choices>()?;

    assert!(matches!(result, Choices::Opaque(NotDerivingFacet(35))));

    let mut partial: Partial<'_, '_> = Partial::alloc::<Choices>()?;
    partial = partial.select_variant_named("Opaque")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), NotDerivingFacetProxy::SHAPE);
    partial = partial.set(NotDerivingFacetProxy(35))?;
    partial = partial.end()?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Choices>()?;

    assert!(matches!(result, Choices::Opaque(NotDerivingFacet(35))));

    Ok(())
}

#[test]
fn wip_opaque_custom_deserialize_enum_fields() -> Result<(), IPanic> {
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

    let mut partial: Partial<'_, '_> = Partial::alloc::<Choices>()?;
    partial = partial.select_variant_named("Opaque")?;
    partial = partial.begin_field("f1")?;
    partial = partial.set(Opaque(NotDerivingFacet(35)))?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Choices>()?;

    assert!(matches!(
        result,
        Choices::Opaque {
            f1: NotDerivingFacet(35)
        }
    ));

    let mut partial: Partial<'_, '_> = Partial::alloc::<Choices>()?;
    partial = partial.select_variant_named("Opaque")?;
    partial = partial.begin_field("f1")?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), NotDerivingFacetProxy::SHAPE);
    partial = partial.set(NotDerivingFacetProxy(35))?;
    partial = partial.end()?;
    partial = partial.end()?;
    let result = partial.build()?.materialize::<Choices>()?;

    assert!(matches!(
        result,
        Choices::Opaque {
            f1: NotDerivingFacet(35)
        }
    ));

    Ok(())
}

#[test]
fn wip_custom_deserialize_errors() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    // Proxy type that derives Facet
    #[derive(Facet, Copy, Clone)]
    pub struct NotDerivingFacetProxy(u64);

    impl TryFrom<NotDerivingFacetProxy> for NotDerivingFacet {
        type Error = &'static str;
        fn try_from(val: NotDerivingFacetProxy) -> Result<Self, Self::Error> {
            if val.0 == 35 {
                Err("35 is not allowed!")
            } else {
                Ok(NotDerivingFacet(val.0))
            }
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

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), NotDerivingFacetProxy::SHAPE);
    partial = partial.set(NotDerivingFacetProxy(35))?;
    let end_result = partial.end();
    if let Err(err) = end_result {
        if let ReflectErrorKind::CustomDeserializationError {
            message,
            src_shape,
            dst_shape,
        } = err.kind
        {
            assert_eq!(message, "35 is not allowed!");
            assert_eq!(src_shape, NotDerivingFacetProxy::SHAPE);
            assert_eq!(dst_shape, Opaque::<NotDerivingFacet>::SHAPE);
        } else {
            panic!("expected custom deserialization error, got: {err:?}");
        }
    } else {
        panic!("expected custom deserialization error");
    }

    Ok(())
}

#[test]
fn wip_custom_deserialize_zst() -> Result<(), IPanic> {
    pub enum ProxyError {
        MustBeThirtyFive,
        MustNeverBeTwenty,
    }

    impl std::fmt::Display for ProxyError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
            let s = match self {
                ProxyError::MustBeThirtyFive => "must be 35!",
                ProxyError::MustNeverBeTwenty => "must never be 20!",
            };
            write!(f, "{s}")
        }
    }

    // Proxy type for () (ZST)
    #[derive(Facet)]
    pub struct UnitProxy(u64);

    impl TryFrom<UnitProxy> for () {
        type Error = ProxyError;
        fn try_from(val: UnitProxy) -> Result<Self, Self::Error> {
            if val.0 == 20 {
                Err(ProxyError::MustNeverBeTwenty)
            } else if val.0 != 35 {
                Err(ProxyError::MustBeThirtyFive)
            } else {
                Ok(())
            }
        }
    }

    impl From<&()> for UnitProxy {
        fn from(_: &()) -> Self {
            UnitProxy(35)
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(proxy = UnitProxy)]
        inner: (),
    }

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), UnitProxy::SHAPE);
    partial = partial.set(UnitProxy(35))?;
    partial = partial.end()?;
    partial = partial.end()?;
    let _result = partial.build()?.materialize::<Container>()?;

    let mut partial: Partial<'_, '_> = Partial::alloc::<Container>()?;
    partial = partial.begin_field("inner")?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), UnitProxy::SHAPE);
    partial = partial.set(UnitProxy(20))?;
    let end_result = partial.end();
    if let Err(err) = end_result {
        if let ReflectErrorKind::CustomDeserializationError {
            message,
            src_shape,
            dst_shape,
        } = err.kind
        {
            assert_eq!(message, "must never be 20!");
            assert_eq!(src_shape, UnitProxy::SHAPE);
            assert_eq!(dst_shape, <() as Facet>::SHAPE);
        } else {
            panic!("expected custom deserialization error, got: {err:?}");
        }
    } else {
        panic!("expected custom deserialization error");
    }

    Ok(())
}
