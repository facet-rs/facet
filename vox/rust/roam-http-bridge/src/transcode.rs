//! JSON ↔ postcard transcoding using facet_value::Value.
//!
//! r[bridge.json.facet]
//! The bridge uses facet-json and facet-postcard for transcoding between
//! HTTP/JSON and roam wire format (postcard). The `Value` type acts as
//! the interchange format, enabling runtime transcoding without per-service
//! code generation.

use crate::BridgeError;
use facet_core::Shape;
use facet_value::Value;

/// Transcode JSON array to postcard bytes using arg shapes.
///
/// r[bridge.json.facet]
/// JSON array → Value elements → postcard tuple (concatenated)
///
/// The JSON body is an array of arguments `[arg0, arg1, ...]`.
/// Each argument is serialized using its corresponding shape from the method signature.
/// The result is the concatenation of the serialized arguments, which is how
/// postcard encodes tuples.
pub fn json_args_to_postcard(
    json: &[u8],
    arg_shapes: &[&'static Shape],
) -> Result<Vec<u8>, BridgeError> {
    // Parse JSON into Value (JSON is self-describing)
    let value: Value = facet_json::from_slice(json)
        .map_err(|e| BridgeError::bad_request(format!("Invalid JSON: {e}")))?;

    // Must be an array
    let args = value.as_array().ok_or_else(|| {
        BridgeError::bad_request("Request body must be a JSON array of arguments")
    })?;

    // Check argument count matches
    if args.len() != arg_shapes.len() {
        return Err(BridgeError::bad_request(format!(
            "Expected {} arguments, got {}",
            arg_shapes.len(),
            args.len()
        )));
    }

    // Serialize each argument using its shape and concatenate
    // This produces the same bytes as serializing a typed tuple
    let mut result = Vec::new();
    for (arg, shape) in args.iter().zip(arg_shapes.iter()) {
        let bytes = facet_postcard::to_vec_with_shape(arg, shape)
            .map_err(|e| BridgeError::bad_request(format!("Failed to encode argument: {e}")))?;
        result.extend(bytes);
    }

    Ok(result)
}

/// Transcode postcard bytes to JSON bytes using shape information.
///
/// r[bridge.json.facet]
/// postcard → Value (with shape hint) → JSON
///
/// Since postcard is not self-describing, we need the shape to decode it.
pub fn postcard_to_json_with_shape(
    postcard: &[u8],
    shape: &'static Shape,
) -> Result<Vec<u8>, BridgeError> {
    // Parse postcard into Value using shape information
    let value: Value = facet_postcard::from_slice_with_shape(postcard, shape)
        .map_err(|e| BridgeError::internal(format!("Invalid postcard response: {e}")))?;

    // Serialize Value to JSON
    facet_json::to_vec(&value)
        .map_err(|e| BridgeError::internal(format!("JSON serialization failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Debug, Facet, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[test]
    fn test_json_args_to_postcard_single_string() {
        // JSON array: ["hello world"]
        // Arg shapes: [String]
        let json = br#"["hello world"]"#;
        let shapes: &[&'static Shape] = &[<String as Facet>::SHAPE];
        let postcard = json_args_to_postcard(json, shapes).unwrap();

        // Compare to typed tuple serialization
        let typed_tuple: (String,) = ("hello world".to_string(),);
        let expected = facet_postcard::to_vec(&typed_tuple).unwrap();

        assert_eq!(postcard, expected);
    }

    #[test]
    fn test_json_args_to_postcard_two_ints() {
        // JSON array: [10, 20]
        // Arg shapes: [i64, i64]
        let json = br#"[10, 20]"#;
        let shapes: &[&'static Shape] = &[<i64 as Facet>::SHAPE, <i64 as Facet>::SHAPE];
        let postcard = json_args_to_postcard(json, shapes).unwrap();

        // Compare to typed tuple serialization
        let typed_tuple: (i64, i64) = (10, 20);
        let expected = facet_postcard::to_vec(&typed_tuple).unwrap();

        assert_eq!(postcard, expected);
    }

    #[test]
    fn test_json_args_to_postcard_struct() {
        // JSON array: [{"x": 10, "y": 20}]
        // Arg shapes: [Point]
        let json = br#"[{"x": 10, "y": 20}]"#;
        let shapes: &[&'static Shape] = &[Point::SHAPE];
        let postcard = json_args_to_postcard(json, shapes).unwrap();

        // Compare to typed tuple serialization
        let typed_tuple: (Point,) = (Point { x: 10, y: 20 },);
        let expected = facet_postcard::to_vec(&typed_tuple).unwrap();

        assert_eq!(postcard, expected);
    }

    #[test]
    fn test_json_args_wrong_count() {
        let json = br#"["hello", "world"]"#;
        let shapes: &[&'static Shape] = &[<String as Facet>::SHAPE];
        let result = json_args_to_postcard(json, shapes);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Expected 1 arguments"));
    }

    #[test]
    fn test_json_args_not_array() {
        let json = br#"{"message": "hello"}"#;
        let shapes: &[&'static Shape] = &[<String as Facet>::SHAPE];
        let result = json_args_to_postcard(json, shapes);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must be a JSON array"));
    }

    #[test]
    fn test_postcard_to_json_with_shape() {
        // Create a Point, serialize to postcard
        let point = Point { x: 42, y: 99 };
        let postcard_bytes = facet_postcard::to_vec(&point).unwrap();

        // Transcode back to JSON using shape
        let json = postcard_to_json_with_shape(&postcard_bytes, Point::SHAPE).unwrap();
        let json_str = String::from_utf8(json).unwrap();

        // Should contain the field values
        assert!(json_str.contains("42"));
        assert!(json_str.contains("99"));
    }

    #[test]
    fn test_roundtrip_typed() {
        // The realistic scenario: typed Rust value → postcard → JSON
        // This is what happens when roam returns a response
        let point = Point { x: 10, y: 20 };

        // Typed value → postcard (what roam does internally)
        let postcard_bytes = facet_postcard::to_vec(&point).unwrap();

        // postcard → JSON (what the bridge does)
        let json = postcard_to_json_with_shape(&postcard_bytes, Point::SHAPE).unwrap();
        let json_str = String::from_utf8(json).unwrap();

        // Verify the values are correct
        let value: Value = facet_json::from_slice(json_str.as_bytes()).unwrap();
        let obj = value.as_object().unwrap();
        assert_eq!(
            obj.get("x").unwrap().as_number().unwrap().to_i64(),
            Some(10)
        );
        assert_eq!(
            obj.get("y").unwrap().as_number().unwrap().to_i64(),
            Some(20)
        );
    }
}
