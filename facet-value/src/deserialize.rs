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
#[cfg(feature = "diagnostics")]
use alloc::vec;
use alloc::vec::Vec;

#[cfg(feature = "diagnostics")]
use alloc::boxed::Box;

use facet_core::{Def, Facet, Shape, StructKind, Type, UserType};
use facet_reflect::{Partial, ReflectError};

use crate::{VNumber, Value, ValueType};

#[cfg(feature = "diagnostics")]
use alloc::borrow::Cow;
#[cfg(feature = "diagnostics")]
use facet_pretty::{PathSegment as ShapePathSegment, format_shape_with_spans};

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
    pub fn new(kind: ValueErrorKind) -> Self {
        Self {
            kind,
            source_path: Vec::new(),
            dest_path: Vec::new(),
            target_shape: None,
            source_value: None,
        }
    }

    /// Set the target shape for diagnostics
    pub fn with_shape(mut self, shape: &'static Shape) -> Self {
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

#[cfg(feature = "diagnostics")]
impl ValueError {
    /// Convert this error into a report that owns its diagnostic data
    pub fn into_report(self) -> ValueErrorReport {
        ValueErrorReport::new(self)
    }
}

/// A sub-diagnostic for a single source (JSON input or Rust target)
#[cfg(feature = "diagnostics")]
struct SourceDiagnostic {
    /// The source text (with syntax highlighting)
    source_text: String,
    /// The source name (e.g., "input.json" or "target.rs")
    source_name: String,
    /// Labels for this source
    labels: Vec<(usize, usize, String)>,
}

#[cfg(feature = "diagnostics")]
impl core::fmt::Debug for SourceDiagnostic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.source_name)
    }
}

#[cfg(feature = "diagnostics")]
impl core::fmt::Display for SourceDiagnostic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.source_name)
    }
}

#[cfg(feature = "diagnostics")]
impl core::error::Error for SourceDiagnostic {}

#[cfg(feature = "diagnostics")]
impl miette::Diagnostic for SourceDiagnostic {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source_text as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        if self.labels.is_empty() {
            None
        } else {
            Some(Box::new(self.labels.iter().map(|(start, end, label)| {
                miette::LabeledSpan::at(*start..*end, label.as_str())
            })))
        }
    }
}

/// A wrapper around ValueError that owns the diagnostic data for miette
#[cfg(feature = "diagnostics")]
pub struct ValueErrorReport {
    /// The original error
    error: ValueError,
    /// Related diagnostics (input JSON, target Rust type)
    related: Vec<SourceDiagnostic>,
}

