//! Tests for Option<T> and Result<T, E> serialization

use eyre::Result;
use facet::Facet;
use facet_postcard::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};

// ============================================================================
// Option<T> tests with various T types
// ============================================================================

mod option_tests {
    use super::*;

    #[test]
    fn test_option_u32_some() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<u32> = Some(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<u32> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_u32_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<u32> = None;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<u32> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_string_some() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<String> = Some("hello world".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_string_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<String> = None;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_vec_some() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<Vec<i32>> = Some(vec![1, 2, 3, 4, 5]);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<Vec<i32>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_vec_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<Vec<i32>> = None;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<Vec<i32>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[test]
    fn test_option_struct_some() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<Point> = Some(Point { x: 10, y: 20 });
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<Point> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_struct_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<Point> = None;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<Point> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_option_some_some() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<Option<u32>> = Some(Some(42));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<Option<u32>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_option_some_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<Option<u32>> = Some(None);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<Option<u32>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_option_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<Option<u32>> = None;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<Option<u32>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Result<T, E> tests with various T and E types
// ============================================================================

mod result_tests {
    use super::*;

    #[test]
    fn test_result_u32_string_ok() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<u32, String> = Ok(42);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<u32, String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_u32_string_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<u32, String> = Err("something went wrong".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<u32, String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_string_u32_ok() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<String, u32> = Ok("success".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<String, u32> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_string_u32_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<String, u32> = Err(404);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<String, u32> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct SuccessData {
        code: u32,
        message: String,
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct ErrorData {
        error_code: i32,
        details: String,
    }

    #[test]
    fn test_result_struct_struct_ok() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<SuccessData, ErrorData> = Ok(SuccessData {
            code: 200,
            message: "OK".to_string(),
        });
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<SuccessData, ErrorData> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_struct_struct_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<SuccessData, ErrorData> = Err(ErrorData {
            error_code: -1,
            details: "Internal error".to_string(),
        });
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<SuccessData, ErrorData> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_vec_vec_ok() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<Vec<u32>, Vec<String>> = Ok(vec![1, 2, 3, 4, 5]);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<Vec<u32>, Vec<String>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_vec_vec_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<Vec<u32>, Vec<String>> =
            Err(vec!["error1".to_string(), "error2".to_string()]);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<Vec<u32>, Vec<String>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_result_ok_ok() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<core::result::Result<u32, String>, String> = Ok(Ok(42));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<core::result::Result<u32, String>, String> =
            from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_result_ok_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<core::result::Result<u32, String>, String> =
            Ok(Err("inner error".to_string()));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<core::result::Result<u32, String>, String> =
            from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_nested_result_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<core::result::Result<u32, String>, String> =
            Err("outer error".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<core::result::Result<u32, String>, String> =
            from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Combined Option and Result tests
// ============================================================================

mod option_result_combined_tests {
    use super::*;

    #[test]
    fn test_option_result_some_ok() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<core::result::Result<u32, String>> = Some(Ok(42));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<core::result::Result<u32, String>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_result_some_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<core::result::Result<u32, String>> = Some(Err("error".to_string()));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<core::result::Result<u32, String>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_result_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<core::result::Result<u32, String>> = None;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<core::result::Result<u32, String>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_option_ok_some() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<Option<u32>, String> = Ok(Some(42));
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<Option<u32>, String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_option_ok_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<Option<u32>, String> = Ok(None);
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<Option<u32>, String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_option_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<Option<u32>, String> = Err("error".to_string());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<Option<u32>, String> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct Container {
        maybe_result: Option<core::result::Result<i32, String>>,
        result_maybe: core::result::Result<Option<String>, u32>,
    }

    #[test]
    fn test_struct_with_option_result_fields() -> Result<()> {
        facet_testhelpers::setup();
        let value = Container {
            maybe_result: Some(Ok(100)),
            result_maybe: Ok(Some("data".to_string())),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Container = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_struct_with_mixed_none_err() -> Result<()> {
        facet_testhelpers::setup();
        let value = Container {
            maybe_result: None,
            result_maybe: Err(404),
        };
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Container = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}

// ============================================================================
// Edge cases
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_result_unit_unit_ok() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<(), ()> = Ok(());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<(), ()> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_result_unit_unit_err() -> Result<()> {
        facet_testhelpers::setup();
        let value: core::result::Result<(), ()> = Err(());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: core::result::Result<(), ()> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_unit_some() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<()> = Some(());
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<()> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_option_unit_none() -> Result<()> {
        facet_testhelpers::setup();
        let value: Option<()> = None;
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Option<()> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_vec_of_options() -> Result<()> {
        facet_testhelpers::setup();
        let value: Vec<Option<u32>> = vec![Some(1), None, Some(3), None, Some(5)];
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Vec<Option<u32>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }

    #[test]
    fn test_vec_of_results() -> Result<()> {
        facet_testhelpers::setup();
        let value: Vec<core::result::Result<u32, String>> = vec![
            Ok(1),
            Err("error".to_string()),
            Ok(3),
            Err("another error".to_string()),
        ];
        let facet_bytes = to_vec(&value)?;
        let postcard_bytes = postcard_to_vec(&value)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: Vec<core::result::Result<u32, String>> = from_slice(&facet_bytes)?;
        assert_eq!(value, decoded);
        Ok(())
    }
}
