// --- Deserializer Trait Definition ---

use facet_core::StructKind;
use facet_reflect::HeapValue;
use log::trace;

/// A trait for implementing format-specific deserialization logic.
/// The core iterative deserializer uses this trait to receive data.
pub trait Deserializer {
    /// The error type returned by deserialization methods
    type Error: std::error::Error;

    /// Deserialize an unsigned 8-bit integer.
    fn deserialize_u8(&mut self) -> Result<u8, Self::Error>;

    /// Deserialize an unsigned 16-bit integer.
    fn deserialize_u16(&mut self) -> Result<u16, Self::Error>;

    /// Deserialize an unsigned 32-bit integer.
    fn deserialize_u32(&mut self) -> Result<u32, Self::Error>;

    /// Deserialize an unsigned 64-bit integer.
    fn deserialize_u64(&mut self) -> Result<u64, Self::Error>;

    /// Deserialize an unsigned 128-bit integer.
    fn deserialize_u128(&mut self) -> Result<u128, Self::Error>;

    /// Deserialize a `usize` integer.
    fn deserialize_usize(&mut self) -> Result<usize, Self::Error>;

    /// Deserialize a signed 8-bit integer.
    fn deserialize_i8(&mut self) -> Result<i8, Self::Error>;

    /// Deserialize a signed 16-bit integer.
    fn deserialize_i16(&mut self) -> Result<i16, Self::Error>;

    /// Deserialize a signed 32-bit integer.
    fn deserialize_i32(&mut self) -> Result<i32, Self::Error>;

    /// Deserialize a signed 64-bit integer.
    fn deserialize_i64(&mut self) -> Result<i64, Self::Error>;

    /// Deserialize a signed 128-bit integer.
    fn deserialize_i128(&mut self) -> Result<i128, Self::Error>;

    /// Deserialize an `isize` integer.
    fn deserialize_isize(&mut self) -> Result<isize, Self::Error>;

    /// Deserialize a single-precision floating-point value.
    fn deserialize_f32(&mut self) -> Result<f32, Self::Error>;

    /// Deserialize a double-precision floating-point value.
    fn deserialize_f64(&mut self) -> Result<f64, Self::Error>;

    /// Deserialize a boolean value.
    fn deserialize_bool(&mut self) -> Result<bool, Self::Error>;

    /// Deserialize a character.
    fn deserialize_char(&mut self) -> Result<char, Self::Error>;

    /// Deserialize a string value to an owned String.
    fn deserialize_string(&mut self) -> Result<String, Self::Error>;

    /// Deserialize bytes to a `Vec<u8>`.
    fn deserialize_bytes(&mut self) -> Result<Vec<u8>, Self::Error>;

    // Special values

    /// Check if the current value is None.
    fn is_none(&mut self) -> Result<bool, Self::Error>;

    /// Deserialize a unit value `()`.
    fn deserialize_unit(&mut self) -> Result<(), Self::Error>;

    // Enum specific values

    /// Get the variant index and name for an enum.
    fn get_variant(&mut self) -> Result<String, Self::Error>;

    /// Begin deserializing an object. Returns an optional size hint.
    fn start_object(&mut self) -> Result<Option<usize>, Self::Error>;

    /// End deserializing an object.
    fn end_object(&mut self) -> Result<(), Self::Error>;

    /// Begin deserializing an array. Returns an optional size hint.
    fn start_array(&mut self) -> Result<Option<usize>, Self::Error>;

    /// End deserializing an array.
    fn end_array(&mut self) -> Result<(), Self::Error>;

    /// Begin deserializing a map. Returns an optional size hint.
    fn start_map(&mut self) -> Result<Option<usize>, Self::Error>;

    /// End deserializing a map.
    fn end_map(&mut self) -> Result<(), Self::Error>;

    /// Get the next field name in an object. Returns an optional size hint.
    fn next_field_name(&mut self) -> Result<Option<String>, Self::Error>;

    /// Check if there are more elements to process.
    fn has_next(&mut self) -> Result<bool, Self::Error>;

    /// Skip the next value in the input.
    fn skip_value(&mut self) -> Result<(), Self::Error>;
}

// --- Iterative Deserialization Logic ---