#[cfg(feature = "diagnostics")]
impl ValueErrorReport {
    /// Create a new report from a ValueError
    pub fn new(error: ValueError) -> Self {
        use crate::format::{PathSegment as ValuePathSegment, format_value_with_spans};
        use crate::highlight::{highlight_json_with_spans, highlight_rust_with_spans};

        let mut related = Vec::new();

        // Format the source value with spans and syntax highlighting
        if let Some(ref value) = error.source_value {
            let result = format_value_with_spans(value);

            // Convert our PathSegment to format's PathSegment
            let value_path: Vec<ValuePathSegment> = error
                .source_path
                .iter()
                .map(|seg| match seg {
                    PathSegment::Field(name) => ValuePathSegment::Key(name.clone()),
                    PathSegment::Variant(name) => ValuePathSegment::Key(name.clone()),
                    PathSegment::Index(i) => ValuePathSegment::Index(*i),
                })
                .collect();

            let span = result
                .spans
                .get(&value_path)
                .copied()
                .unwrap_or((0, result.text.len().saturating_sub(1).max(1)));

            let label = match &error.kind {
                ValueErrorKind::TypeMismatch { got, .. } => {
                    alloc::format!("got {got:?}")
                }
                ValueErrorKind::NumberOutOfRange { message } => {
                    alloc::format!("this value: {message}")
                }
                ValueErrorKind::UnknownField { field } => {
                    alloc::format!("unknown field `{field}`")
                }
                _ => "this value".into(),
            };

            // Apply JSON syntax highlighting and convert span positions
            let plain_spans = vec![(span.0, span.1, label)];
            let (highlighted_text, converted_spans) =
                highlight_json_with_spans(&result.text, &plain_spans);

            related.push(SourceDiagnostic {
                source_text: highlighted_text,
                source_name: "input.json".into(),
                labels: converted_spans,
            });
        }

        // Format the target shape with spans and syntax highlighting
        if let Some(shape) = error.target_shape {
            let result = format_shape_with_spans(shape);

            // Only add if there's actual content
            if !result.text.is_empty() {
                // Convert our PathSegment to facet_pretty's PathSegment
                let shape_path: Vec<ShapePathSegment> = error
                    .dest_path
                    .iter()
                    .filter_map(|seg| match seg {
                        PathSegment::Field(name) => {
                            Some(ShapePathSegment::Field(Cow::Owned(name.clone())))
                        }
                        PathSegment::Variant(name) => {
                            Some(ShapePathSegment::Variant(Cow::Owned(name.clone())))
                        }
                        PathSegment::Index(_) => None,
                    })
                    .collect();

                let span = result
                    .spans
                    .get(&shape_path)
                    .map(|s| s.value)
                    .unwrap_or((0, result.text.len().saturating_sub(1).max(1)));

                let label = match &error.kind {
                    ValueErrorKind::TypeMismatch { expected, .. } => {
                        alloc::format!("expected {expected}")
                    }
                    ValueErrorKind::MissingField { field } => {
                        alloc::format!("missing field `{field}`")
                    }
                    ValueErrorKind::NumberOutOfRange { .. } => "for this type".into(),
                    _ => "target type".into(),
                };

                // Apply Rust syntax highlighting and convert span positions
                let plain_spans = vec![(span.0, span.1, label)];
                let (highlighted_text, converted_spans) =
                    highlight_rust_with_spans(&result.text, &plain_spans);

                related.push(SourceDiagnostic {
                    source_text: highlighted_text,
                    source_name: "target.rs".into(),
                    labels: converted_spans,
                });
            }
        }

        Self { error, related }
    }
}

#[cfg(feature = "diagnostics")]
impl core::fmt::Debug for ValueErrorReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.error, f)
    }
}

#[cfg(feature = "diagnostics")]
impl core::fmt::Display for ValueErrorReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.error, f)
    }
}

#[cfg(feature = "diagnostics")]
impl core::error::Error for ValueErrorReport {}

