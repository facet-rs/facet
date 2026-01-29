#![allow(clippy::result_large_err)]
//! Deserialize from a `Value` into any type implementing `Facet`.
//!
//! This module provides the inverse of serialization: given a `Value`, you can
//! deserialize it into any Rust type that implements `Facet`.
//!
//! # Example
//!
//! ```ignore
//! use facet::Facet;
//! use facet_value::{Value, from_value};
//!
//! #[derive(Debug, Facet, PartialEq)]
//! struct Person {
//!     name: String,
//!     age: u32,
//! }
//!
//! // Create a Value (could come from JSON, MessagePack, etc.)
//! let value = facet_value::value!({
//!     "name": "Alice",
//!     "age": 30
//! });
//!
//! // Deserialize into a typed struct
//! let person: Person = from_value(value).unwrap();
//! assert_eq!(person.name, "Alice");
//! assert_eq!(person.age, 30);
//! ```

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use bumpalo::Bump;
use facet_core::{
    Def, Facet, NumericType, PrimitiveType, Shape, StructKind, TextualType, Type, UserType, Variant,
};
use facet_reflect::{AllocError, Partial, ReflectError, ShapeMismatchError};

use crate::{VNumber, Value, ValueType};

/// A segment in a deserialization path
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathSegment {
    /// A field name in a struct or map
    Field(String),
    /// A variant name in an enum
    Variant(String),
    /// An index in an array or list
    Index(usize),
}

impl core::fmt::Display for PathSegment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PathSegment::Field(name) => write!(f, ".{name}"),
            PathSegment::Variant(name) => write!(f, "::{name}"),
            PathSegment::Index(i) => write!(f, "[{i}]"),
        }
    }
}

/// Error type for Value deserialization.
#[derive(Debug)]
pub struct ValueError {
    /// The specific kind of error
    pub kind: ValueErrorKind,
    /// Path through the source Value where the error occurred
    pub source_path: Vec<PathSegment>,
    /// Path through the target Shape where the error occurred
    pub dest_path: Vec<PathSegment>,
    /// The target Shape we were deserializing into (for diagnostics)
    pub target_shape: Option<&'static Shape>,
    /// The source Value we were deserializing from (for diagnostics)
    pub source_value: Option<Value>,
}

impl core::fmt::Display for ValueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.source_path.is_empty() {
            write!(f, "{}", self.kind)
        } else {
            write!(f, "at {}: {}", self.source_path_string(), self.kind)
        }
    }
}

impl ValueError {
    /// Create a new ValueError with empty paths
    pub const fn new(kind: ValueErrorKind) -> Self {
        Self {
            kind,
            source_path: Vec::new(),
            dest_path: Vec::new(),
            target_shape: None,
            source_value: None,
        }
    }

    /// Set the target shape for diagnostics
    pub const fn with_shape(mut self, shape: &'static Shape) -> Self {
        self.target_shape = Some(shape);
        self
    }

    /// Set the source value for diagnostics
    pub fn with_value(mut self, value: Value) -> Self {
        self.source_value = Some(value);
        self
    }

    /// Add a path segment to both paths (prepends since we unwind from error site)
    pub fn with_path(mut self, segment: PathSegment) -> Self {
        self.source_path.insert(0, segment.clone());
        self.dest_path.insert(0, segment);
        self
    }

    /// Format the source path as a string
    pub fn source_path_string(&self) -> String {
        if self.source_path.is_empty() {
            "<root>".into()
        } else {
            use core::fmt::Write;
            let mut s = String::new();
            for seg in &self.source_path {
                let _ = write!(s, "{seg}");
            }
            s
        }
    }

    /// Format the dest path as a string
    pub fn dest_path_string(&self) -> String {
        if self.dest_path.is_empty() {
            "<root>".into()
        } else {
            use core::fmt::Write;
            let mut s = String::new();
            for seg in &self.dest_path {
                let _ = write!(s, "{seg}");
            }
            s
        }
    }
}

#[cfg(feature = "std")]
impl core::error::Error for ValueError {}

/// Specific error kinds for Value deserialization.
#[derive(Debug)]
pub enum ValueErrorKind {
    /// Type mismatch between Value and target type
    TypeMismatch {
        /// What the target type expected
        expected: &'static str,
        /// What the Value actually contained
        got: ValueType,
    },
    /// A required field is missing from the object
    MissingField {
        /// The name of the missing field
        field: &'static str,
    },
    /// An unknown field was encountered (when deny_unknown_fields is set)
    UnknownField {
        /// The unknown field name
        field: String,
    },
    /// Number conversion failed (out of range)
    NumberOutOfRange {
        /// Description of the error
        message: String,
    },
    /// Reflection error from facet-reflect
    Reflect(ReflectError),
    /// Unsupported type or feature
    Unsupported {
        /// Description of what's unsupported
        message: String,
    },
}

impl core::fmt::Display for ValueErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ValueErrorKind::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got:?}")
            }
            ValueErrorKind::MissingField { field } => {
                write!(f, "missing required field `{field}`")
            }
            ValueErrorKind::UnknownField { field } => {
                write!(f, "unknown field `{field}`")
            }
            ValueErrorKind::NumberOutOfRange { message } => {
                write!(f, "number out of range: {message}")
            }
            ValueErrorKind::Reflect(e) => write!(f, "reflection error: {e}"),
            ValueErrorKind::Unsupported { message } => {
                write!(f, "unsupported: {message}")
            }
        }
    }
}

