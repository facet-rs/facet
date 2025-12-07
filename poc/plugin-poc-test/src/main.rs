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
    x: i32,
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
