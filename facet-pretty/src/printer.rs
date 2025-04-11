//! Pretty printer implementation for Facet types

use std::{
    collections::{HashMap, VecDeque},
    fmt::{self, Write},
    hash::{DefaultHasher, Hash, Hasher},
    str,
};

use facet::{Facet, Peek};

use crate::{ansi, color::ColorGenerator};

/// A formatter for pretty-printing Facet types
pub struct PrettyPrinter {
    indent_size: usize,
    max_depth: Option<usize>,
    color_generator: ColorGenerator,
    use_colors: bool,
    list_u8_as_bytes: bool,
}

impl Default for PrettyPrinter {
    fn default() -> Self {
        Self {
            indent_size: 2,
            max_depth: None,
            color_generator: ColorGenerator::default(),
            use_colors: true,
            list_u8_as_bytes: true,
        }
    }
}

/// Stack state for iterative formatting
enum StackState {
    Start,
    ProcessStructField { field_index: usize },
    ProcessListItem { item_index: usize },
    ProcessBytesItem { item_index: usize },
    ProcessMapEntry,
    Finish,
    OptionFinish,
}

/// Stack item for iterative traversal
struct StackItem<'a> {
    peek: Peek<'a>,
    format_depth: usize,
    type_depth: usize,
    state: StackState,
}

impl PrettyPrinter {
    /// Create a new PrettyPrinter with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the indentation size
    pub fn with_indent_size(mut self, size: usize) -> Self {
        self.indent_size = size;
        self
    }

    /// Set the maximum depth for recursive printing
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Set the color generator
    pub fn with_color_generator(mut self, generator: ColorGenerator) -> Self {
        self.color_generator = generator;
        self
    }

    /// Enable or disable colors
    pub fn with_colors(mut self, use_colors: bool) -> Self {
        self.use_colors = use_colors;
        self
    }

    /// Format a value to a string
    pub fn format<T: Facet>(&self, value: &T) -> String {
        let peek = Peek::new(value);

        let mut output = String::new();
        self.format_peek_internal(peek, &mut output, 0, 0, &mut HashMap::new())
            .expect("Formatting failed");

        output
    }

    /// Format a value to a formatter
    pub fn format_to<T: Facet>(&self, value: &T, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let peek = Peek::new(value);
        self.format_peek_internal(peek, f, 0, 0, &mut HashMap::new())
    }

    /// Format a Peek value to a string
    pub fn format_peek(&self, peek: Peek<'_>) -> String {
        let mut output = String::new();
        self.format_peek_internal(peek, &mut output, 0, 0, &mut HashMap::new())
            .expect("Formatting failed");
        output
    }