impl From<ReflectError> for ValueError {
    fn from(err: ReflectError) -> Self {
        ValueError::new(ValueErrorKind::Reflect(err))
    }
}

impl From<ShapeMismatchError> for ValueError {
    fn from(err: ShapeMismatchError) -> Self {
        ValueError::new(ValueErrorKind::Unsupported {
            message: format!(
                "shape mismatch: expected {}, got {}",
                err.expected, err.actual
            ),
        })
    }
}

impl From<AllocError> for ValueError {
    fn from(err: AllocError) -> Self {
        ValueError::new(ValueErrorKind::Unsupported {
            message: format!("allocation failed for {}: {}", err.shape, err.operation),
        })
    }
}

/// Result type for Value deserialization.
pub type Result<T> = core::result::Result<T, ValueError>;

/// Deserialize a `Value` into any type implementing `Facet`.
///
/// This is the main entry point for converting a dynamic `Value` into a
/// typed Rust value.
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
/// use facet_value::{Value, from_value};
///
/// #[derive(Debug, Facet, PartialEq)]
/// struct Point {
///     x: i32,
///     y: i32,
/// }
///
/// let value = facet_value::value!({"x": 10, "y": 20});
/// let point: Point = from_value(value).unwrap();
/// assert_eq!(point, Point { x: 10, y: 20 });
/// ```
pub fn from_value<T: Facet<'static>>(value: Value) -> Result<T> {
    let bump = Bump::new();
    let partial = Partial::alloc_owned::<T>(&bump).map_err(|e| {
        ValueError::from(e)
            .with_shape(T::SHAPE)
            .with_value(value.clone())
    })?;
    let partial = deserialize_value_into(&value, partial)
        .map_err(|e| e.with_shape(T::SHAPE).with_value(value.clone()))?;
    let heap_value = partial.build().map_err(|e| {
        ValueError::from(e)
            .with_shape(T::SHAPE)
            .with_value(value.clone())
    })?;
    heap_value.materialize().map_err(|e| {
        ValueError::from(e)
            .with_shape(T::SHAPE)
            .with_value(value.clone())
    })
}

/// Internal deserializer that reads from a Value and writes to a Partial.
fn deserialize_value_into<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let shape = partial.shape();

    // Check for Option first (it's also an enum but needs special handling)
    if matches!(&shape.def, Def::Option(_)) {
        return deserialize_option(value, partial);
    }

    // Check for smart pointers
    if matches!(&shape.def, Def::Pointer(_)) {
        return deserialize_pointer(value, partial);
    }

    // Check for container-level proxy
    #[cfg(feature = "alloc")]
    if shape.proxy.is_some() {
        let (partial_returned, has_proxy) = partial.begin_custom_deserialization_from_shape()?;
        partial = partial_returned;
        if has_proxy {
            partial = deserialize_value_into(value, partial)?;
            partial = partial.end()?;
            return Ok(partial);
        }
    }

    // Priority 1: Check for builder_shape (immutable collections like Bytes -> BytesMut)
    if shape.builder_shape.is_some() {
        partial = partial.begin_inner()?;
        partial = deserialize_value_into(value, partial)?;
        partial = partial.end()?;
        return Ok(partial);
    }

    // Priority 2: Check for .inner (transparent wrappers like NonZero)
    // Collections (List/Map/Set/Array) have .inner for variance but shouldn't use this path
    if shape.inner.is_some()
        && !matches!(
            &shape.def,
            Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
        )
    {
        partial = partial.begin_inner()?;
        partial = deserialize_value_into(value, partial)?;
        partial = partial.end()?;
        return Ok(partial);
    }

    // Priority 3: Check the Type for structs and enums
    match &shape.ty {
        Type::User(UserType::Struct(struct_def)) => {
            if struct_def.kind == StructKind::Tuple {
                return deserialize_tuple(value, partial);
            }
            return deserialize_struct(value, partial);
        }
        Type::User(UserType::Enum(_)) => return deserialize_enum(value, partial),
        _ => {}
    }

    // Priority 4: Check Def for containers and special types
    match &shape.def {
        Def::Scalar => deserialize_scalar(value, partial),
        Def::List(_) => deserialize_list(value, partial),
        Def::Map(_) => deserialize_map(value, partial),
        Def::Array(_) => deserialize_array(value, partial),
        Def::Set(_) => deserialize_set(value, partial),
        Def::DynamicValue(_) => {
            // Target is a DynamicValue (like Value itself) - just clone
            partial = partial.set(value.clone())?;
            Ok(partial)
        }
        _ => Err(ValueError::new(ValueErrorKind::Unsupported {
            message: format!("unsupported shape def: {:?}", shape.def),
        })),
    }
}

