#![deny(unsafe_code)]

pub mod evolved;

use facet::Facet;
use roam::service;
use roam::{Rx, Tx};

/// Testbed service for conformance testing.
///
/// Combines simple RPC, channeling, and complex type methods for comprehensive testing.
#[service]
pub trait Testbed {
    // ========================================================================
    // Simple RPC methods
    // ========================================================================

    /// Echoes the message back.
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed.
    async fn reverse(&self, message: String) -> String;

    // ========================================================================
    // Fallible methods (for testing User(E) error path)
    // ========================================================================

    /// Divides two numbers, returning an error if divisor is zero.
    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError>;

    /// Looks up a user by ID, returning an error if not found.
    async fn lookup(&self, id: u32) -> Result<Person, LookupError>;

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

    /// Server streams numbers back to client on a non-idempotent retry probe.
    ///
    /// Tests: channel retry fails closed when the session breaks mid-stream.
    async fn generate_retry_non_idem(&self, count: u32, output: Tx<i32>);

    /// Server streams numbers back to client on an idempotent retry probe.
    ///
    /// Tests: channel retry reruns the method with fresh channel bindings.
    #[roam(idem)]
    async fn generate_retry_idem(&self, count: u32, output: Tx<i32>);

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

    // ========================================================================
    // Schema evolution methods
    // ========================================================================

    /// Echo a profile back. Tests added optional field.
    async fn echo_profile(&self, profile: Profile) -> Profile;

    /// Echo a record back. Tests field reordering.
    async fn echo_record(&self, record: Record) -> Record;

    /// Echo a status back. Tests added enum variant.
    async fn echo_status(&self, status: Status) -> Status;

    /// Echo a tag back. Tests removed field (v2 drops a field v1 has).
    async fn echo_tag(&self, tag: Tag) -> Tag;

    /// Echo a measurement back. Tests incompatible type change.
    async fn echo_measurement(&self, m: Measurement) -> Measurement;

    /// Echo a config back. Tests missing required field.
    async fn echo_config(&self, c: Config) -> Config;
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

// ============================================================================
// Schema evolution types (v1 — the "original" definitions)
// ============================================================================

/// Tests added optional field: v1 has {name, bio}, v2 adds {avatar: `Option<String>`}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Profile {
    pub name: String,
    pub bio: String,
}

/// Tests field reordering: v1 has {alpha, beta, gamma}, v2 reorders to {gamma, alpha, beta}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Record {
    pub alpha: i32,
    pub beta: String,
    pub gamma: f64,
}

/// Tests added enum variant: v1 has {Active, Inactive}, v2 adds {Suspended}.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Inactive = 1,
}

/// Tests removed field: v1 has {label, priority, note}, v2 drops {note}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Tag {
    pub label: String,
    pub priority: u32,
    pub note: String,
}

/// Tests incompatible type change: v1 has {value: f64}, v2 changes to {value: String}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Measurement {
    pub unit: String,
    pub value: f64,
}

/// Tests missing required field: v1 has {key, value}, v2 adds required {owner: String}.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Config {
    pub key: String,
    pub value: String,
}

// ============================================================================
// Error types for testing User(E) error path
// ============================================================================

/// Error from math operations.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum MathError {
    DivisionByZero = 0,
    Overflow = 1,
}

/// Error from lookup operations.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum LookupError {
    NotFound = 0,
    AccessDenied = 1,
}

pub fn all_services() -> Vec<&'static roam::session::ServiceDescriptor> {
    vec![testbed_service_descriptor()]
}
