//! This defines a few types showcasing various features of the Facet derive macro.

use facet::Facet;

/// A struct demonstrating various field types and attributes.
#[derive(Facet)]
pub struct KitchenSinkStruct {
    /// A basic string field.
    pub basic_field: String,
    /// A field marked as sensitive.
    #[facet(sensitive)]
    pub sensitive_field: u64,
    /// A tuple field.
    pub tuple_field: (i32, bool),
    /// An array field.
    pub array_field: [u8; 4],
    /// A static slice field.
    pub slice_field: &'static [u8],
    /// A vector field.
    pub vec_field: Vec<f32>,
    /// A field containing another struct that derives Facet.
    pub nested_struct_field: Point,
}

/// A simple point struct, also deriving Facet.
#[derive(Facet)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    /// Nested sensitive data within the struct.
    #[facet(sensitive)]
    pub metadata: String,
}

/// An enum demonstrating different variant types and attributes.
#[derive(Facet)]
#[repr(u8)]
pub enum KitchenSinkEnum {
    /// A simple unit variant.
    UnitVariant,
    /// A tuple variant with a single element.
    ///
    /// The contained `String` represents an important message payload.
    TupleVariantSimple(String),
    /// A tuple variant with multiple elements.
    ///
    /// Contains important positional data:
    /// - `_0` (i32): An identifier code.
    /// - `_1` (i32): A sequence number.
    /// - `_2` (i32): A status flag.
    TupleVariantMulti(i32, i32, i32),
    /// A struct variant with named fields.
    StructVariant {
        /// The width dimension, crucial for rendering.
        width: f64,
        /// The height dimension, also crucial for rendering.
        height: f64,
    },
    /// A tuple variant marked entirely as sensitive.
    #[facet(sensitive)]
    SensitiveTupleVariant(Vec<u8>),
    /// A struct variant containing a sensitive field.
    StructVariantWithSensitiveField {
        /// The main data payload, publicly accessible.
        payload: Vec<u8>,
        /// The sensitive checksum for integrity verification.
        #[facet(sensitive)]
        checksum: u32,
    },
    /// A variant marked as arbitrary, potentially skipped during processing.
    #[facet(arbitrary)]
    ArbitraryVariant((f64, f64)),
    /// A variant containing another enum that derives Facet.
    ///
    /// The nested `SubEnum` indicates a specific sub-state or option.
    NestedEnumVariant(SubEnum),
}

/// A sub-enum used within `KitchenSinkEnum`.
#[derive(Facet)]
#[repr(u8)]
pub enum SubEnum {
    /// Option A.
    OptionA,
    /// Option B with data.
    OptionB(u8),
    /// A sensitive option.
    #[facet(sensitive)]
    SensitiveOption(u64),
    /// An arbitrary option.
    #[facet(arbitrary)]
    ArbitraryOption(u8),
}
