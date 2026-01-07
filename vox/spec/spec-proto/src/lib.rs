#![deny(unsafe_code)]

use facet::Facet;
use roam::service;
use roam::session::{Pull, Push};

/// Simple echo service for conformance testing.
#[service]
pub trait Echo {
    /// Echoes the message back.
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed.
    async fn reverse(&self, message: String) -> String;
}

// ============================================================================
// Complex types for testing encoding/decoding
// ============================================================================

/// A simple struct with primitive fields.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A struct with various field types.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Person {
    pub name: String,
    pub age: u8,
    pub email: Option<String>,
}

/// A nested struct containing other structs.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Rectangle {
    pub top_left: Point,
    pub bottom_right: Point,
    pub label: Option<String>,
}

/// A simple enum with unit variants.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Color {
    Red = 0,
    Green = 1,
    Blue = 2,
}

/// An enum with different payload types.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Shape {
    Circle { radius: f64 } = 0,
    Rectangle { width: f64, height: f64 } = 1,
    Point = 2,
}

/// A deeply nested structure for testing.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Canvas {
    pub name: String,
    pub shapes: Vec<Shape>,
    pub background: Color,
}

/// An enum with newtype variants.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Message {
    Text(String) = 0,
    Number(i64) = 1,
    Data(Vec<u8>) = 2,
}

/// Streaming service for cross-language conformance testing.
///
/// Tests Push/Pull semantics, stream lifecycle, and bidirectional streaming.
#[service]
pub trait Streaming {
    /// Client pushes numbers, server returns their sum.
    ///
    /// Tests: client-to-server streaming (`Push<T>` → scalar return).
    /// r[impl streaming.client-to-server] - Client sends stream, server returns scalar.
    async fn sum(&self, numbers: Pull<i32>) -> i64;

    /// Client sends a count, server returns that many numbers.
    ///
    /// Tests: server-to-client streaming (scalar → `Pull<T>`).
    /// r[impl streaming.server-to-client] - Client sends scalar, server returns stream.
    async fn range(&self, count: u32) -> Push<u32>;

    /// Client pushes strings, server echoes each back.
    ///
    /// Tests: bidirectional streaming (`Push<T>` ↔ `Pull<T>`).
    /// r[impl streaming.bidirectional] - Both sides stream simultaneously.
    async fn pipe(&self, input: Pull<String>) -> Push<String>;

    /// Client pushes numbers, server returns (sum, count, average).
    ///
    /// Tests: aggregating a stream into a compound result.
    async fn stats(&self, numbers: Pull<i32>) -> (i64, u64, f64);
}

/// Complex types service for testing struct/enum encoding.
///
/// Tests postcard encoding of nested structs, enums, and various types.
#[service]
pub trait Complex {
    /// Echo a point back.
    async fn echo_point(&self, point: Point) -> Point;

    /// Create a person and return it.
    async fn create_person(&self, name: String, age: u8, email: Option<String>) -> Person;

    /// Calculate the area of a rectangle.
    async fn rectangle_area(&self, rect: Rectangle) -> f64;

    /// Get a color by name.
    async fn parse_color(&self, name: String) -> Option<Color>;

    /// Calculate the area of a shape.
    async fn shape_area(&self, shape: Shape) -> f64;

    /// Create a canvas with given shapes.
    async fn create_canvas(&self, name: String, shapes: Vec<Shape>, background: Color) -> Canvas;

    /// Process a message and return a response.
    async fn process_message(&self, msg: Message) -> Message;

    /// Return multiple points.
    async fn get_points(&self, count: u32) -> Vec<Point>;

    /// Test tuple types.
    async fn swap_pair(&self, pair: (i32, String)) -> (String, i32);
}

pub fn all_services() -> Vec<roam::schema::ServiceDetail> {
    vec![
        echo_service_detail(),
        streaming_service_detail(),
        complex_service_detail(),
    ]
}