/// Deserialize a scalar value (primitives, strings).
fn deserialize_scalar<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let shape = partial.shape();

    match value.value_type() {
        ValueType::Null => {
            partial = partial.set_default()?;
            Ok(partial)
        }
        ValueType::Bool => {
            let b = value.as_bool().unwrap();
            partial = partial.set(b)?;
            Ok(partial)
        }
        ValueType::Number => {
            let num = value.as_number().unwrap();
            // If target expects a string, stringify the number
            // This is needed for formats like XML where type inference may produce
            // numbers even when strings are expected
            if *shape == *String::SHAPE {
                let s = if let Some(i) = num.to_i64() {
                    format!("{i}")
                } else if let Some(u) = num.to_u64() {
                    format!("{u}")
                } else if let Some(f) = num.to_f64() {
                    format!("{f}")
                } else {
                    return Err(ValueError::new(ValueErrorKind::TypeMismatch {
                        expected: "String",
                        got: ValueType::Number,
                    }));
                };
                partial = partial.set(s)?;
                Ok(partial)
            } else {
                set_number(num, partial, shape)
            }
        }
        ValueType::String => {
            let s = value.as_string().unwrap();
            // Try parse_from_str first if the type supports it
            if shape.vtable.has_parse() {
                partial = partial.parse_from_str(s.as_str())?;
            } else {
                partial = partial.set(s.as_str().to_string())?;
            }
            Ok(partial)
        }
        ValueType::Bytes => {
            let bytes = value.as_bytes().unwrap();
            partial = partial.set(bytes.as_slice().to_vec())?;
            Ok(partial)
        }
        other => Err(ValueError::new(ValueErrorKind::TypeMismatch {
            expected: shape.type_identifier,
            got: other,
        })),
    }
}

/// Set a numeric value with appropriate type conversion.
fn set_number<'facet, 'bump>(
    num: &VNumber,
    partial: Partial<'facet, 'bump, false>,
    shape: &Shape,
) -> Result<Partial<'facet, 'bump, false>> {
    use facet_core::{NumericType, PrimitiveType, ShapeLayout};

    let mut partial = partial;
    let size = match shape.layout {
        ShapeLayout::Sized(layout) => layout.size(),
        _ => {
            return Err(ValueError::new(ValueErrorKind::Unsupported {
                message: "unsized numeric type".into(),
            }));
        }
    };

    match &shape.ty {
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
            let val = num.to_i64().ok_or_else(|| {
                ValueError::new(ValueErrorKind::NumberOutOfRange {
                    message: "value cannot be represented as i64".into(),
                })
            })?;
            // Check shape to distinguish i64 from isize (both 8 bytes on 64-bit)
            if *shape == *isize::SHAPE {
                let v = isize::try_from(val).map_err(|_| {
                    ValueError::new(ValueErrorKind::NumberOutOfRange {
                        message: format!("{val} out of range for isize"),
                    })
                })?;
                partial = partial.set(v)?;
            } else {
                match size {
                    1 => {
                        let v = i8::try_from(val).map_err(|_| {
                            ValueError::new(ValueErrorKind::NumberOutOfRange {
                                message: format!("{val} out of range for i8"),
                            })
                        })?;
                        partial = partial.set(v)?;
                    }
                    2 => {
                        let v = i16::try_from(val).map_err(|_| {
                            ValueError::new(ValueErrorKind::NumberOutOfRange {
                                message: format!("{val} out of range for i16"),
                            })
                        })?;
                        partial = partial.set(v)?;
                    }
                    4 => {
                        let v = i32::try_from(val).map_err(|_| {
                            ValueError::new(ValueErrorKind::NumberOutOfRange {
                                message: format!("{val} out of range for i32"),
                            })
                        })?;
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(val)?;
                    }
                    16 => {
                        partial = partial.set(val as i128)?;
                    }
                    _ => {
                        return Err(ValueError::new(ValueErrorKind::Unsupported {
                            message: format!("unexpected integer size: {size}"),
                        }));
                    }
                }
            }
        }
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
            let val = num.to_u64().ok_or_else(|| {
                ValueError::new(ValueErrorKind::NumberOutOfRange {
                    message: "value cannot be represented as u64".into(),
                })
            })?;
            // Check shape to distinguish u64 from usize (both 8 bytes on 64-bit)
            if *shape == *usize::SHAPE {
                let v = usize::try_from(val).map_err(|_| {
                    ValueError::new(ValueErrorKind::NumberOutOfRange {
                        message: format!("{val} out of range for usize"),
                    })
                })?;
                partial = partial.set(v)?;
            } else {
                match size {
                    1 => {
                        let v = u8::try_from(val).map_err(|_| {
                            ValueError::new(ValueErrorKind::NumberOutOfRange {
                                message: format!("{val} out of range for u8"),
                            })
                        })?;
                        partial = partial.set(v)?;
                    }
                    2 => {
                        let v = u16::try_from(val).map_err(|_| {
                            ValueError::new(ValueErrorKind::NumberOutOfRange {
                                message: format!("{val} out of range for u16"),
                            })
                        })?;
                        partial = partial.set(v)?;
                    }
                    4 => {
                        let v = u32::try_from(val).map_err(|_| {
                            ValueError::new(ValueErrorKind::NumberOutOfRange {
                                message: format!("{val} out of range for u32"),
                            })
                        })?;
                        partial = partial.set(v)?;
                    }
                    8 => {
                        partial = partial.set(val)?;
                    }
                    16 => {
                        partial = partial.set(val as u128)?;
                    }
                    _ => {
                        return Err(ValueError::new(ValueErrorKind::Unsupported {
                            message: format!("unexpected integer size: {size}"),
                        }));
                    }
                }
            }
        }
        Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => {
            let val = num.to_f64_lossy();
            match size {
                4 => {
                    partial = partial.set(val as f32)?;
                }
                8 => {
                    partial = partial.set(val)?;
                }
                _ => {
                    return Err(ValueError::new(ValueErrorKind::Unsupported {
                        message: format!("unexpected float size: {size}"),
                    }));
                }
            }
        }
        _ => {
            return Err(ValueError::new(ValueErrorKind::TypeMismatch {
                expected: shape.type_identifier,
                got: ValueType::Number,
            }));
        }
    }
    Ok(partial)
}