/// Error type for deserialization failures.
#[derive(Debug, PartialEq, Clone)]
pub enum DeserializeError<E> {
    /// An error from the underlying deserializer.
    Format(E),
    /// An error from the reflection system.
    Reflect(facet_reflect::ReflectError),
    UnknownField {
        field_name: String,
        shape: &'static facet_core::Shape,
    },
    /// A custom error with a message.
    Custom(String),
}

impl<E> core::fmt::Display for DeserializeError<E>
where
    E: core::fmt::Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeserializeError::Format(err) => write!(f, "Format error: {err}"),
            DeserializeError::Reflect(err) => write!(f, "Reflect error: {err}"),
            DeserializeError::UnknownField { field_name, shape } => {
                write!(f, "{shape} doesn't have a field named `{field_name}`")
            }
            DeserializeError::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

#[cfg(feature = "std")]
impl<E> std::error::Error for DeserializeError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DeserializeError::Format(err) => Some(err),
            _ => None,
        }
    }
}

impl<E> From<facet_reflect::ReflectError> for DeserializeError<E> {
    fn from(err: facet_reflect::ReflectError) -> Self {
        DeserializeError::Reflect(err)
    }
}

/// Result type for deserialization operations.
pub type DeserializeResult<T, E> = Result<T, DeserializeError<E>>;

/// Deserializes a value using the provided `Deserializer` into the provided `Wip` instance.
///
/// This function uses an iterative approach with a stack to avoid recursion depth limits.
pub fn deserialize_iterative<'facet, D, T>(
    wip: facet_reflect::Wip<'facet>,
    deserializer: &mut D,
) -> DeserializeResult<HeapValue<'facet>, D::Error>
where
    D: Deserializer,
    T: facet_core::Facet<'facet>,
{
    let runner = Runner {
        stack: vec![DeserializeOp::Value],
        wip,
        deserializer,
    };

    let heap_value = runner.run()?;

    Ok(heap_value)
}

#[derive(Debug)]
enum DeserializeOp {
    Value,
    SkipValue,
    End(EndContext),
    ObjectKey,
    ObjectComma,
    ArrayItem,
    ArrayComma,
    Pop,
}

#[derive(Debug)]
enum EndContext {
    Object,
    Array,
    Map,
}

// State machine runner for deserialization
struct Runner<'a, 'f, D: Deserializer> {
    stack: Vec<DeserializeOp>,
    wip: facet_reflect::Wip<'f>,
    deserializer: &'a mut D,
}

