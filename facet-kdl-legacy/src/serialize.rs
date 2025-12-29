//! KDL serialization implementation.

use std::io::Write;

use facet_core::{Facet, Field};
use facet_reflect::{HasFields, Peek, is_spanned_shape};

use crate::deserialize::{KdlChildrenFieldExt, KdlFieldExt};
use crate::error::{KdlError, KdlErrorKind};

pub(crate) type Result<T> = std::result::Result<T, KdlError>;

/// Serialize a value of type `T` to a KDL string.
///
/// The type `T` must be a struct where all fields are marked with either
/// `#[facet(kdl::child)]` or `#[facet(kdl::children)]` (the "document" pattern).
///
/// # Example
/// ```
/// # use facet::Facet;
/// # use facet_kdl_legacy as kdl;
/// # use facet_kdl_legacy::to_string;
/// #[derive(Facet)]
/// struct Config {
///     #[facet(kdl::child)]
///     server: Server,
/// }
///
/// #[derive(Facet)]
/// struct Server {
///     #[facet(kdl::argument)]
///     host: String,
///     #[facet(kdl::property)]
///     port: u16,
/// }
///
/// # fn main() -> Result<(), facet_kdl_legacy::KdlError> {
/// let config = Config {
///     server: Server { host: "localhost".into(), port: 8080 },
/// };
/// let kdl = to_string(&config)?;
/// assert_eq!(kdl, "server \"localhost\" port=8080\n");
/// # Ok(())
/// # }
/// ```
pub fn to_string<T: Facet<'static>>(value: &T) -> Result<String> {
    let mut output = Vec::new();
    to_writer(&mut output, value)?;
    Ok(String::from_utf8(output).expect("KDL output should be valid UTF-8"))
}

/// Serialize a value of type `T` to a writer as KDL.
///
/// This is the streaming version of [`to_string`] - it writes directly to any
/// type implementing [`std::io::Write`], which is useful for writing to files,
/// network streams, or other I/O destinations without buffering the entire
/// output in memory first.
///
/// The type `T` must be a struct where all fields are marked with either
/// `#[facet(kdl::child)]` or `#[facet(kdl::children)]` (the "document" pattern).
///
/// # Example
///
/// Writing to a file:
/// ```no_run
/// # use facet::Facet;
/// # use facet_kdl_legacy as kdl;
/// # use facet_kdl_legacy::to_writer;
/// # use std::fs::File;
/// #[derive(Facet)]
/// struct Config {
///     #[facet(kdl::child)]
///     server: Server,
/// }
///
/// #[derive(Facet)]
/// struct Server {
///     #[facet(kdl::argument)]
///     host: String,
///     #[facet(kdl::property)]
///     port: u16,
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = Config {
///     server: Server { host: "localhost".into(), port: 8080 },
/// };
///
/// let mut file = File::create("config.kdl")?;
/// to_writer(&mut file, &config)?;
/// # Ok(())
/// # }
/// ```
///
/// Writing to a `Vec<u8>` buffer:
/// ```
/// # use facet::Facet;
/// # use facet_kdl_legacy as kdl;
/// # use facet_kdl_legacy::to_writer;
/// #[derive(Facet)]
/// struct Config {
///     #[facet(kdl::child)]
///     server: Server,
/// }
///
/// #[derive(Facet)]
/// struct Server {
///     #[facet(kdl::argument)]
///     host: String,
///     #[facet(kdl::property)]
///     port: u16,
/// }
///
/// # fn main() -> Result<(), facet_kdl_legacy::KdlError> {
/// let config = Config {
///     server: Server { host: "localhost".into(), port: 8080 },
/// };
///
/// let mut buffer = Vec::new();
/// to_writer(&mut buffer, &config)?;
/// let kdl = String::from_utf8(buffer).unwrap();
/// assert_eq!(kdl, "server \"localhost\" port=8080\n");
/// # Ok(())
/// # }
/// ```
pub fn to_writer<W: Write, T: Facet<'static>>(writer: &mut W, value: &T) -> Result<()> {
    let peek = Peek::new(value);
    let mut serializer = KdlSerializer::new(writer);
    serializer.serialize_document(peek)
}