/// Deserialize a struct from a Value::Object.
fn deserialize_struct<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let obj = value.as_object().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "object",
            got: value.value_type(),
        })
    })?;

    let struct_def = match &partial.shape().ty {
        Type::User(UserType::Struct(s)) => s,
        _ => {
            return Err(ValueError::new(ValueErrorKind::Unsupported {
                message: "expected struct type".into(),
            }));
        }
    };

    let deny_unknown_fields = partial.struct_plan().unwrap().deny_unknown_fields;

    // Check if we have any flattened fields
    let has_flattened = struct_def.fields.iter().any(|f| f.is_flattened());

    if has_flattened {
        return deserialize_struct_with_flatten(obj, partial, struct_def, deny_unknown_fields);
    }

    // Track which fields we've set
    let num_fields = struct_def.fields.len();
    let mut fields_set = alloc::vec![false; num_fields];

    // Process each key-value pair in the object
    for (key, val) in obj.iter() {
        let key_str = key.as_str();

        // Find matching field by effective_name (rename if present, else name) or alias
        let field_info = struct_def
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.effective_name() == key_str || f.alias == Some(key_str));

        if let Some((idx, field)) = field_info {
            partial = partial.begin_field(field.name)?;
            // Check for field-level proxy
            #[cfg(feature = "alloc")]
            if field.proxy_convert_in_fn().is_some() {
                partial = partial.begin_custom_deserialization()?;
                partial = deserialize_value_into(val, partial)?;
                partial = partial.end()?;
            } else {
                partial = deserialize_value_into(val, partial)?;
            }
            #[cfg(not(feature = "alloc"))]
            {
                partial = deserialize_value_into(val, partial)?;
            }
            partial = partial.end()?;
            fields_set[idx] = true;
        } else if deny_unknown_fields {
            return Err(ValueError::new(ValueErrorKind::UnknownField {
                field: key_str.to_string(),
            }));
        }
        // else: skip unknown field
    }

    // Handle missing fields - try to set defaults
    for (idx, field) in struct_def.fields.iter().enumerate() {
        if fields_set[idx] {
            continue;
        }

        // Try to set default for the field
        partial = partial
            .set_nth_field_to_default(idx)
            .map_err(|_| ValueError::new(ValueErrorKind::MissingField { field: field.name }))?;
    }

    Ok(partial)
}

/// Deserialize a struct that has flattened fields.
fn deserialize_struct_with_flatten<'facet, 'bump>(
    obj: &crate::VObject,
    mut partial: Partial<'facet, 'bump, false>,
    struct_def: &'static facet_core::StructType,
    deny_unknown_fields: bool,
) -> Result<Partial<'facet, 'bump, false>> {
    use alloc::collections::BTreeMap;

    let num_fields = struct_def.fields.len();
    let mut fields_set = alloc::vec![false; num_fields];

    // Collect which keys go to which flattened field
    // Key -> (flattened_field_idx, inner_field_name)
    let mut flatten_keys: BTreeMap<&str, (usize, &str)> = BTreeMap::new();

    // First pass: identify which keys belong to flattened fields
    for (idx, field) in struct_def.fields.iter().enumerate() {
        if !field.is_flattened() {
            continue;
        }

        // Get the inner struct's fields
        let inner_shape = field.shape.get();
        if let Type::User(UserType::Struct(inner_struct)) = &inner_shape.ty {
            for inner_field in inner_struct.fields.iter() {
                // Use the serialization name (rename if present, else name)
                let key_name = inner_field.rename.unwrap_or(inner_field.name);
                flatten_keys.insert(key_name, (idx, inner_field.name));
            }
        }
    }

    // Collect values for each flattened field
    let mut flatten_values: Vec<BTreeMap<String, Value>> =
        (0..num_fields).map(|_| BTreeMap::new()).collect();

    // Process each key-value pair in the object
    for (key, val) in obj.iter() {
        let key_str = key.as_str();

        // First, check for direct field match (non-flattened fields) by effective_name or alias
        let direct_field = struct_def.fields.iter().enumerate().find(|(_, f)| {
            !f.is_flattened() && (f.effective_name() == key_str || f.alias == Some(key_str))
        });

        if let Some((idx, field)) = direct_field {
            partial = partial.begin_field(field.name)?;
            // Check for field-level proxy
            #[cfg(feature = "alloc")]
            if field.proxy_convert_in_fn().is_some() {
                partial = partial.begin_custom_deserialization()?;
                partial = deserialize_value_into(val, partial)?;
                partial = partial.end()?;
            } else {
                partial = deserialize_value_into(val, partial)?;
            }
            #[cfg(not(feature = "alloc"))]
            {
                partial = deserialize_value_into(val, partial)?;
            }
            partial = partial.end()?;
            fields_set[idx] = true;
            continue;
        }

        // Check if this key belongs to a flattened field
        if let Some(&(flatten_idx, inner_name)) = flatten_keys.get(key_str) {
            flatten_values[flatten_idx].insert(inner_name.to_string(), val.clone());
            fields_set[flatten_idx] = true;
            continue;
        }

        // Unknown field
        if deny_unknown_fields {
            return Err(ValueError::new(ValueErrorKind::UnknownField {
                field: key_str.to_string(),
            }));
        }
        // else: skip unknown field
    }

    // Deserialize each flattened field from its collected values
    for (idx, field) in struct_def.fields.iter().enumerate() {
        if !field.is_flattened() {
            continue;
        }

        if !flatten_values[idx].is_empty() {
            // Build a synthetic Value::Object for this flattened field
            let mut synthetic_obj = crate::VObject::new();
            let values = core::mem::take(&mut flatten_values[idx]);
            for (k, v) in values {
                synthetic_obj.insert(k, v);
            }
            let synthetic_value = Value::from(synthetic_obj);

            partial = partial.begin_field(field.name)?;
            partial = deserialize_value_into(&synthetic_value, partial)?;
            partial = partial.end()?;
            fields_set[idx] = true;
        }
    }

    // Handle missing fields - try to set defaults
    for (idx, field) in struct_def.fields.iter().enumerate() {
        if fields_set[idx] {
            continue;
        }

        // Try to set default for the field
        partial = partial
            .set_nth_field_to_default(idx)
            .map_err(|_| ValueError::new(ValueErrorKind::MissingField { field: field.name }))?;
    }

    Ok(partial)
}