#[cfg(feature = "diagnostics")]
impl miette::Diagnostic for ValueErrorReport {
    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        if self.related.is_empty() {
            None
        } else {
            Some(Box::new(
                self.related.iter().map(|d| d as &dyn miette::Diagnostic),
            ))
        }
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(miette::Severity::Error)
    }
}

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
pub fn from_value<'facet, T: Facet<'facet>>(value: Value) -> Result<T> {
    let partial = Partial::alloc::<T>().map_err(|e| {
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
fn deserialize_value_into<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
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

    // Check for transparent/inner wrapper types
    if shape.inner.is_some() {
        partial = partial.begin_inner()?;
        partial = deserialize_value_into(value, partial)?;
        partial = partial.end()?;
        return Ok(partial);
    }

    // Check the Type for structs and enums
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

    // Check Def for containers and special types
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
fn deserialize_scalar<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
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
            set_number(num, partial, shape)
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
fn set_number<'p>(num: &VNumber, partial: Partial<'p>, shape: &Shape) -> Result<Partial<'p>> {
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
        Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
            let val = num.to_u64().ok_or_else(|| {
                ValueError::new(ValueErrorKind::NumberOutOfRange {
                    message: "value cannot be represented as u64".into(),
                })
            })?;
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
fn deserialize_struct<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
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

    let deny_unknown_fields = partial.shape().has_deny_unknown_fields_attr();

    // Track which fields we've set
    let num_fields = struct_def.fields.len();
    let mut fields_set = alloc::vec![false; num_fields];

    // Process each key-value pair in the object
    for (key, val) in obj.iter() {
        let key_str = key.as_str();

        // Find matching field
        let field_info = struct_def
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == key_str);

        if let Some((idx, _field)) = field_info {
            partial = partial.begin_field(key_str)?;
            partial = deserialize_value_into(val, partial)?;
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

/// Deserialize a tuple from a Value::Array.
fn deserialize_tuple<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
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
fn deserialize_enum<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
    let mut partial = partial;
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

            // Get the selected variant to determine how to deserialize
            let variant = partial.selected_variant().ok_or_else(|| {
                ValueError::new(ValueErrorKind::Unsupported {
                    message: "failed to get selected variant".into(),
                })
            })?;

            match variant.data.kind {
                StructKind::Unit => {
                    // Unit variant - val should be null
                    if !val.is_null() {
                        return Err(ValueError::new(ValueErrorKind::TypeMismatch {
                            expected: "null for unit variant",
                            got: val.value_type(),
                        }));
                    }
                }
                StructKind::TupleStruct | StructKind::Tuple => {
                    let num_fields = variant.data.fields.len();
                    if num_fields == 0 {
                        // Zero-field tuple variant, same as unit
                    } else if num_fields == 1 {
                        // Single-element tuple: value directly
                        partial = partial.begin_nth_field(0)?;
                        partial = deserialize_value_into(val, partial)?;
                        partial = partial.end()?;
                    } else {
                        // Multi-element tuple: array
                        let arr = val.as_array().ok_or_else(|| {
                            ValueError::new(ValueErrorKind::TypeMismatch {
                                expected: "array for tuple variant",
                                got: val.value_type(),
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
                    // Struct variant: object with named fields
                    let inner_obj = val.as_object().ok_or_else(|| {
                        ValueError::new(ValueErrorKind::TypeMismatch {
                            expected: "object for struct variant",
                            got: val.value_type(),
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
        other => Err(ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "string or object for enum",
            got: other,
        })),
    }
}

/// Deserialize a list/Vec from a Value::Array.
fn deserialize_list<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
    let mut partial = partial;
    let arr = value.as_array().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "array",
            got: value.value_type(),
        })
    })?;

    partial = partial.begin_list()?;

    for item in arr.iter() {
        partial = partial.begin_list_item()?;
        partial = deserialize_value_into(item, partial)?;
        partial = partial.end()?;
    }

    Ok(partial)
}

/// Deserialize a fixed-size array from a Value::Array.
fn deserialize_array<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
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
fn deserialize_set<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
    let mut partial = partial;
    let arr = value.as_array().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "array",
            got: value.value_type(),
        })
    })?;

    partial = partial.begin_set()?;

    for item in arr.iter() {
        partial = partial.begin_set_item()?;
        partial = deserialize_value_into(item, partial)?;
        partial = partial.end()?;
    }

    Ok(partial)
}

/// Deserialize a map from a Value::Object.
fn deserialize_map<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
    let mut partial = partial;
    let obj = value.as_object().ok_or_else(|| {
        ValueError::new(ValueErrorKind::TypeMismatch {
            expected: "object",
            got: value.value_type(),
        })
    })?;

    partial = partial.begin_map()?;

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
fn deserialize_option<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
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

/// Deserialize a smart pointer (Box, Arc, Rc) from a Value.
fn deserialize_pointer<'p>(value: &Value, partial: Partial<'p>) -> Result<Partial<'p>> {
    use facet_core::{KnownPointer, SequenceType};

    let mut partial = partial;
    let (is_slice_pointer, is_reference) = if let Def::Pointer(ptr_def) = partial.shape().def {
        let is_slice = if let Some(pointee) = ptr_def.pointee() {
            matches!(pointee.ty, Type::Sequence(SequenceType::Slice(_)))
        } else {
            false
        };
        let is_ref = matches!(
            ptr_def.known,
            Some(KnownPointer::SharedReference | KnownPointer::ExclusiveReference)
        );
        (is_slice, is_ref)
    } else {
        (false, false)
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
