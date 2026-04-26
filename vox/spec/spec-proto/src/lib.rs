#![deny(unsafe_code)]

pub mod evolved;

use facet::Facet;
use vox::service;
use vox::{Rx, Tx};

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

    /// Divides two numbers, returning an error if divisor is zero or would overflow.
    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError>;

    /// Looks up a user by ID.
    ///
    /// - IDs 1..=3: return Ok(Person)
    /// - IDs 100..=199: return Err(AccessDenied)
    /// - Anything else: return Err(NotFound)
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
    #[vox(idem)]
    async fn generate_retry_idem(&self, count: u32, output: Tx<i32>);

    /// Bidirectional: client sends strings, server echoes each back.
    ///
    /// Tests: bidirectional streaming. Server receives via `Rx<T>`, sends via `Tx<T>`.
    async fn transform(&self, input: Rx<String>, output: Tx<String>);

    /// Server returns before streaming numbers back to the client.
    ///
    /// Tests: callee-held `Tx<T>` outlives the unary method response.
    async fn post_reply_generate(&self, output: Tx<i32>);

    /// Server returns before receiving numbers from the client, then reports their sum.
    ///
    /// Tests: callee-held `Rx<T>` outlives the unary method response.
    async fn post_reply_sum(&self, input: Rx<i32>, result: Tx<i64>);

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

    /// Echo a deeply nested payload back unchanged.
    async fn echo_gnarly(&self, payload: GnarlyPayload) -> GnarlyPayload;

    /// Process a message and return a response.
    async fn process_message(&self, msg: Message) -> Message;

    /// Return multiple points.
    async fn get_points(&self, count: u32) -> Vec<Point>;

    /// Test tuple types.
    async fn swap_pair(&self, pair: (i32, String)) -> (String, i32);

    /// Echo raw bytes back. Tests Vec<u8> as a first-class arg/return type.
    async fn echo_bytes(&self, data: Vec<u8>) -> Vec<u8>;

    /// Echo a bool. Tests the bool primitive type.
    async fn echo_bool(&self, b: bool) -> bool;

    /// Echo a u64. Tests the u64 primitive type.
    async fn echo_u64(&self, n: u64) -> u64;

    /// Echo an optional string. Tests Option<String> directly.
    async fn echo_option_string(&self, s: Option<String>) -> Option<String>;

    /// Sum a large stream (tests channel credit/backpressure for > initial credit).
    ///
    /// Tests: channel flow control when sender must wait for credit grants.
    async fn sum_large(&self, numbers: Rx<i32>) -> i64;

    /// Generate a large stream (tests Tx backpressure with > initial credit items).
    ///
    /// Tests: server must wait for client to grant credit mid-stream.
    async fn generate_large(&self, count: u32, output: Tx<i32>);

    /// Return all three Color variants in a Vec, testing enum + vec round-trip.
    async fn all_colors(&self) -> Vec<Color>;

    /// Accept multiple args of different types; return a summary struct.
    /// Tests multi-arg encoding and struct return.
    async fn describe_point(&self, label: String, x: i32, y: i32, active: bool) -> TaggedPoint;

    /// Echo a nested enum back unchanged. Tests deep enum encoding.
    async fn echo_shape(&self, shape: Shape) -> Shape;

    /// Echo a status back. Tests simple enum with unit variants.
    async fn echo_status_v1(&self, status: Status) -> Status;

    /// Echo a tag back. Tests struct with String + u32 + String fields.
    async fn echo_tag_v1(&self, tag: Tag) -> Tag;

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

/// A point with a string label and an active flag.
/// Used to test multi-arg methods and varied field types.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TaggedPoint {
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub active: bool,
}

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

/// A key/value attribute for the gnarly payload benchmark.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct GnarlyAttr {
    pub key: String,
    pub value: String,
}

/// A nested enum used by the gnarly payload benchmark.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(u8)]
pub enum GnarlyKind {
    File {
        mime: String,
        tags: Vec<String>,
    } = 0,
    Directory {
        child_count: u32,
        children: Vec<String>,
    } = 1,
    Symlink {
        target: String,
        hops: Vec<u32>,
    } = 2,
}

/// An entry inside the gnarly payload benchmark fixture.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct GnarlyEntry {
    pub id: u64,
    pub parent: Option<u64>,
    pub name: String,
    pub path: String,
    pub attrs: Vec<GnarlyAttr>,
    pub chunks: Vec<Vec<u8>>,
    pub kind: GnarlyKind,
}

/// A deep, heterogenous payload for transport and codec benchmarking.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct GnarlyPayload {
    pub revision: u64,
    pub mount: String,
    pub entries: Vec<GnarlyEntry>,
    pub footer: Option<String>,
    pub digest: Vec<u8>,
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

pub fn all_services() -> Vec<&'static vox::session::ServiceDescriptor> {
    vec![testbed_service_descriptor()]
}