/// Deserialize a tuple from a Value::Array.
fn deserialize_tuple<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let arr = value.as_array().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "array",
            got: value.value_type(),
        })
    })?;

    let tuple_len = match &partial.shape().ty {
        Type::User(UserType::Struct(struct_def)) => struct_def.fields.len(),
        _ => {
            return Err(ValueError::new(ValueErrorKind::Unsupported {
                message: "expected tuple type".into(),
            }));
        }
    };

    if arr.len() != tuple_len {
        return Err(ValueError::new(ValueErrorKind::Unsupported {
            message: format!("tuple has {} elements but got {}", tuple_len, arr.len()),
        }));
    }

    for (i, item) in arr.iter().enumerate() {
        partial = partial.begin_nth_field(i)?;
        partial = deserialize_value_into(item, partial)?;
        partial = partial.end()?;
    }

    Ok(partial)
}

/// Deserialize an enum from a Value.
fn deserialize_enum<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let shape = partial.shape();

    // Check for numeric enums first (like #[repr(u8)] enums)
    if shape.is_numeric() {
        return deserialize_numeric_enum(value, partial);
    }

    if shape.is_untagged() {
        return deserialize_untagged_enum(value, partial);
    }

    let tag_key = shape.get_tag_attr();
    let content_key = shape.get_content_attr();

    match (tag_key, content_key) {
        // Internally tagged: {"type": "Circle", "radius": 5.0}
        (Some(tag_key), None) => deserialize_internally_tagged_enum(value, partial, tag_key),
        // Adjacently tagged: {"t": "Message", "c": "hello"}
        (Some(tag_key), Some(content_key)) => {
            deserialize_adjacently_tagged_enum(value, partial, tag_key, content_key)
        }
        // Externally tagged (default): {"VariantName": {...}}
        (None, None) => deserialize_externally_tagged_enum(value, partial),
        // Invalid: content without tag
        (None, Some(_)) => Err(ValueError::new(ValueErrorKind::Unsupported {
            message: "content key without tag key is invalid".into(),
        })),
    }
}

/// Deserialize a numeric enum from a Value::Number or Value::String.
///
/// Numeric enums use their discriminant value for serialization (e.g., `#[repr(u8)]` enums).
/// Accepts:
/// - Number values (i64/u64)
/// - String values that can be parsed as i64
fn deserialize_numeric_enum<'facet, 'bump>(
    value: &Value,
    mut partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let discriminant = match value.value_type() {
        ValueType::Number => {
            let num = value.as_number().unwrap();
            if let Some(i) = num.to_i64() {
                i
            } else {
                return Err(ValueError::new(ValueErrorKind::TypeMismatch {
                    expected: "Could not parse discriminant into i64", // TODO
                    got: ValueType::Number,
                }));
            }
        }
        ValueType::String => {
            // Parse string as i64 discriminant
            let s = value.as_string().unwrap().as_str();
            s.parse().map_err(|_| {
                ValueError::new(ValueErrorKind::TypeMismatch {
                    expected: "Failed to parse string into i64",
                    got: ValueType::String,
                })
            })?
        }
        other => {
            return Err(ValueError::new(ValueErrorKind::TypeMismatch {
                expected: "Expected number or string for numeric enum",
                got: other,
            }));
        }
    };

    partial = partial.select_variant(discriminant)?;
    Ok(partial)
}