    /// Internal method to format a Peek value
    pub(crate) fn format_peek_internal(
        &self,
        peek: Peek<'_>,
        f: &mut impl Write,
        format_depth: usize,
        type_depth: usize,
        visited: &mut HashMap<*const (), usize>,
    ) -> fmt::Result {
        // Create a queue for our stack items
        let mut stack = VecDeque::new();

        // Push the initial item
        stack.push_back(StackItem {
            peek,
            format_depth,
            type_depth,
            state: StackState::Start,
        });

        // Process items until the stack is empty
        while let Some(mut item) = stack.pop_back() {
            match item.state {
                StackState::Start => {
                    // Check if we've reached the maximum depth
                    if let Some(max_depth) = self.max_depth {
                        if item.format_depth > max_depth {
                            self.write_punctuation(f, "[")?;
                            write!(f, "...")?;
                            continue;
                        }
                    }

                    // Get the data pointer for cycle detection
                    let ptr = unsafe { item.peek.data().as_ptr() };

                    // Check for cycles - if we've seen this pointer before at a different type_depth
                    if let Some(&ptr_type_depth) = visited.get(&ptr) {
                        // If the current type_depth is significantly deeper than when we first saw this pointer,
                        // we have a true cycle, not just a transparent wrapper
                        if item.type_depth > ptr_type_depth + 1 {
                            self.write_type_name(f, &item.peek)?;
                            self.write_punctuation(f, " { ")?;
                            self.write_comment(
                                f,
                                &format!(
                                    "/* cycle detected at {:p} (first seen at type_depth {}) */",
                                    ptr, ptr_type_depth
                                ),
                            )?;
                            self.write_punctuation(f, " }")?;
                            continue;
                        }
                    } else {
                        // First time seeing this pointer, record its type_depth
                        visited.insert(ptr, item.type_depth);
                    }

                    // Process based on the peek variant
                    match item.peek {
                        Peek::Value(value) => {
                            self.format_value(value, f)?;
                        }
                        Peek::Option(option) => {
                            // Print the Option name
                            self.write_type_name(f, &option)?;

                            if option.is_some() {
                                self.write_punctuation(f, "::Some(")?;

                                if let Some(inner_value) = option.value() {
                                    // Create a custom stack item for Option::Some value
                                    let start_item = StackItem {
                                        peek: inner_value,
                                        format_depth: item.format_depth,
                                        type_depth: item.type_depth + 1,
                                        state: StackState::Start,
                                    };

                                    // Add a special close parenthesis item
                                    let close_paren_item = StackItem {
                                        peek: Peek::Option(option),
                                        format_depth: item.format_depth,
                                        type_depth: item.type_depth,
                                        state: StackState::OptionFinish,
                                    };

                                    // Process the value first, then handle closing
                                    stack.push_back(close_paren_item);
                                    stack.push_back(start_item);
                                }

                                // Skip to next item
                                continue;
                            } else {
                                self.write_punctuation(f, "::None")?;
                            }
                        }
                        Peek::Struct(struct_) => {
                            // When recursing into a struct, always increment format_depth
                            // Only increment type_depth if we're moving to a different address
                            let new_type_depth =
                                if core::ptr::eq(unsafe { struct_.data().as_ptr() }, ptr) {
                                    item.type_depth // Same pointer, don't increment type_depth
                                } else {
                                    item.type_depth + 1 // Different pointer, increment type_depth
                                };

                            // Get struct doc comments from the shape
                            let doc_comments = struct_.shape().doc;
                            if !doc_comments.is_empty() {
                                for line in doc_comments {
                                    self.write_comment(f, &format!("///{}", line))?;
                                    writeln!(f)?;
                                }
                            }

                            // Print the struct name
                            self.write_type_name(f, &struct_)?;
                            self.write_punctuation(f, " {")?;

                            if struct_.field_count() == 0 {
                                self.write_punctuation(f, "}")?;
                                continue;
                            }

                            writeln!(f)?;

                            // Push back the item with the next state to continue processing fields
                            item.state = StackState::ProcessStructField { field_index: 0 };
                            item.format_depth += 1;
                            item.type_depth = new_type_depth;
                            stack.push_back(item);
                        }
                        Peek::List(list) => {
                            // When recursing into a list, always increment format_depth
                            // Only increment type_depth if we're moving to a different address
                            let new_type_depth =
                                if core::ptr::eq(unsafe { list.data().as_ptr() }, ptr) {
                                    item.type_depth // Same pointer, don't increment type_depth
                                } else {
                                    item.type_depth + 1 // Different pointer, increment type_depth
                                };

                            // Print the list name
                            self.write_type_name(f, &list)?;

                            if list.def().t.is_type::<u8>() && self.list_u8_as_bytes {
                                // Push back the item with the next state to continue processing list items
                                item.state = StackState::ProcessBytesItem { item_index: 0 };
                                writeln!(f)?;
                                write!(f, " ")?;

                                // TODO: write all the bytes here instead?
                            } else {
                                // Push back the item with the next state to continue processing list items
                                item.state = StackState::ProcessListItem { item_index: 0 };
                                self.write_punctuation(f, " [")?;
                                writeln!(f)?;
                            }

                            item.format_depth += 1;
                            item.type_depth = new_type_depth;
                            stack.push_back(item);
                        }
                        Peek::Map(map) => {
                            // Print the map name
                            self.write_type_name(f, &map)?;
                            self.write_punctuation(f, " {")?;
                            writeln!(f)?;

                            // Push back the item with the next state to continue processing map
                            item.state = StackState::ProcessMapEntry;
                            item.format_depth += 1;
                            // When recursing into a map, always increment format_depth
                            // Only increment type_depth if we're moving to a different address
                            item.type_depth = if core::ptr::eq(unsafe { map.data().as_ptr() }, ptr)
                            {
                                item.type_depth // Same pointer, don't increment type_depth
                            } else {
                                item.type_depth + 1 // Different pointer, increment type_depth
                            };
                            stack.push_back(item);
                        }
                        Peek::Enum(enum_) => {
                            // When recursing into an enum, increment format_depth
                            // Only increment type_depth if we're moving to a different address
                            let _new_type_depth =
                                if core::ptr::eq(unsafe { enum_.data().as_ptr() }, ptr) {
                                    item.type_depth // Same pointer, don't increment type_depth
                                } else {
                                    item.type_depth + 1 // Different pointer, increment type_depth
                                };

                            // Get the active variant
                            let variant = enum_.active_variant();

                            // Get enum and variant doc comments
                            let doc_comments = enum_.shape().doc;

                            // Display doc comments before the type name
                            for line in doc_comments {
                                self.write_comment(f, &format!("///{}", line))?;
                                writeln!(f)?;
                            }

                            // Show variant docs
                            for line in variant.doc {
                                self.write_comment(f, &format!("///{}", line))?;
                                writeln!(f)?;
                            }

                            // Print the enum name and separator
                            self.write_type_name(f, &enum_)?;
                            self.write_punctuation(f, "::")?;

                            // Variant docs are already handled above

                            // Get the active variant name
                            let variant_name = enum_.variant_name_active();

                            // Apply color for variant name
                            if self.use_colors {
                                ansi::write_bold(f)?;
                                write!(f, "{}", variant_name)?;
                                ansi::write_reset(f)?;
                            } else {
                                write!(f, "{}", variant_name)?;
                            }

                            // Process the variant fields based on the variant kind
                            match enum_.variant_kind_active() {
                                facet::VariantKind::Unit => {
                                    // Unit variant has no fields, nothing more to print
                                }
                                facet::VariantKind::Tuple { .. } => {
                                    // Tuple variant, print the fields like a tuple
                                    self.write_punctuation(f, "(")?;

                                    // Check if there are any fields to print
                                    let has_fields = enum_.fields().count() > 0;

                                    if !has_fields {
                                        self.write_punctuation(f, ")")?;
                                        continue;
                                    }

                                    writeln!(f)?;

                                    // Push back item to process fields
                                    item.state = StackState::ProcessStructField { field_index: 0 };
                                    item.format_depth += 1;
                                    stack.push_back(item);
                                }
                                facet::VariantKind::Struct { .. } => {
                                    // Struct variant, print the fields like a struct
                                    self.write_punctuation(f, " {")?;

                                    // Check if there are any fields to print
                                    let has_fields = enum_.fields().count() > 0;

                                    if !has_fields {
                                        self.write_punctuation(f, " }")?;
                                        continue;
                                    }

                                    writeln!(f)?;

                                    // Push back item to process fields
                                    item.state = StackState::ProcessStructField { field_index: 0 };
                                    item.format_depth += 1;
                                    stack.push_back(item);
                                }
                                _ => {
                                    // Other variant kinds that might be added in the future
                                    write!(f, " /* unsupported variant kind */")?;
                                }
                            }
                        }
                        _ => {
                            write!(f, "unsupported peek variant: {:?}", item.peek)?;
                        }
                    }
                }
                StackState::ProcessStructField { field_index } => {
                    // Handle both struct and enum fields
                    if let Peek::Struct(struct_) = item.peek {
                        let fields: Vec<_> = struct_.fields_with_metadata().collect();

                        if field_index >= fields.len() {
                            // All fields processed, write closing brace
                            write!(
                                f,
                                "{:width$}{}",
                                "",
                                self.style_punctuation("}"),
                                width = (item.format_depth - 1) * self.indent_size
                            )?;
                            continue;
                        }

                        let (_, field_name, field_value, field) = &fields[field_index];

                        // Field doc comment
                        if !field.doc.is_empty() {
                            // Only add new line if not the first field
                            if field_index > 0 {
                                writeln!(f)?;
                            }
                            // Hard-code consistent indentation for doc comments
                            for line in field.doc {
                                // Use exactly the same indentation as fields (2 spaces)
                                write!(
                                    f,
                                    "{:width$}",
                                    "",
                                    width = item.format_depth * self.indent_size
                                )?;
                                self.write_comment(f, &format!("///{}", line))?;
                                writeln!(f)?;
                            }
                        }

                        // Field name
                        write!(
                            f,
                            "{:width$}",
                            "",
                            width = item.format_depth * self.indent_size
                        )?;
                        self.write_field_name(f, field_name)?;
                        self.write_punctuation(f, ": ")?;

                        // Check if field is sensitive
                        if field.flags.contains(facet::FieldFlags::SENSITIVE) {
                            // Field value is sensitive, use write_redacted
                            self.write_redacted(f, "[REDACTED]")?;
                            self.write_punctuation(f, ",")?;
                            writeln!(f)?;

                            item.state = StackState::ProcessStructField {
                                field_index: field_index + 1,
                            };
                            stack.push_back(item);
                        } else {
                            // Field value is not sensitive, format normally
                            // Push back current item to continue after formatting field value
                            item.state = StackState::ProcessStructField {
                                field_index: field_index + 1,
                            };

                            let finish_item = StackItem {
                                peek: *field_value,
                                format_depth: item.format_depth,
                                type_depth: item.type_depth + 1,
                                state: StackState::Finish,
                            };
                            let start_item = StackItem {
                                peek: *field_value,
                                format_depth: item.format_depth,
                                type_depth: item.type_depth + 1,
                                state: StackState::Start,
                            };

                            stack.push_back(item);
                            stack.push_back(finish_item);
                            stack.push_back(start_item);
                        }
                    } else if let Peek::Enum(enum_val) = item.peek {
                        // Since PeekEnum implements Copy, we can use it directly

                        // Get all fields with their metadata
                        let fields: Vec<_> = enum_val.fields_with_metadata().collect();

                        // Check if we're done processing fields
                        if field_index >= fields.len() {
                            // Determine variant kind to use the right closing delimiter
                            match enum_val.variant_kind_active() {
                                facet::VariantKind::Tuple { .. } => {
                                    // Close tuple variant with )
                                    write!(
                                        f,
                                        "{:width$}{}",
                                        "",
                                        self.style_punctuation(")"),
                                        width = (item.format_depth - 1) * self.indent_size
                                    )?;
                                }
                                facet::VariantKind::Struct { .. } => {
                                    // Close struct variant with }
                                    write!(
                                        f,
                                        "{:width$}{}",
                                        "",
                                        self.style_punctuation("}"),
                                        width = (item.format_depth - 1) * self.indent_size
                                    )?;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        // Get the current field with metadata
                        let (_, field_name, field_peek, field) = fields[field_index];

                        // Define consistent indentation
                        let field_indent = "  "; // Use 2 spaces for all fields

                        // Add field doc comments if available
                        if !field.doc.is_empty() {
                            // Only add new line if not the first field
                            if field_index > 0 {
                                writeln!(f)?;
                            }
                            for line in field.doc {
                                // Hard-code consistent indentation (2 spaces)
                                write!(f, "  ")?;
                                self.write_comment(f, &format!("///{}", line))?;
                                writeln!(f)?;
                            }
                            // Rewrite indentation after doc comments
                            write!(f, "{}", field_indent)?;
                        }

                        // For struct variants, print field name
                        if let facet::VariantKind::Struct { .. } = enum_val.variant_kind_active() {
                            self.write_field_name(f, field_name)?;
                            self.write_punctuation(f, ": ")?;
                        }

                        // Set up to process the next field after this one
                        item.state = StackState::ProcessStructField {
                            field_index: field_index + 1,
                        };

                        // Create finish and start items for processing the field value
                        let finish_item = StackItem {
                            peek: field_peek, // field_peek is already a Peek which is Copy
                            format_depth: item.format_depth,
                            type_depth: item.type_depth + 1,
                            state: StackState::Finish,
                        };
                        let start_item = StackItem {
                            peek: field_peek, // field_peek is already a Peek which is Copy
                            format_depth: item.format_depth,
                            type_depth: item.type_depth + 1,
                            state: StackState::Start,
                        };

                        // Push items to stack in the right order
                        stack.push_back(item);
                        stack.push_back(finish_item);
                        stack.push_back(start_item);
                    }
                }
                StackState::ProcessListItem { item_index } => {
                    if let Peek::List(list) = item.peek {
                        if item_index >= list.len() {
                            // All items processed, write closing bracket
                            write!(
                                f,
                                "{:width$}",
                                "",
                                width = (item.format_depth - 1) * self.indent_size
                            )?;
                            self.write_punctuation(f, "]")?;
                            continue;
                        }

                        // Indent
                        write!(
                            f,
                            "{:width$}",
                            "",
                            width = item.format_depth * self.indent_size
                        )?;

                        // Push back current item to continue after formatting list item
                        item.state = StackState::ProcessListItem {
                            item_index: item_index + 1,
                        };
                        let next_format_depth = item.format_depth;
                        let next_type_depth = item.type_depth + 1;
                        stack.push_back(item);

                        // Push list item to format first
                        let list_item = list.iter().nth(item_index).unwrap();
                        stack.push_back(StackItem {
                            peek: list_item,
                            format_depth: next_format_depth,
                            type_depth: next_type_depth,
                            state: StackState::Finish,
                        });

                        // When we push a list item to format, we need to process it from the beginning
                        stack.push_back(StackItem {
                            peek: list_item,
                            format_depth: next_format_depth,
                            type_depth: next_type_depth,
                            state: StackState::Start, // Use Start state to properly process the item
                        });
                    }
                }
                StackState::ProcessBytesItem { item_index } => {
                    if let Peek::List(list) = item.peek {
                        if item_index >= list.len() {
                            // All items processed, write closing bracket
                            write!(
                                f,
                                "{:width$}",
                                "",
                                width = (item.format_depth - 1) * self.indent_size
                            )?;
                            continue;
                        }

                        // On the first byte, write the opening byte sequence indicator
                        if item_index == 0 {
                            write!(f, " ")?;
                        }

                        // Only display 16 bytes per line
                        if item_index > 0 && item_index % 16 == 0 {
                            writeln!(f)?;
                            write!(
                                f,
                                "{:width$}",
                                "",
                                width = item.format_depth * self.indent_size
                            )?;
                        } else if item_index > 0 {
                            write!(f, " ")?;
                        }

                        // Get the byte
                        if let Some(Peek::Value(value)) = list.iter().nth(item_index) {
                            let byte = unsafe { value.data().read::<u8>() };

                            // Generate a color for this byte based on its value
                            let mut hasher = DefaultHasher::new();
                            byte.hash(&mut hasher);
                            let hash = hasher.finish();
                            let color = self.color_generator.generate_color(hash);

                            // Apply color if needed
                            if self.use_colors {
                                color.write_fg(f)?;
                            }

                            // Display the byte in hex format
                            write!(f, "{:02x}", byte)?;

                            // Reset color if needed
                            if self.use_colors {
                                ansi::write_reset(f)?;
                            }
                        } else {
                            unreachable!()
                        }

                        // Push back current item to continue after formatting byte
                        item.state = StackState::ProcessBytesItem {
                            item_index: item_index + 1,
                        };
                        stack.push_back(item);
                    }
                }
                StackState::ProcessMapEntry => {
                    if let Peek::Map(_) = item.peek {
                        // TODO: Implement proper map iteration when available in facet

                        // Indent
                        write!(
                            f,
                            "{:width$}",
                            "",
                            width = item.format_depth * self.indent_size
                        )?;
                        write!(f, "{}", self.style_comment("/* Map contents */"))?;
                        writeln!(f)?;

                        // Closing brace with proper indentation
                        write!(
                            f,
                            "{:width$}{}",
                            "",
                            self.style_punctuation("}"),
                            width = (item.format_depth - 1) * self.indent_size
                        )?;
                    }
                }
                StackState::Finish => {
                    // Add comma and newline for struct fields and list items
                    self.write_punctuation(f, ",")?;
                    writeln!(f)?;
                }
                StackState::OptionFinish => {
                    // Just close the Option::Some parenthesis, with no comma
                    self.write_punctuation(f, ")")?;
                }
            }
        }

        Ok(())
    }

    /// Format a scalar value
    fn format_value(&self, value: facet::PeekValue, f: &mut impl Write) -> fmt::Result {
        // Generate a color for this shape
        let mut hasher = DefaultHasher::new();
        value.shape().def.hash(&mut hasher);
        let hash = hasher.finish();
        let color = self.color_generator.generate_color(hash);

        // Apply color if needed
        if self.use_colors {
            color.write_fg(f)?;
        }

        // Display the value
        struct DisplayWrapper<'a>(&'a facet::PeekValue<'a>);

        impl fmt::Display for DisplayWrapper<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if self.0.display(f).is_none() {
                    // If the value doesn't implement Display, use Debug
                    if self.0.debug(f).is_none() {
                        // If the value doesn't implement Debug either, just show the type name
                        self.0.type_name(f, facet::TypeNameOpts::infinite())?;
                        write!(f, "(⋯)")?;
                    }
                }
                Ok(())
            }
        }

        write!(f, "{}", DisplayWrapper(&value))?;

        // Reset color if needed
        if self.use_colors {
            ansi::write_reset(f)?;
        }

        Ok(())
    }

    /// Write styled type name to formatter
    fn write_type_name<W: fmt::Write>(&self, f: &mut W, peek: &facet::PeekValue) -> fmt::Result {
        struct TypeNameWriter<'a, 'b: 'a>(&'b facet::PeekValue<'a>);

        impl core::fmt::Display for TypeNameWriter<'_, '_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                self.0.type_name(f, facet::TypeNameOpts::infinite())
            }
        }
        let type_name = TypeNameWriter(peek);