impl<'f, D: Deserializer> Runner<'_, 'f, D> {
    fn run(mut self) -> DeserializeResult<facet_reflect::HeapValue<'f>, D::Error> {
        trace!("Starting deserialization");

        while let Some(op) = self.stack.pop() {
            let frame_count = self.wip.frames_count();
            debug_assert!(
                frame_count
                    >= self
                        .stack
                        .iter()
                        .filter(|f| matches!(f, DeserializeOp::End(_)))
                        .count()
            );

            match op {
                DeserializeOp::Value => self = self.handle_value()?,
                DeserializeOp::SkipValue => self
                    .deserializer
                    .skip_value()
                    .map_err(DeserializeError::Format)?,
                DeserializeOp::End(context) => match context {
                    EndContext::Object => {
                        self.deserializer
                            .end_object()
                            .map_err(DeserializeError::Format)?;

                        // After processing the entire object, check for uninitialized fields
                        // and set defaults where necessary
                        self = self.check_and_apply_defaults()?;
                    }
                    EndContext::Array => {
                        self.deserializer
                            .end_array()
                            .map_err(DeserializeError::Format)?;
                    }
                    EndContext::Map => {
                        self.deserializer
                            .end_map()
                            .map_err(DeserializeError::Format)?;
                    }
                },
                DeserializeOp::ObjectKey => self = self.handle_object_key()?,
                DeserializeOp::ObjectComma => self = self.handle_object_comma()?,
                DeserializeOp::ArrayItem => self = self.handle_array_item()?,
                DeserializeOp::ArrayComma => self = self.handle_array_comma()?,
                DeserializeOp::Pop => self.wip = self.wip.pop()?,
            }
        }

        self.wip.build().map_err(Into::into)
    }

    fn check_and_apply_defaults(mut self) -> DeserializeResult<Self, D::Error> {
        trace!("Checking for uninitialized fields that have defaults");

        let container_shape = self.wip.shape();
        if let facet_core::Def::Struct(sd) = container_shape.def {
            for (index, field) in sd.fields.iter().enumerate() {
                // Check if the field is set
                let is_set = self.wip.is_field_set(index).map_err(|e| {
                    DeserializeError::Custom(format!("Failed to check if field is set: {e}"))
                })?;

                if !is_set {
                    // Field is not set, check if it has a default attribute
                    if field.flags.contains(facet_core::FieldFlags::DEFAULT)
                        || container_shape.has_default_attr()
                    {
                        trace!("Field #{} has default attribute and is not set", index);

                        // Enter the field
                        let mut field_wip = self.wip.field(index).map_err(|e| {
                            DeserializeError::Custom(format!("Failed to access field: {e}"))
                        })?;

                        // Try to apply a default value
                        if field.flags.contains(facet_core::FieldFlags::DEFAULT)
                            && field.vtable.default_fn.is_some()
                        {
                            // Use custom default function if available
                            let default_fn = field.vtable.default_fn.unwrap();
                            trace!("Using custom default function for field #{}", index);
                            field_wip = field_wip.put_from_fn(default_fn).map_err(|e| {
                                DeserializeError::Custom(format!("Failed to apply default: {e}"))
                            })?;
                        } else {
                            // Otherwise use the standard default implementation
                            if !field.shape().is(facet_core::Characteristic::Default) {
                                return Err(DeserializeError::Custom(format!(
                                    "Field has default attribute but no default implementation: {:?}",
                                    field.shape()
                                )));
                            }

                            trace!("Using default impl for field #{}", index);
                            field_wip = field_wip.put_default().map_err(|e| {
                                DeserializeError::Custom(format!("Failed to apply default: {e}"))
                            })?;
                        }

                        // Return to parent
                        self.wip = field_wip.pop().map_err(|e| {
                            DeserializeError::Custom(format!("Failed to return to parent: {e}"))
                        })?;
                    }
                }
            }
        }

        Ok(self)
    }

    fn handle_value(mut self) -> DeserializeResult<Self, D::Error> {
        trace!("Handling value");

        // Check for optional types first
        let shape = self.wip.shape();

        if let facet_core::Def::Option(_) = shape.def {
            if self
                .deserializer
                .is_none()
                .map_err(DeserializeError::Format)?
            {
                trace!("Optional value is none");
                // It's None, just put the default value (None)
                self.wip = self.wip.put_default()?;
                return Ok(self);
            } else {
                trace!("Optional value is Some, entering Some variant");
                // It's Some, continue with the inner value
                self.wip = self.wip.push_some()?;
                self.stack.push(DeserializeOp::Pop);
            }
        }

        // Get the shape again as it might have changed for Option types
        let shape = self.wip.innermost_shape();

        match shape.def {
            facet_core::Def::Scalar(_) => {
                // Handle primitive types
                if shape.is_type::<bool>() {
                    let value = self
                        .deserializer
                        .deserialize_bool()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<char>() {
                    let value = self
                        .deserializer
                        .deserialize_char()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<&str>() || shape.is_type::<std::string::String>() {
                    let value = self
                        .deserializer
                        .deserialize_string()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<u8>() {
                    let value = self
                        .deserializer
                        .deserialize_u8()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<u16>() {
                    let value = self
                        .deserializer
                        .deserialize_u16()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<u32>() {
                    let value = self
                        .deserializer
                        .deserialize_u32()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<u64>() {
                    let value = self
                        .deserializer
                        .deserialize_u64()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<u128>() {
                    let value = self
                        .deserializer
                        .deserialize_u128()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<usize>() {
                    let value = self
                        .deserializer
                        .deserialize_usize()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<i8>() {
                    let value = self
                        .deserializer
                        .deserialize_i8()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<i16>() {
                    let value = self
                        .deserializer
                        .deserialize_i16()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<i32>() {
                    let value = self
                        .deserializer
                        .deserialize_i32()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<i64>() {
                    let value = self
                        .deserializer
                        .deserialize_i64()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<i128>() {
                    let value = self
                        .deserializer
                        .deserialize_i128()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<isize>() {
                    let value = self
                        .deserializer
                        .deserialize_isize()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<f32>() {
                    let value = self
                        .deserializer
                        .deserialize_f32()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<f64>() {
                    let value = self
                        .deserializer
                        .deserialize_f64()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put(value)?;
                } else if shape.is_type::<()>() {
                    self.deserializer
                        .deserialize_unit()
                        .map_err(DeserializeError::Format)?;
                    self.wip = self.wip.put_default()?;
                } else {
                    return Err(DeserializeError::Custom(format!(
                        "Unsupported scalar type: {shape}",
                    )));
                }
            }
            facet_core::Def::Struct(sd) if sd.kind == StructKind::Unit => {
                self.deserializer
                    .deserialize_unit()
                    .map_err(DeserializeError::Format)?;
                self.wip = self.wip.put_default()?;
            }
            facet_core::Def::Struct(sd)
                if sd.kind == StructKind::Tuple || sd.kind == StructKind::TupleStruct =>
            {
                // Prepare to read array elements
                let _size = self
                    .deserializer
                    .start_array()
                    .map_err(DeserializeError::Format)?;

                self.stack.push(DeserializeOp::End(EndContext::Array));
                self.stack.push(DeserializeOp::ArrayItem);
                // Enter the first field
                self.wip = self.wip.begin_pushback()?;
            }
            facet_core::Def::Struct(sd) if sd.kind == StructKind::Struct => {
                // Prepare to read object fields
                let _size = self
                    .deserializer
                    .start_object()
                    .map_err(DeserializeError::Format)?;

                self.stack.push(DeserializeOp::End(EndContext::Object));
                self.stack.push(DeserializeOp::ObjectKey);
            }
            facet_core::Def::Struct(otherwise) => {
                return Err(DeserializeError::Custom(format!(
                    "Unsupported struct kind: {:?}",
                    otherwise.kind
                )));
            }
            facet_core::Def::Enum(_) => {
                let variant_name = self
                    .deserializer
                    .get_variant()
                    .map_err(DeserializeError::Format)?;

                // Select the variant
                self.wip = self.wip.variant_named(&variant_name)?;
                let def_idx = self
                    .wip
                    .shape()
                    .def
                    .into_enum()
                    .unwrap()
                    .variants
                    .iter()
                    .position(|v| v.name == variant_name)
                    .unwrap();
                let def = self.wip.shape().def.into_enum().unwrap().variants[def_idx].data;

                match def {
                    facet_core::StructDef {
                        kind: StructKind::Unit,
                        ..
                    } => {
                        // Unit variant, represented as a string. We have entered the right variant
                        // above and so there is nothing else to do.
                        trace!("Unit variant");
                    }
                    facet_core::StructDef {
                        kind: StructKind::Tuple | StructKind::TupleStruct,
                        fields,
                        ..
                    } if fields.len() == 1 => {
                        trace!("Tuple variant with one field, treating as inner field");

                        // Enter said inner field
                        self.wip = self.wip.field(0)?;

                        self.stack.push(DeserializeOp::Pop);
                        self.stack.push(DeserializeOp::Value);
                    }
                    facet_core::StructDef {
                        kind: StructKind::Tuple | StructKind::TupleStruct,
                        ..
                    } => {
                        trace!("Tuple variant with multiple fields");

                        // Tuple variant
                        let _size = self
                            .deserializer
                            .start_array()
                            .map_err(DeserializeError::Format)?;

                        self.wip = self.wip.begin_pushback()?;
                        self.stack.push(DeserializeOp::End(EndContext::Array));
                        self.stack.push(DeserializeOp::ArrayItem);
                    }
                    facet_core::StructDef {
                        kind: StructKind::Struct,
                        ..
                    } => {
                        // Struct variant
                        let _size = self
                            .deserializer
                            .start_object()
                            .map_err(DeserializeError::Format)?;

                        self.stack.push(DeserializeOp::End(EndContext::Object));
                        self.stack.push(DeserializeOp::ObjectKey);
                    }
                    otherwise => panic!("Unexpected def: {otherwise:?}"),
                };
            }
            facet_core::Def::List(_) | facet_core::Def::Array(_) | facet_core::Def::Slice(_) => {
                // Start by creating an empty list/array
                self.wip = self.wip.put_default()?;

                let _size = self
                    .deserializer
                    .start_array()
                    .map_err(DeserializeError::Format)?;

                // TODO preallocate `_size` in the Wip

                self.stack.push(DeserializeOp::End(EndContext::Array));
                self.stack.push(DeserializeOp::ArrayItem);

                // Initialize pushback state for array items
                self.wip = self.wip.begin_pushback()?;
            }
            facet_core::Def::Map(_) => {
                // Create empty map
                self.wip = self.wip.put_default()?;

                let _size = self
                    .deserializer
                    .start_map()
                    .map_err(DeserializeError::Format)?;

                // TODO same thing, reserve space in the map

                self.stack.push(DeserializeOp::End(EndContext::Map));

                // Handle key-value pairs one by one
                if self
                    .deserializer
                    .has_next()
                    .map_err(DeserializeError::Format)?
                {
                    // Push key first, then we'll handle value
                    self.wip = self.wip.push_map_key()?;
                    self.stack.push(DeserializeOp::Value);
                }
            }
            _ => {
                return Err(DeserializeError::Custom(format!(
                    "Unsupported type: {shape}"
                )));
            }
        }

        Ok(self)
    }

    fn handle_object_key(mut self) -> DeserializeResult<Self, D::Error> {
        trace!("Handling object key");
        if !self
            .deserializer
            .has_next()
            .map_err(DeserializeError::Format)?
        {
            return Ok(self);
        }

        let field_name = match self
            .deserializer
            .next_field_name()
            .map_err(DeserializeError::Format)?
        {
            Some(name) => name,
            None => return Ok(self),
        };
        trace!("Object key is `{field_name}`");

        // Check if this field exists in our struct
        if let Some(field_index) = self.wip.field_index(&field_name) {
            // Push next operations to handle the field value
            self.stack.push(DeserializeOp::ObjectComma);
            self.stack.push(DeserializeOp::Pop);
            self.stack.push(DeserializeOp::Value);

            // We'll process the next value and then return to the struct
            self.wip = self.wip.field(field_index)?;
        } else {
            if self.wip.shape().has_deny_unknown_fields_attr() {
                return Err(DeserializeError::UnknownField {
                    field_name,
                    shape: self.wip.shape(),
                });
            }
            // Field not found, skip this value
            self.stack.push(DeserializeOp::ObjectComma);
            self.stack.push(DeserializeOp::SkipValue);
        }

        Ok(self)
    }

    fn handle_object_comma(mut self) -> DeserializeResult<Self, D::Error> {
        trace!("Handling object comma");

        if self
            .deserializer
            .has_next()
            .map_err(DeserializeError::Format)?
        {
            self.stack.push(DeserializeOp::ObjectKey);
        }

        Ok(self)
    }

    fn handle_array_item(mut self) -> DeserializeResult<Self, D::Error> {
        trace!("Handling array item");
        if !self
            .deserializer
            .has_next()
            .map_err(DeserializeError::Format)?
        {
            return Ok(self);
        }

        // Push a new item to the array/list
        self.wip = self.wip.push()?;

        // Set up to process the item's value
        self.stack.push(DeserializeOp::ArrayComma);
        self.stack.push(DeserializeOp::Pop);
        self.stack.push(DeserializeOp::Value);

        Ok(self)
    }

    fn handle_array_comma(mut self) -> DeserializeResult<Self, D::Error> {
        trace!("Handling array comma");

        // Check if there are more items
        if self
            .deserializer
            .has_next()
            .map_err(DeserializeError::Format)?
        {
            self.stack.push(DeserializeOp::ArrayItem);
        }

        Ok(self)
    }
}

// --- Helper Trait for Ergonomics ---

// /// Extension trait to simplify calling the generic deserializer.
// pub trait Deserialize<'a>: facet_core::Facet<'a> {
//     /// Deserialize a value of this type using the provided `Deserializer`.
//     fn deserialize<D: Deserializer>(deserializer: &mut D) -> DeserializeResult<Self, D::Error>;
// }

// impl<'a, T> Deserialize<'a> for T
// where
//     T: facet_core::Facet<'a>,
// {
//     /// Deserialize a value of this type using the provided `Deserializer`.
//     fn deserialize<D: Deserializer>(deserializer: &mut D) -> DeserializeResult<Self, D::Error> {
//         let wip = facet_reflect::Wip::alloc::<T>()
//             .map_err(|e| DeserializeError::Custom(format!("Failed to allocate: {e}")))?;
//         Ok(deserialize_iterative(wip, deserializer)
//             .unwrap()
//             .materialize()
//             .unwrap())
//     }
// }