struct KdlSerializer<W> {
    writer: W,
    indent: usize,
}

impl<W: Write> KdlSerializer<W> {
    fn new(writer: W) -> Self {
        Self { writer, indent: 0 }
    }

    fn write_indent(&mut self) -> Result<()> {
        for _ in 0..self.indent {
            write!(self.writer, "    ").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
        }
        Ok(())
    }

    fn serialize_document<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        let struct_peek = peek
            .into_struct()
            .map_err(|_| KdlErrorKind::SerializeNotStruct)?;

        for (field, field_peek) in struct_peek.fields() {
            if field.is_kdl_child() {
                self.serialize_child_field(&field, field_peek)?;
            } else if field.has_attr(Some("kdl"), "children") {
                self.serialize_children_field(&field, field_peek)?;
            }
        }

        Ok(())
    }

    fn serialize_child_field<'mem, 'facet>(
        &mut self,
        field: &Field,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        // Handle Option<T> - skip if None
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            // Unwrap the Some value
            if let Some(inner) = opt_peek.value() {
                return self.serialize_child_field(field, inner);
            }
            return Ok(());
        }

        // For enum child fields, use variant name as node name
        if let Ok(enum_peek) = peek.into_enum() {
            let variant_name = enum_peek
                .variant_name_active()
                .map_err(|_| KdlErrorKind::SerializeUnknownNodeType)?;
            self.write_indent()?;
            write!(self.writer, "{}", escape_node_name(variant_name))
                .map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            self.serialize_enum_variant_contents(enum_peek)?;
            writeln!(self.writer).map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        self.serialize_node(field.name, peek)
    }

    fn serialize_children_field<'mem, 'facet>(
        &mut self,
        field: &Field,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        let list_peek = peek
            .into_list()
            .map_err(|_| KdlErrorKind::SerializeNotList)?;

        // Check if the field has a custom node name override
        let custom_node_name = field.kdl_children_node_name();

        for item_peek in list_peek.iter() {
            if let Some(node_name) = custom_node_name {
                // Use the field-level custom node name
                self.serialize_node_with_name(node_name, item_peek)?;
            } else {
                // Fall back to inferring the node name from the value
                self.serialize_node_from_value(item_peek)?;
            }
        }

        Ok(())
    }

    /// Serialize a node with an explicit node name (used for custom node name overrides)
    fn serialize_node_with_name<'mem, 'facet>(
        &mut self,
        node_name: &str,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        self.write_indent()?;
        write!(self.writer, "{}", escape_node_name(node_name))
            .map_err(|e| KdlErrorKind::Io(e.to_string()))?;

        self.serialize_node_contents(peek)?;

        writeln!(self.writer).map_err(|e| KdlErrorKind::Io(e.to_string()))?;
        Ok(())
    }

    fn serialize_node<'mem, 'facet>(
        &mut self,
        node_name: &str,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        self.write_indent()?;
        write!(self.writer, "{}", escape_node_name(node_name))
            .map_err(|e| KdlErrorKind::Io(e.to_string()))?;

        self.serialize_node_contents(peek)?;

        writeln!(self.writer).map_err(|e| KdlErrorKind::Io(e.to_string()))?;
        Ok(())
    }

    fn serialize_node_from_value<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        // For items in a children list, we need to determine the node name
        // Check if it's an enum (node name = variant name) or struct with node_name field

        if let Ok(enum_peek) = peek.into_enum() {
            let variant_name = enum_peek
                .variant_name_active()
                .map_err(|_| KdlErrorKind::SerializeUnknownNodeType)?;
            self.write_indent()?;
            write!(self.writer, "{}", escape_node_name(variant_name))
                .map_err(|e| KdlErrorKind::Io(e.to_string()))?;

            // Serialize the variant's fields as node contents using HasFields
            self.serialize_enum_variant_contents(enum_peek)?;

            writeln!(self.writer).map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        // Get type identifier before converting to PeekStruct
        let type_name = Some(peek.shape().type_identifier);

        if let Ok(struct_peek) = peek.into_struct() {
            // Check for node_name field first, then fall back to type name
            let node_name = self.find_node_name_with_fallback(&struct_peek, type_name)?;

            self.write_indent()?;
            write!(self.writer, "{}", escape_node_name(&node_name))
                .map_err(|e| KdlErrorKind::Io(e.to_string()))?;

            self.serialize_struct_contents(struct_peek)?;

            writeln!(self.writer).map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        Err(KdlErrorKind::SerializeUnknownNodeType.into())
    }

    fn serialize_node_contents<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        // Check if this is an enum
        if let Ok(enum_peek) = peek.into_enum() {
            return self.serialize_enum_variant_contents(enum_peek);
        }

        // Otherwise treat as struct
        if let Ok(struct_peek) = peek.into_struct() {
            return self.serialize_struct_contents(struct_peek);
        }

        Ok(())
    }

    fn serialize_enum_variant_contents<'mem, 'facet>(
        &mut self,
        enum_peek: facet_reflect::PeekEnum<'mem, 'facet>,
    ) -> Result<()> {
        let mut has_children = false;
        let mut children_to_serialize: Vec<(Field, Peek<'mem, 'facet>)> = Vec::new();

        // First pass: serialize arguments and properties inline
        for (field, field_peek) in enum_peek.fields() {
            if field.has_attr(Some("kdl"), "node_name") {
                // Skip node_name field - it's used for the node name itself
                continue;
            }

            if field.has_attr(Some("kdl"), "argument") {
                self.serialize_argument(field_peek)?;
            } else if field.has_attr(Some("kdl"), "arguments") {
                self.serialize_arguments(field_peek)?;
            } else if field.has_attr(Some("kdl"), "property") {
                self.serialize_property(field.name, field_peek)?;
            } else if field.is_kdl_child() || field.has_attr(Some("kdl"), "children") {
                has_children = true;
                children_to_serialize.push((field, field_peek));
            } else if field.is_flattened() {
                // Flattened fields in enum variants: serialize their contents inline
                self.serialize_flattened_field(
                    field_peek,
                    &mut has_children,
                    &mut children_to_serialize,
                )?;
            }
        }

        // Second pass: serialize child nodes in a block
        if has_children {
            writeln!(self.writer, " {{").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            self.indent += 1;

            for (field, field_peek) in children_to_serialize {
                if field.is_kdl_child() {
                    self.serialize_child_field(&field, field_peek)?;
                } else {
                    self.serialize_children_field(&field, field_peek)?;
                }
            }

            self.indent -= 1;
            self.write_indent()?;
            write!(self.writer, "}}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
        }

        Ok(())
    }

    fn serialize_struct_contents<'mem, 'facet>(
        &mut self,
        struct_peek: facet_reflect::PeekStruct<'mem, 'facet>,
    ) -> Result<()> {
        let mut has_children = false;
        let mut children_to_serialize: Vec<(Field, Peek<'mem, 'facet>)> = Vec::new();

        // First pass: serialize arguments and properties inline
        for (field, field_peek) in struct_peek.fields() {
            if field.has_attr(Some("kdl"), "node_name") {
                // Skip node_name field - it's used for the node name itself
                continue;
            }

            if field.has_attr(Some("kdl"), "argument") {
                self.serialize_argument(field_peek)?;
            } else if field.has_attr(Some("kdl"), "arguments") {
                self.serialize_arguments(field_peek)?;
            } else if field.has_attr(Some("kdl"), "property") {
                self.serialize_property(field.name, field_peek)?;
            } else if field.is_kdl_child() || field.has_attr(Some("kdl"), "children") {
                has_children = true;
                children_to_serialize.push((field, field_peek));
            } else if field.is_flattened() {
                // Flattened fields: serialize their contents inline (not as a nested node)
                self.serialize_flattened_field(
                    field_peek,
                    &mut has_children,
                    &mut children_to_serialize,
                )?;
            }
        }

        // Second pass: serialize child nodes in a block
        if has_children {
            writeln!(self.writer, " {{").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            self.indent += 1;

            for (field, field_peek) in children_to_serialize {
                if field.is_kdl_child() {
                    self.serialize_child_field(&field, field_peek)?;
                } else {
                    self.serialize_children_field(&field, field_peek)?;
                }
            }

            self.indent -= 1;
            self.write_indent()?;
            write!(self.writer, "}}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
        }

        Ok(())
    }

    /// Serialize a flattened field's contents inline.
    /// This handles both structs and enums - for enums, it serializes the active variant's fields.
    fn serialize_flattened_field<'mem, 'facet>(
        &mut self,
        peek: Peek<'mem, 'facet>,
        has_children: &mut bool,
        children_to_serialize: &mut Vec<(Field, Peek<'mem, 'facet>)>,
    ) -> Result<()> {
        // Handle Option<T> - skip if None, unwrap if Some
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_flattened_field(inner, has_children, children_to_serialize);
            }
            return Ok(());
        }

        // Handle enum - serialize the active variant's fields
        if let Ok(enum_peek) = peek.into_enum() {
            // For tuple variants with a single struct (e.g., Local(LocalSource)),
            // we need to serialize the inner struct's fields, not the tuple field.
            let fields: Vec<_> = enum_peek.fields().collect();
            if fields.len() == 1 {
                let (field, field_peek) = &fields[0];
                // Check if this is a tuple field (name is a number like "0")
                if field.name.parse::<usize>().is_ok() {
                    // Recurse into the inner type
                    return self.serialize_flattened_field(
                        *field_peek,
                        has_children,
                        children_to_serialize,
                    );
                }
            }
            // Normal struct-like variant fields
            for (field, field_peek) in fields {
                self.serialize_flattened_inner_field(
                    &field,
                    field_peek,
                    has_children,
                    children_to_serialize,
                )?;
            }
            return Ok(());
        }

        // Handle struct - serialize all fields
        if let Ok(struct_peek) = peek.into_struct() {
            for (field, field_peek) in struct_peek.fields() {
                self.serialize_flattened_inner_field(
                    &field,
                    field_peek,
                    has_children,
                    children_to_serialize,
                )?;
            }
            return Ok(());
        }

        Ok(())
    }

    /// Serialize a single field from inside a flattened struct/enum.
    fn serialize_flattened_inner_field<'mem, 'facet>(
        &mut self,
        field: &Field,
        field_peek: Peek<'mem, 'facet>,
        has_children: &mut bool,
        children_to_serialize: &mut Vec<(Field, Peek<'mem, 'facet>)>,
    ) -> Result<()> {
        if field.has_attr(Some("kdl"), "argument") {
            self.serialize_argument(field_peek)?;
        } else if field.has_attr(Some("kdl"), "arguments") {
            self.serialize_arguments(field_peek)?;
        } else if field.has_attr(Some("kdl"), "property") {
            self.serialize_property(field.name, field_peek)?;
        } else if field.is_kdl_child() || field.has_attr(Some("kdl"), "children") {
            *has_children = true;
            children_to_serialize.push((*field, field_peek));
        } else if field.is_flattened() {
            // Nested flatten - recurse
            self.serialize_flattened_field(field_peek, has_children, children_to_serialize)?;
        }
        Ok(())
    }

    fn serialize_argument<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        write!(self.writer, " ").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
        self.serialize_value(peek)
    }

    fn serialize_arguments<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        let list_peek = peek
            .into_list()
            .map_err(|_| KdlErrorKind::SerializeNotList)?;

        for item_peek in list_peek.iter() {
            write!(self.writer, " ").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            self.serialize_value(item_peek)?;
        }

        Ok(())
    }

    fn serialize_property<'mem, 'facet>(
        &mut self,
        name: &str,
        peek: Peek<'mem, 'facet>,
    ) -> Result<()> {
        // Handle Option<T> - skip if None
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                write!(self.writer, " {}=", escape_node_name(name))
                    .map_err(|e| KdlErrorKind::Io(e.to_string()))?;
                return self.serialize_value(inner);
            }
            return Ok(());
        }

        write!(self.writer, " {}=", escape_node_name(name))
            .map_err(|e| KdlErrorKind::Io(e.to_string()))?;
        self.serialize_value(peek)
    }

    fn serialize_value<'mem, 'facet>(&mut self, peek: Peek<'mem, 'facet>) -> Result<()> {
        // Handle Option<T>
        if let Ok(opt_peek) = peek.into_option() {
            if opt_peek.is_none() {
                write!(self.writer, "#null").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
                return Ok(());
            }
            if let Some(inner) = opt_peek.value() {
                return self.serialize_value(inner);
            }
            return Ok(());
        }

        // Handle Spanned<T> - unwrap to the inner value
        if is_spanned_shape(peek.shape())
            && let Ok(struct_peek) = peek.into_struct()
            && let Ok(value_field) = struct_peek.field_by_name("value")
        {
            return self.serialize_value(value_field);
        }

        // Unwrap transparent wrappers to get the inner value
        let peek = peek.innermost_peek();

        // Try string first
        if let Some(s) = peek.as_str() {
            write!(self.writer, "{}", escape_string(s))
                .map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        // Try various numeric types
        if let Ok(v) = peek.get::<bool>() {
            write!(self.writer, "#{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        if let Ok(v) = peek.get::<i8>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i16>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i32>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<i64>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        if let Ok(v) = peek.get::<u8>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u16>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u32>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<u64>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        if let Ok(v) = peek.get::<f32>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }
        if let Ok(v) = peek.get::<f64>() {
            write!(self.writer, "{v}").map_err(|e| KdlErrorKind::Io(e.to_string()))?;
            return Ok(());
        }

        Err(KdlErrorKind::SerializeUnknownValueType.into())
    }

    fn find_node_name_with_fallback<'mem, 'facet>(
        &self,
        struct_peek: &facet_reflect::PeekStruct<'mem, 'facet>,
        type_name: Option<&'static str>,
    ) -> Result<String> {
        for (field, field_peek) in struct_peek.fields() {
            if field.has_attr(Some("kdl"), "node_name") {
                // Try direct string first
                if let Some(s) = field_peek.as_str() {
                    return Ok(s.to_string());
                }
                // Handle Spanned<String> - extract the value field
                if is_spanned_shape(field_peek.shape())
                    && let Ok(spanned_struct) = field_peek.into_struct()
                    && let Ok(value_peek) = spanned_struct.field_by_name("value")
                    && let Some(s) = value_peek.as_str()
                {
                    return Ok(s.to_string());
                }
            }
        }
        // Fallback to type name (lowercase) if available, otherwise "node"
        Ok(type_name
            .map(to_lowercase_first)
            .unwrap_or_else(|| "node".to_string()))
    }
}

fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn escape_node_name(name: &str) -> &str {
    // For now, assume valid KDL identifiers. Could add quoting later if needed.
    name
}

/// Convert PascalCase to lowercase (e.g., "Step" -> "step", "MyType" -> "myType")
fn to_lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().chain(chars).collect(),
    }
}

/// Convert kebab-case to PascalCase (e.g., "http-source" -> "HttpSource", "git" -> "Git")
pub(crate) fn kebab_to_pascal(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}
