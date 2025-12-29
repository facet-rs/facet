//! Tests for all enum variant kinds

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

// ============================================================================
// Unit variant tests
// ============================================================================

mod unit_variant_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    enum UnitEnum {
        A,
        B,
        C,
    }

    #[test]
    fn test_unit_variant_first() -> Result<()> {
        facet_testhelpers::setup();
        let value = UnitEnum::A;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: UnitEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_unit_variant_middle() -> Result<()> {
        facet_testhelpers::setup();
        let value = UnitEnum::B;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: UnitEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_unit_variant_last() -> Result<()> {
        facet_testhelpers::setup();
        let value = UnitEnum::C;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: UnitEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    // Enum with many variants to test varint encoding
    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum ManyVariants {
        V0,
        V1,
        V2,
        V3,
        V4,
        V5,
        V6,
        V7,
        V8,
        V9,
        V10,
        V11,
        V12,
        V13,
        V14,
        V15,
        V16,
        V17,
        V18,
        V19,
    }

    #[test]
    fn test_many_unit_variants() -> Result<()> {
        facet_testhelpers::setup();

        let value = ManyVariants::V0;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let value = ManyVariants::V10;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let value = ManyVariants::V19;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        Ok(())
    }
}

// ============================================================================
// Newtype variant tests
// ============================================================================

mod newtype_variant_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum NewtypeEnum {
        Unit,
        Number(u32),
        Text(String),
    }

    #[test]
    fn test_newtype_variant_number() -> Result<()> {
        facet_testhelpers::setup();
        let value = NewtypeEnum::Number(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NewtypeEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_newtype_variant_string() -> Result<()> {
        facet_testhelpers::setup();
        let value = NewtypeEnum::Text("hello".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NewtypeEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum NewtypeWithVec {
        Empty,
        Numbers(Vec<u32>),
    }

    #[test]
    fn test_newtype_variant_vec() -> Result<()> {
        facet_testhelpers::setup();
        let value = NewtypeWithVec::Numbers(vec![1, 2, 3, 4, 5]);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: NewtypeWithVec = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Tuple variant tests
// ============================================================================

mod tuple_variant_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum TupleEnum {
        Unit,
        Pair(u32, String),
        Triple(u8, u16, u32),
    }

    #[test]
    fn test_tuple_variant_pair() -> Result<()> {
        facet_testhelpers::setup();
        let value = TupleEnum::Pair(42, "hello".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TupleEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_tuple_variant_triple() -> Result<()> {
        facet_testhelpers::setup();
        let value = TupleEnum::Triple(1, 1000, 100000);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: TupleEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Struct variant tests
// ============================================================================

mod struct_variant_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum StructEnum {
        Unit,
        Named { x: i32, y: i32 },
        MoreFields { a: String, b: bool, c: f64 },
    }

    #[test]
    fn test_struct_variant_named() -> Result<()> {
        facet_testhelpers::setup();
        let value = StructEnum::Named { x: 10, y: -20 };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: StructEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_struct_variant_more_fields() -> Result<()> {
        facet_testhelpers::setup();
        let value = StructEnum::MoreFields {
            a: "test".to_string(),
            b: true,
            c: 1.5,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: StructEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Mixed variant tests
// ============================================================================

mod mixed_variant_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum MixedEnum {
        Unit,
        Newtype(u32),
        Tuple(String, bool),
        Struct { name: String, value: i64 },
    }

    #[test]
    fn test_mixed_unit() -> Result<()> {
        facet_testhelpers::setup();
        let value = MixedEnum::Unit;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MixedEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_mixed_newtype() -> Result<()> {
        facet_testhelpers::setup();
        let value = MixedEnum::Newtype(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MixedEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_mixed_tuple() -> Result<()> {
        facet_testhelpers::setup();
        let value = MixedEnum::Tuple("hello".to_string(), true);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MixedEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_mixed_struct() -> Result<()> {
        facet_testhelpers::setup();
        let value = MixedEnum::Struct {
            name: "test".to_string(),
            value: -42,
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MixedEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Nested enum tests
// ============================================================================

mod nested_enum_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Inner {
        A,
        B(u32),
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Outer {
        Wrap(Inner),
        Direct(u32),
    }

    #[test]
    fn test_nested_enum_a() -> Result<()> {
        facet_testhelpers::setup();
        let value = Outer::Wrap(Inner::A);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Outer = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_enum_b() -> Result<()> {
        facet_testhelpers::setup();
        let value = Outer::Wrap(Inner::B(42));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Outer = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Option<Enum> tests
// ============================================================================

mod option_enum_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SimpleEnum {
        A,
        B,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct OptionalEnum {
        value: Option<SimpleEnum>,
    }

    #[test]
    fn test_option_enum_none() -> Result<()> {
        facet_testhelpers::setup();
        let value = OptionalEnum { value: None };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionalEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_enum_some() -> Result<()> {
        facet_testhelpers::setup();
        let value = OptionalEnum {
            value: Some(SimpleEnum::B),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: OptionalEnum = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Vec<Enum> tests
// ============================================================================

mod vec_enum_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Colors {
        values: Vec<Color>,
    }

    #[test]
    fn test_vec_of_enums() -> Result<()> {
        facet_testhelpers::setup();
        let value = Colors {
            values: vec![Color::Red, Color::Green, Color::Blue, Color::Red],
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Colors = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_empty_vec_of_enums() -> Result<()> {
        facet_testhelpers::setup();
        let value = Colors { values: vec![] };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Colors = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Generic enum tests
// ============================================================================

mod generic_enum_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum MyResult<T, E> {
        Ok(T),
        Err(E),
    }

    #[test]
    fn test_generic_enum_ok() -> eyre::Result<()> {
        facet_testhelpers::setup();
        let value: MyResult<u32, String> = MyResult::Ok(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MyResult<u32, String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_generic_enum_err() -> eyre::Result<()> {
        facet_testhelpers::setup();
        let value: MyResult<u32, String> = MyResult::Err("error".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MyResult<u32, String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}
