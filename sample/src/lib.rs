//! This defines just a bunch of types so that we can see what the generated output looks like

use facet::Facet;

/// A struct with a couple fields
#[derive(Facet)]
pub struct FooBar {
    pub foo: String,
    pub bar: u32,
}

/// Represents different types of messages
#[derive(Facet)]
#[repr(u8)]
pub enum Message {
    /// Simple notification without data
    Quit,
    /// Movement information
    #[facet(sensitive)]
    Move { x: i32, y: i32 },
    /// Text message with content
    Write(String),
    /// Color change request
    ChangeColor(i32, i32, i32),
}

/// Represents geometric shapes
#[derive(Facet)]
#[repr(u8)]
pub enum Shape {
    /// A circle with radius
    Circle(f64),
    /// A rectangle with width and height
    Rectangle { width: f64, height: f64 },
    /// A triangle with three points
    #[facet(arbitrary)]
    Triangle((f64, f64), (f64, f64), (f64, f64)),
}

/// Network packet types
#[derive(Facet)]
#[repr(u8)]
pub enum Packet {
    /// Data packet with payload
    #[facet(sensitive)]
    Data { payload: Vec<u8>, checksum: u32 },
    /// Control packet
    Control(PacketType, u16),
    /// Array of bytes representing the header
    Header([u8; 4]),
    /// Slice of the packet buffer
    Fragment(&'static [u8]),
}

/// Different types of control packets
#[derive(Facet)]
#[repr(u8)]
pub enum PacketType {
    /// Acknowledgment packet
    Ack,
    /// Negative acknowledgment
    Nack,
    /// Synchronization packet
    #[facet(sensitive)]
    Sync(u64),
    /// Reset connection
    Reset,
}

/// Events in a system
#[derive(Facet)]
#[repr(u8)]
pub enum SystemEvent {
    /// Timer events with duration
    Timer {
        #[facet(sensitive)]
        duration_ms: u64,
        repeating: bool,
    },
    /// IO events
    IO(IOType),
    /// User interaction events
    #[facet(arbitrary)]
    UserInput(UserInputType),
    /// System signals with array of parameters
    Signal([i32; 3]),
}

/// Types of IO operations
#[derive(Facet)]
#[repr(u8)]
pub enum IOType {
    /// Read operation
    Read,
    /// Write operation with data
    #[facet(sensitive)]
    Write(Vec<u8>),
    /// Both read and write
    ReadWrite { buffer_size: usize, timeout_ms: u32 },
}

/// User input types
#[derive(Facet)]
#[repr(u8)]
pub enum UserInputType {
    /// Keyboard input
    Keyboard { key: char, modifiers: u8 },
    /// Mouse event
    #[facet(sensitive)]
    Mouse(i32, i32, MouseButton),
    /// Touch event with coordinates array
    Touch([TouchPoint; 5]),
}

/// Mouse button types
#[derive(Facet)]
#[repr(u8)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    #[facet(arbitrary)]
    Extra(u8),
}

/// Represents a point of touch on a screen
#[derive(Facet)]
pub struct TouchPoint {
    pub x: f32,
    pub y: f32,
    #[facet(sensitive)]
    pub pressure: f32,
}
