//! JSON-specific lowered program vocabulary.
//!
//! `weavy` owns the op-agnostic carrier: root programs, recursive blocks, and
//! stack-safe program execution. This module keeps the JSON semantics in
//! `facet-json`: field matching, scalar/string policy, enum representation, and
//! shape-specific operations that a future interpreter and copy-and-patch backend
//! must agree on.

use alloc::vec::Vec;

use facet_core::{ScalarType, Shape};

/// A lowered JSON deserialization program.
pub type JsonProgram = weavy::Program<JsonOp>;

/// A lowered JSON deserialization program with recursive shape blocks.
pub type JsonLowered = weavy::Lowered<JsonBlockId, JsonOp>;

/// Identifier for a recursive JSON deserialization block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JsonBlockId(usize);

impl JsonBlockId {
    /// Use a reflected shape as a stable block key for the current process.
    #[must_use]
    pub fn for_shape(shape: &'static Shape) -> Self {
        Self(shape as *const Shape as usize)
    }
}

/// One lowered JSON deserialization instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum JsonOp {
    /// Allocate or enter a value of this reflected shape.
    EnterShape { shape: &'static Shape },
    /// Read JSON `null` into unit-like or null-capable targets.
    Null,
    /// Read a scalar JSON value.
    Scalar(JsonScalar),
    /// Read a JSON string.
    String(JsonString),
    /// Read a bytes value using the active JSON bytes representation.
    Bytes(JsonBytes),
    /// Capture the source JSON value as raw JSON.
    RawJson,
    /// Deserialize a named-field object.
    Struct(JsonStruct),
    /// Deserialize a fixed tuple or tuple struct.
    Tuple { elements: Vec<JsonProgram> },
    /// Deserialize a variable-length sequence.
    List { element: JsonProgram },
    /// Deserialize a fixed-length array.
    Array { len: usize, element: JsonProgram },
    /// Deserialize a map.
    Map {
        key: JsonProgram,
        value: JsonProgram,
    },
    /// Deserialize a set.
    Set { element: JsonProgram },
    /// Deserialize an option.
    Option { some: JsonProgram },
    /// Deserialize a result.
    Result { ok: JsonProgram, err: JsonProgram },
    /// Deserialize an enum using one of Facet's JSON enum representations.
    Enum(JsonEnum),
    /// Deserialize through a transparent wrapper or proxy shape.
    Transparent { inner: JsonProgram },
    /// Deserialize a dynamic reflected value.
    Dynamic,
    /// Call a recursive shape block.
    CallBlock(JsonBlockId),
    /// Finish the current program.
    Return,
}

/// Scalar decoding policy for a JSON value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonScalar {
    /// Reflected scalar target, when the shape has one.
    pub ty: Option<ScalarType>,
    /// Whether string fallback through `FromStr` is allowed for this scalar.
    pub from_str: bool,
}

/// String decoding policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonString {
    /// Whether the decoded string may borrow from the input.
    pub borrow: JsonBorrow,
    /// Whether the string is the direct value or an input to scalar parsing.
    pub role: JsonStringRole,
}

/// Borrowing policy for input-backed JSON spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonBorrow {
    /// Materialize an owned value.
    Owned,
    /// Borrow when JSON escaping permits it.
    Borrowed,
}

/// Why a JSON string is being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonStringRole {
    /// The target is string-like.
    Value,
    /// The target is a map key.
    MapKey,
    /// The target is a variant tag.
    VariantTag,
}

/// Bytes representation accepted by JSON deserialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonBytes {
    /// JSON array of byte numbers.
    Array,
    /// Hex string.
    Hex,
}

/// Object-field plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonStruct {
    /// Expected fields in lowered planning order.
    pub fields: Vec<JsonField>,
    /// Unknown-field behavior for this object.
    pub unknown_fields: UnknownFields,
}

/// One named JSON field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonField {
    /// Primary serialized field name.
    pub name: &'static str,
    /// Alternate accepted field name, when `#[facet(alias = "...")]` is present.
    pub alias: Option<&'static str>,
    /// Field payload program.
    pub value: JsonProgram,
    /// How to handle this field when it is absent.
    pub missing: MissingField,
    /// Whether the field is flattened into its parent object.
    pub flattened: bool,
}

/// Unknown object field behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownFields {
    /// Skip unknown JSON fields.
    Ignore,
    /// Reject unknown JSON fields.
    Deny,
    /// Route unknown JSON fields into a flattened map field.
    Capture,
}

/// Missing-field behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingField {
    /// The field must be present in JSON.
    Required,
    /// Fill from the field's reflected default.
    Default,
    /// Leave unset because the reflected type can complete it.
    Omit,
}

/// Enum representation used by JSON deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonEnum {
    /// Shape-level representation.
    pub repr: JsonEnumRepr,
    /// Variant arms in declaration order.
    pub variants: Vec<JsonVariant>,
    /// Catch-all `#[facet(other)]` variant, if any.
    pub other: Option<JsonVariant>,
}

/// Facet enum representation as used by JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonEnumRepr {
    /// Variant name is the object key.
    ExternallyTagged,
    /// Variant tag is a field next to the variant fields.
    InternallyTagged { tag: &'static str },
    /// Tag and content are adjacent fields.
    AdjacentlyTagged {
        tag: &'static str,
        content: &'static str,
    },
    /// Untagged/flattened matching through the solver.
    Flattened,
}

impl JsonEnumRepr {
    /// Detect the JSON enum representation from a Facet shape.
    #[must_use]
    pub fn from_shape(shape: &'static Shape) -> Self {
        match (
            shape.is_untagged(),
            shape.get_tag_attr(),
            shape.get_content_attr(),
        ) {
            (true, _, _) => Self::Flattened,
            (false, Some(tag), Some(content)) => Self::AdjacentlyTagged { tag, content },
            (false, Some(tag), None) => Self::InternallyTagged { tag },
            (false, None, _) => Self::ExternallyTagged,
        }
    }
}

/// One enum variant arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonVariant {
    /// Effective serialized variant name.
    pub name: &'static str,
    /// Variant payload shape.
    pub payload: JsonVariantPayload,
}

/// Lowered enum payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonVariantPayload {
    /// Unit variant.
    Unit,
    /// Single-field payload.
    Newtype(JsonProgram),
    /// Tuple payload.
    Tuple(Vec<JsonProgram>),
    /// Struct payload.
    Struct(JsonStruct),
}