        if self.use_colors {
            ansi::write_bold(f)?;
            write!(f, "{}", type_name)?;
            ansi::write_reset(f)
        } else {
            write!(f, "{}", type_name)
        }
    }

    /// Style a type name and return it as a string
    #[allow(dead_code)]
    fn style_type_name(&self, peek: &facet::PeekValue) -> String {
        let mut result = String::new();
        self.write_type_name(&mut result, peek).unwrap();
        result
    }

    /// Write styled field name to formatter
    fn write_field_name<W: fmt::Write>(&self, f: &mut W, name: &str) -> fmt::Result {
        if self.use_colors {
            ansi::write_rgb(f, 114, 160, 193)?;
            write!(f, "{}", name)?;
            ansi::write_reset(f)
        } else {
            write!(f, "{}", name)
        }
    }

    /// Write styled punctuation to formatter
    fn write_punctuation<W: fmt::Write>(&self, f: &mut W, text: &str) -> fmt::Result {
        if self.use_colors {
            ansi::write_dim(f)?;
            write!(f, "{}", text)?;
            ansi::write_reset(f)
        } else {
            write!(f, "{}", text)
        }
    }

    /// Style punctuation and return it as a string
    fn style_punctuation(&self, text: &str) -> String {
        let mut result = String::new();
        self.write_punctuation(&mut result, text).unwrap();
        result
    }

    /// Write styled comment to formatter
    fn write_comment<W: fmt::Write>(&self, f: &mut W, text: &str) -> fmt::Result {
        if self.use_colors {
            ansi::write_dim(f)?;
            write!(f, "{}", text)?;
            ansi::write_reset(f)
        } else {
            write!(f, "{}", text)
        }
    }

    /// Style a comment and return it as a string
    fn style_comment(&self, text: &str) -> String {
        let mut result = String::new();
        self.write_comment(&mut result, text).unwrap();
        result
    }

    /// Write styled redacted value to formatter
    fn write_redacted<W: fmt::Write>(&self, f: &mut W, text: &str) -> fmt::Result {
        if self.use_colors {
            ansi::write_rgb(f, 224, 49, 49)?; // Use bright red for redacted values
            ansi::write_bold(f)?;
            write!(f, "{}", text)?;
            ansi::write_reset(f)
        } else {
            write!(f, "{}", text)
        }
    }

    /// Style a redacted value and return it as a string
    #[allow(dead_code)]
    fn style_redacted(&self, text: &str) -> String {
        let mut result = String::new();
        self.write_redacted(&mut result, text).unwrap();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic tests for the PrettyPrinter
    #[test]
    fn test_pretty_printer_default() {
        let printer = PrettyPrinter::default();
        assert_eq!(printer.indent_size, 2);
        assert_eq!(printer.max_depth, None);
        assert!(printer.use_colors);
    }

    #[test]
    fn test_pretty_printer_with_methods() {
        let printer = PrettyPrinter::new()
            .with_indent_size(4)
            .with_max_depth(3)
            .with_colors(false);

        assert_eq!(printer.indent_size, 4);
        assert_eq!(printer.max_depth, Some(3));
        assert!(!printer.use_colors);
    }
}
