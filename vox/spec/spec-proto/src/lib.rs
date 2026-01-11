#![deny(unsafe_code)]

use facet::Facet;
use roam::service;
use roam::session::{Rx, Tx};

/// Testbed service for conformance testing.
///
/// Combines unary, streaming, and complex type methods for comprehensive testing.
#[service]
pub trait Testbed {
    // ========================================================================
    // Unary methods
    // ========================================================================

    /// Echoes the message back.
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed.
    async fn reverse(&self, message: String) -> String;

    // ========================================================================
    // Streaming methods
    // ========================================================================

    /// Client sends numbers, server returns their sum.
    ///
    /// Tests: client→server streaming. Server receives via `Rx<T>`, returns scalar.
    async fn sum(&self, numbers: Rx<i32>) -> i64;

    /// Server streams numbers back to client.
    ///
    /// Tests: server→client streaming. Server sends via `Tx<T>`.
    async fn generate(&self, count: u32, output: Tx<i32>);

    /// Bidirectional: client sends strings, server echoes each back.
    ///
    /// Tests: bidirectional streaming. Server receives via `Rx<T>`, sends via `Tx<T>`.
    async fn transform(&self, input: Rx<String>, output: Tx<String>);

    // ========================================================================
    // Complex type methods
    // ========================================================================

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

pub fn all_services() -> Vec<roam::schema::ServiceDetail> {
    vec![testbed_service_detail()]
}