/// Deserialize an externally tagged enum: {"VariantName": data} or "VariantName"
fn deserialize_externally_tagged_enum<'facet, 'bump>(
    value: &Value,
    mut partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    match value.value_type() {
        // String = unit variant
        ValueType::String => {
            let variant_name = value.as_string().unwrap().as_str();
            partial = partial.select_variant_named(variant_name)?;
            Ok(partial)
        }
        // Object = externally tagged variant with data
        ValueType::Object => {
            let obj = value.as_object().unwrap();
            if obj.len() != 1 {
                return Err(ValueError::new(ValueErrorKind::Unsupported {
                    message: format!("enum object must have exactly 1 key, got {}", obj.len()),
                }));
            }

            let (key, val) = obj.iter().next().unwrap();
            let variant_name = key.as_str();

            partial = partial.select_variant_named(variant_name)?;

            let variant = partial.selected_variant().ok_or_else(|| {
                ValueError::new(ValueErrorKind::Unsupported {
                    message: "failed to get selected variant".into(),
                })
            })?;

            populate_variant_from_value(val, partial, &variant)
        }
        other => Err(ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "string or object for enum",
            got: other,
        })),
    }
}

/// Deserialize an internally tagged enum: {"type": "Circle", "radius": 5.0}
fn deserialize_internally_tagged_enum<'facet, 'bump>(
    value: &Value,
    mut partial: Partial<'facet, 'bump, false>,
    tag_key: &str,
) -> Result<Partial<'facet, 'bump, false>> {
    let obj = value.as_object().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "object for internally tagged enum",
            got: value.value_type(),
        })
    })?;

    // Find the tag value
    let tag_value = obj.get(tag_key).ok_or_else(|| {
        ValueError::new(ValueErrorKind::Unsupported {
            message: format!("internally tagged enum missing tag key '{tag_key}'"),
        })
    })?;

    let variant_name = tag_value.as_string().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "string for enum tag",
            got: tag_value.value_type(),
        })
    })?;

    partial = partial.select_variant_named(variant_name.as_str())?;

    let variant = partial.selected_variant().ok_or_else(|| {
        ValueError::new(ValueErrorKind::Unsupported {
            message: "failed to get selected variant".into(),
        })
    })?;

    // For struct variants, deserialize the remaining fields (excluding the tag)
    match variant.data.kind {
        StructKind::Unit => {
            // Unit variant - just the tag, no other fields expected
            Ok(partial)
        }
        StructKind::Struct => {
            // Struct variant - deserialize fields from the same object (excluding tag)
            for field in variant.data.fields.iter() {
                if let Some(field_value) = obj.get(field.name) {
                    partial = partial.begin_field(field.name)?;
                    partial = deserialize_value_into(field_value, partial)?;
                    partial = partial.end()?;
                }
            }
            Ok(partial)
        }
        StructKind::TupleStruct | StructKind::Tuple => {
            Err(ValueError::new(ValueErrorKind::Unsupported {
                message: "internally tagged tuple variants are not supported".into(),
            }))
        }
    }
}

/// Deserialize an adjacently tagged enum: {"t": "Message", "c": "hello"}
fn deserialize_adjacently_tagged_enum<'facet, 'bump>(
    value: &Value,
    mut partial: Partial<'facet, 'bump, false>,
    tag_key: &str,
    content_key: &str,
) -> Result<Partial<'facet, 'bump, false>> {
    let obj = value.as_object().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "object for adjacently tagged enum",
            got: value.value_type(),
        })
    })?;

    // Find the tag value
    let tag_value = obj.get(tag_key).ok_or_else(|| {
        ValueError::new(ValueErrorKind::Unsupported {
            message: format!("adjacently tagged enum missing tag key '{tag_key}'"),
        })
    })?;

    let variant_name = tag_value.as_string().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "string for enum tag",
            got: tag_value.value_type(),
        })
    })?;

    partial = partial.select_variant_named(variant_name.as_str())?;

    let variant = partial.selected_variant().ok_or_else(|| {
        ValueError::new(ValueErrorKind::Unsupported {
            message: "failed to get selected variant".into(),
        })
    })?;

    // For non-unit variants, get the content
    match variant.data.kind {
        StructKind::Unit => {
            // Unit variant - no content field needed
            Ok(partial)
        }
        _ => {
            // Get the content value
            let content_value = obj.get(content_key).ok_or_else(|| {
                ValueError::new(ValueErrorKind::Unsupported {
                    message: format!("adjacently tagged enum missing content key '{content_key}'"),
                })
            })?;

            populate_variant_from_value(content_value, partial, &variant)
        }
    }
}

fn deserialize_untagged_enum<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let shape = partial.shape();
    let enum_type = match &shape.ty {
        Type::User(UserType::Enum(enum_def)) => enum_def,
        _ => {
            return Err(ValueError::new(ValueErrorKind::Unsupported {
                message: "expected enum type".into(),
            }));
        }
    };

    for variant in enum_type.variants.iter() {
        if value_matches_variant(value, variant) {
            partial = partial.select_variant_named(variant.effective_name())?;
            return populate_variant_from_value(value, partial, variant);
        }
    }

    Err(ValueError::new(ValueErrorKind::TypeMismatch {
        expected: shape.type_identifier,
        got: value.value_type(),
    }))
}

