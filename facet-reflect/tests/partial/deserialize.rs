use facet_testhelpers::{IPanic, test};

use facet::{Facet, Opaque};
use facet_reflect::{Partial, ReflectError};

#[test]
fn wip_opaque_custom_deserialize() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    fn deserialize_with(val: &u64) -> Result<NotDerivingFacet, &'static str> {
        Ok(NotDerivingFacet(*val))
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, deserialize_with=deserialize_with)]
        inner: NotDerivingFacet,
    }

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.set(Opaque(NotDerivingFacet(35)))?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result.inner, NotDerivingFacet(35));

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), u64::SHAPE);
    partial.set(35u64)?;
    partial.end()?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result.inner, NotDerivingFacet(35));

    Ok(())
}

#[test]
fn wip_shaped_custom_deserialize() -> Result<(), IPanic> {
    #[derive(Facet, Copy, Clone, Debug, Eq, PartialEq)]
    pub struct Struct1 {
        val: u64,
    }

    #[derive(Facet)]
    pub struct Struct2 {
        sum: String,
    }

    fn deserialize_with(val: &Struct2) -> Result<Struct1, &'static str> {
        Ok(Struct1 {
            val: val.sum.parse().unwrap(),
        })
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(deserialize_with=deserialize_with)]
        inner: Struct1,
    }

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.set(Struct1 { val: 10 })?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result.inner.val, 10);

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), Struct2::SHAPE);
    partial.set(Struct2 { sum: "10".into() })?;
    partial.end()?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result.inner, Struct1 { val: 10 });

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.begin_custom_deserialization()?;
    partial.begin_field("sum")?;
    partial.set::<String>("10".into())?;
    partial.end()?;
    partial.end()?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result.inner, Struct1 { val: 10 });

    // skipping using the deserialize_with and bulding the target struct directly instead
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.begin_field("val")?;
    partial.set(10u64)?;
    partial.end()?;
    partial.end()?;
    let result = *partial.build()?;

    assert_eq!(result.inner, Struct1 { val: 10 });

    Ok(())
}

#[test]
fn wip_opaque_custom_deserialize_enum_tuple() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    fn deserialize_with(val: &u64) -> Result<NotDerivingFacet, &'static str> {
        Ok(NotDerivingFacet(*val))
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    pub enum Choices {
        Opaque(#[facet(opaque, deserialize_with=deserialize_with)] NotDerivingFacet),
    }

    let mut partial = Partial::alloc::<Choices>()?;
    partial.select_variant_named("Opaque")?;
    partial.begin_nth_field(0)?;
    partial.set(Opaque(NotDerivingFacet(35)))?;
    partial.end()?;
    let result = *partial.build()?;

    assert!(matches!(result, Choices::Opaque(NotDerivingFacet(35))));

    let mut partial = Partial::alloc::<Choices>()?;
    partial.select_variant_named("Opaque")?;
    partial.begin_nth_field(0)?;
    partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), u64::SHAPE);
    partial.set(35u64)?;
    partial.end()?;
    partial.end()?;
    let result = *partial.build()?;

    assert!(matches!(result, Choices::Opaque(NotDerivingFacet(35))));

    Ok(())
}

#[test]
fn wip_opaque_custom_deserialize_enum_fields() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    fn deserialize_with(val: &u64) -> Result<NotDerivingFacet, &'static str> {
        Ok(NotDerivingFacet(*val))
    }

    #[allow(dead_code)]
    #[derive(Facet)]
    #[repr(u8)]
    pub enum Choices {
        Opaque {
            #[facet(opaque, deserialize_with=deserialize_with)]
            f1: NotDerivingFacet,
        },
    }

    let mut partial = Partial::alloc::<Choices>()?;
    partial.select_variant_named("Opaque")?;
    partial.begin_field("f1")?;
    partial.set(Opaque(NotDerivingFacet(35)))?;
    partial.end()?;
    let result = *partial.build()?;

    assert!(matches!(
        result,
        Choices::Opaque {
            f1: NotDerivingFacet(35)
        }
    ));

    let mut partial = Partial::alloc::<Choices>()?;
    partial.select_variant_named("Opaque")?;
    partial.begin_field("f1")?;
    partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), u64::SHAPE);
    partial.set(35u64)?;
    partial.end()?;
    partial.end()?;
    let result = *partial.build()?;

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

    fn deserialize_with(val: &u64) -> Result<NotDerivingFacet, &'static str> {
        if *val == 35 {
            Err("35 is not allowed!")
        } else {
            Ok(NotDerivingFacet(*val))
        }
    }

    #[derive(Facet)]
    pub struct Container {
        #[facet(opaque, deserialize_with=deserialize_with)]
        inner: NotDerivingFacet,
    }

    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_field("inner")?;
    partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), u64::SHAPE);
    partial.set(35u64)?;
    let end_result = partial.end();
    if let Err(ReflectError::CustomDeserializationError {
        message,
        src_shape,
        dst_shape,
    }) = end_result
    {
        assert_eq!(message, "35 is not allowed!");
        assert_eq!(src_shape, u64::SHAPE);
        assert_eq!(dst_shape, Opaque::<NotDerivingFacet>::SHAPE);
    } else {
        assert!(false, "expected custom deserialization error");
    }

    Ok(())
}
