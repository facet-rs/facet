//! Test the facet plugin POC
//!
//! This demonstrates the macro expansion handshake pattern.

use facet_plugin_poc::FacetPoc;

/// Connection failed: {0}
#[derive(FacetPoc)]
#[facet_poc(display)]
pub enum MyError {
    /// data store disconnected
    Disconnect(std::io::Error),

    /// invalid header (expected {expected}, found {found})
    InvalidHeader { expected: String, found: String },

    /// unknown error occurred
    Unknown,
}

/// A simple point
#[derive(FacetPoc)]
#[facet_poc(display, debug)]
pub struct Point {
    #[allow(dead_code)]
    x: i32,
    #[allow(dead_code)]
    y: i32,
}

fn main() {
    // Test Display impl from plugin
    let err = MyError::Unknown;
    println!("Error: {err}");

    let err2 = MyError::InvalidHeader {
        expected: "JSON".to_string(),
        found: "XML".to_string(),
    };
    println!("Error: {err2}");

    // Test struct with both display and debug
    let p = Point { x: 10, y: 20 };
    println!("Point display: {p}");
    println!("Point debug: {p:?}");

    println!("\nPlugin system POC works!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_display_unit_variant() {
        let err = MyError::Unknown;
        assert_eq!(format!("{err}"), "unknown error occurred");
    }

    #[test]
    fn test_enum_display_struct_variant() {
        let err = MyError::InvalidHeader {
            expected: "JSON".to_string(),
            found: "XML".to_string(),
        };
        // The field names in the doc comment get interpolated!
        assert_eq!(
            format!("{err}"),
            "invalid header (expected JSON, found XML)"
        );
    }

    #[test]
    fn test_enum_display_tuple_variant() {
        let err = MyError::Disconnect(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        ));
        assert_eq!(format!("{err}"), "data store disconnected");
    }

    #[test]
    fn test_struct_display() {
        let p = Point { x: 10, y: 20 };
        assert_eq!(format!("{p}"), "A simple point");
    }

    #[test]
    fn test_struct_debug() {
        let p = Point { x: 10, y: 20 };
        assert_eq!(format!("{p:?}"), "Point");
    }
}