fn populate_variant_from_value<'facet, 'bump>(
    value: &Value,
    mut partial: Partial<'facet, 'bump, false>,
    variant: &Variant,
) -> Result<Partial<'facet, 'bump, false>> {
    match variant.data.kind {
        StructKind::Unit => {
            if !matches!(value.value_type(), ValueType::Null) {
                return Err(ValueError::new(ValueErrorKind::TypeMismatch {
                    expected: "null for unit variant",
                    got: value.value_type(),
                }));
            }
        }
        StructKind::TupleStruct | StructKind::Tuple => {
            let num_fields = variant.data.fields.len();
            if num_fields == 0 {
                // nothing to populate
            } else if num_fields == 1 {
                partial = partial.begin_nth_field(0)?;
                partial = deserialize_value_into(value, partial)?;
                partial = partial.end()?;
            } else {
                let arr = value.as_array().ok_or_else(|| {
                    ValueError::new(ValueErrorKind::TypeMismatch {
                        expected: "array for tuple variant",
                        got: value.value_type(),
                    })
                })?;

                if arr.len() != num_fields {
                    return Err(ValueError::new(ValueErrorKind::Unsupported {
                        message: format!(
                            "tuple variant has {} fields but got {}",
                            num_fields,
                            arr.len()
                        ),
                    }));
                }

                for (i, item) in arr.iter().enumerate() {
                    partial = partial.begin_nth_field(i)?;
                    partial = deserialize_value_into(item, partial)?;
                    partial = partial.end()?;
                }
            }
        }
        StructKind::Struct => {
            let inner_obj = value.as_object().ok_or_else(|| {
                ValueError::new(ValueErrorKind::TypeMismatch {
                    expected: "object for struct variant",
                    got: value.value_type(),
                })
            })?;

            for (field_key, field_val) in inner_obj.iter() {
                partial = partial.begin_field(field_key.as_str())?;
                partial = deserialize_value_into(field_val, partial)?;
                partial = partial.end()?;
            }
        }
    }

    Ok(partial)
}

fn value_matches_variant(value: &Value, variant: &Variant) -> bool {
    match variant.data.kind {
        StructKind::Unit => matches!(value.value_type(), ValueType::Null),
        StructKind::TupleStruct | StructKind::Tuple => {
            let fields = variant.data.fields;
            if fields.is_empty() {
                matches!(value.value_type(), ValueType::Null)
            } else if fields.len() == 1 {
                value_matches_shape(value, fields[0].shape.get())
            } else {
                value
                    .as_array()
                    .map(|arr| arr.len() == fields.len())
                    .unwrap_or(false)
            }
        }
        StructKind::Struct => matches!(value.value_type(), ValueType::Object),
    }
}

fn value_matches_shape(value: &Value, shape: &'static Shape) -> bool {
    match &shape.ty {
        Type::Primitive(PrimitiveType::Boolean) => {
            matches!(value.value_type(), ValueType::Bool)
        }
        Type::Primitive(PrimitiveType::Numeric(num)) => match num {
            NumericType::Integer { signed } => {
                if *signed {
                    value.as_number().and_then(|n| n.to_i64()).is_some()
                } else {
                    value.as_number().and_then(|n| n.to_u64()).is_some()
                }
            }
            NumericType::Float => value.as_number().and_then(|n| n.to_f64()).is_some(),
        },
        _ => true,
    }
}

/// Deserialize a list/Vec from a Value::Array.
fn deserialize_list<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let arr = value.as_array().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "array",
            got: value.value_type(),
        })
    })?;

    partial = partial.init_list()?;

    for item in arr.iter() {
        partial = partial.begin_list_item()?;
        partial = deserialize_value_into(item, partial)?;
        partial = partial.end()?;
    }

    Ok(partial)
}

/// Deserialize a fixed-size array from a Value::Array.
fn deserialize_array<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let arr = value.as_array().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "array",
            got: value.value_type(),
        })
    })?;

    let array_len = match &partial.shape().def {
        Def::Array(arr_def) => arr_def.n,
        _ => {
            return Err(ValueError::new(ValueErrorKind::Unsupported {
                message: "expected array type".into(),
            }));
        }
    };

    if arr.len() != array_len {
        return Err(ValueError::new(ValueErrorKind::Unsupported {
            message: format!(
                "fixed array has {} elements but got {}",
                array_len,
                arr.len()
            ),
        }));
    }

    for (i, item) in arr.iter().enumerate() {
        partial = partial.begin_nth_field(i)?;
        partial = deserialize_value_into(item, partial)?;
        partial = partial.end()?;
    }

    Ok(partial)
}

/// Deserialize a set from a Value::Array.
fn deserialize_set<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let arr = value.as_array().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "array",
            got: value.value_type(),
        })
    })?;

    partial = partial.init_set()?;

    for item in arr.iter() {
        partial = partial.begin_set_item()?;
        partial = deserialize_value_into(item, partial)?;
        partial = partial.end()?;
    }

    Ok(partial)
}

/// Deserialize a map from a Value::Object.
fn deserialize_map<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    let obj = value.as_object().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "object",
            got: value.value_type(),
        })
    })?;

    partial = partial.init_map()?;

    for (key, val) in obj.iter() {
        // Set the key
        partial = partial.begin_key()?;
        // For map keys, we need to handle the key type
        // Most commonly it's String, but could be other types with inner
        if partial.shape().inner.is_some() {
            partial = partial.begin_inner()?;
            partial = partial.set(key.as_str().to_string())?;
            partial = partial.end()?;
        } else {
            partial = partial.set(key.as_str().to_string())?;
        }
        partial = partial.end()?;

        // Set the value
        partial = partial.begin_value()?;
        partial = deserialize_value_into(val, partial)?;
        partial = partial.end()?;
    }

    Ok(partial)
}

/// Deserialize an Option from a Value.
fn deserialize_option<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    let mut partial = partial;
    if value.is_null() {
        partial = partial.set_default()?; // None
    } else {
        partial = partial.begin_some()?;
        partial = deserialize_value_into(value, partial)?;
        partial = partial.end()?;
    }
    Ok(partial)
}

/// Deserialize a smart pointer (Box, Arc, Rc) or Cow from a Value.
fn deserialize_pointer<'facet, 'bump>(
    value: &Value,
    partial: Partial<'facet, 'bump, false>,
) -> Result<Partial<'facet, 'bump, false>> {
    use facet_core::{KnownPointer, SequenceType};

    let mut partial = partial;
    let (is_slice_pointer, is_reference, is_cow) =
        if let Def::Pointer(ptr_def) = partial.shape().def {
            let is_slice = if let Some(pointee) = ptr_def.pointee() {
                matches!(pointee.ty, Type::Sequence(SequenceType::Slice(_)))
            } else {
                false
            };
            let is_ref = matches!(
                ptr_def.known,
                Some(KnownPointer::SharedReference | KnownPointer::ExclusiveReference)
            );
            let is_cow = matches!(ptr_def.known, Some(KnownPointer::Cow));
            (is_slice, is_ref, is_cow)
        } else {
            (false, false, false)
        };

    // References can't be deserialized (need existing data to borrow from)
    if is_reference {
        return Err(ValueError::new(ValueErrorKind::Unsupported {
            message: format!(
                "cannot deserialize into reference type '{}'",
                partial.shape().type_identifier
            ),
        }));
    }

    // Cow needs special handling
    if is_cow {
        // Check if this is Cow<str> - we can set it directly from a string value
        if let Def::Pointer(ptr_def) = partial.shape().def
            && let Some(pointee) = ptr_def.pointee()
            && matches!(
                pointee.ty,
                Type::Primitive(PrimitiveType::Textual(TextualType::Str))
            )
        {
            // This is Cow<str> - deserialize from string
            if let Some(s) = value.as_string() {
                // Set the owned string value - Cow<str> will store it as Owned
                partial = partial.set(alloc::borrow::Cow::<'static, str>::Owned(
                    s.as_str().to_string(),
                ))?;
                return Ok(partial);
            } else {
                return Err(ValueError::new(ValueErrorKind::TypeMismatch {
                    expected: "string for Cow<str>",
                    got: value.value_type(),
                }));
            }
        }
        // For other Cow types, use begin_inner
        partial = partial.begin_inner()?;
        partial = deserialize_value_into(value, partial)?;
        partial = partial.end()?;
        return Ok(partial);
    }

    partial = partial.begin_smart_ptr()?;

    if is_slice_pointer {
        // This is a slice pointer like Arc<[T]> - deserialize as array
        let arr = value.as_array().ok_or_else(|| {
            ValueError::new(ValueErrorKind::TypeMismatch {
                expected: "array",
                got: value.value_type(),
            })
        })?;

        for item in arr.iter() {
            partial = partial.begin_list_item()?;
            partial = deserialize_value_into(item, partial)?;
            partial = partial.end()?;
        }
    } else {
        // Regular smart pointer - deserialize the inner type
        partial = deserialize_value_into(value, partial)?;
    }

    partial = partial.end()?;
    Ok(partial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VArray, VObject, VString};

    #[test]
    fn test_deserialize_primitives() {
        // bool
        let v = Value::TRUE;
        let b: bool = from_value(v).unwrap();
        assert!(b);

        // i32
        let v = Value::from(42i64);
        let n: i32 = from_value(v).unwrap();
        assert_eq!(n, 42);

        // String
        let v: Value = VString::new("hello").into();
        let s: String = from_value(v).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_deserialize_option() {
        // Some
        let v = Value::from(42i64);
        let opt: Option<i32> = from_value(v).unwrap();
        assert_eq!(opt, Some(42));

        // None
        let v = Value::NULL;
        let opt: Option<i32> = from_value(v).unwrap();
        assert_eq!(opt, None);
    }

    #[test]
    fn test_deserialize_vec() {
        let mut arr = VArray::new();
        arr.push(Value::from(1i64));
        arr.push(Value::from(2i64));
        arr.push(Value::from(3i64));

        let v: Value = arr.into();
        let vec: alloc::vec::Vec<i32> = from_value(v).unwrap();
        assert_eq!(vec, alloc::vec![1, 2, 3]);
    }

    #[test]
    fn test_deserialize_nested() {
        // Vec<Option<i32>>
        let mut arr = VArray::new();
        arr.push(Value::from(1i64));
        arr.push(Value::NULL);
        arr.push(Value::from(3i64));

        let v: Value = arr.into();
        let vec: alloc::vec::Vec<Option<i32>> = from_value(v).unwrap();
        assert_eq!(vec, alloc::vec![Some(1), None, Some(3)]);
    }

    #[test]
    fn test_deserialize_map() {
        use alloc::collections::BTreeMap;

        let mut obj = VObject::new();
        obj.insert("a", Value::from(1i64));
        obj.insert("b", Value::from(2i64));

        let v: Value = obj.into();
        let map: BTreeMap<String, i32> = from_value(v).unwrap();
        assert_eq!(map.get("a"), Some(&1));
        assert_eq!(map.get("b"), Some(&2));
    }
}
