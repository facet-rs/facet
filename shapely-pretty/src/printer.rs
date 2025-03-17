//! Pretty printer implementation for Shapely types

use std::{
    collections::HashSet,
    fmt::{self, Write},
    hash::{DefaultHasher, Hash, Hasher},
    str,
};

use shapely_core::{Innards, Scalar, ScalarContents, Shape, ShapeDesc, Shapely};

use crate::{
    ansi,
    color::{self, ColorGenerator},
};

/// A formatter for pretty-printing Shapely types
pub struct PrettyPrinter {
    indent_size: usize,
    max_depth: Option<usize>,
    color_generator: ColorGenerator,
    use_colors: bool,
}

impl Default for PrettyPrinter {
    fn default() -> Self {
        Self {
            indent_size: 2,
            max_depth: None,
            color_generator: ColorGenerator::default(),
            use_colors: true,
        }
    }
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

    /// Pretty-print a value that implements Shapely
    pub fn print<T: Shapely>(&self, value: &T) {
        let shape_desc = T::shape_desc();
        let ptr = value as *const T as *mut u8;

        let mut output = String::new();
        self.format_value(ptr, shape_desc, &mut output, 0, &mut HashSet::new())
            .expect("Formatting failed");

        print!("{}", output);
    }

    /// Format a value to a string
    pub fn format<T: Shapely>(&self, value: &T) -> String {
        let shape_desc = T::shape_desc();
        let ptr = value as *const T as *mut u8;

        let mut output = String::new();
        self.format_value(ptr, shape_desc, &mut output, 0, &mut HashSet::new())
            .expect("Formatting failed");

        output
    }

    /// Format a value to a formatter
    pub fn format_to<T: Shapely>(&self, value: &T, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shape_desc = T::shape_desc();
        let ptr = value as *const T as *mut u8;

        self.format_value(ptr, shape_desc, f, 0, &mut HashSet::new())
    }

    /// Internal method to format a value at a specific memory address
    pub(crate) fn format_value(
        &self,
        ptr: *mut u8,
        shape_desc: ShapeDesc,
        f: &mut impl Write,
        depth: usize,
        visited: &mut HashSet<*mut u8>,
    ) -> fmt::Result {
        // Check if we've reached the maximum depth
        if let Some(max_depth) = self.max_depth {
            if depth > max_depth {
                self.write_punctuation(f, "[")?;
                write!(f, "...")?;
                return Ok(());
            }
        }

        // Get the shape
        let shape = shape_desc.get();

        // Generate a color for this shape
        let mut hasher = DefaultHasher::new();
        shape.typeid.hash(&mut hasher);
        let hash = hasher.finish();
        let color = self.color_generator.generate_color(hash);

        // Format based on the shape's innards
        match &shape.innards {
            Innards::Scalar(scalar) => self.format_scalar(ptr, *scalar, f, color),
            Innards::Struct { fields } => self.format_struct(ptr, shape, fields, f, depth, visited),
            Innards::HashMap { value_shape } => {
                self.format_hashmap(ptr, shape, *value_shape, f, depth, visited)
            }
            Innards::Array(elem_shape) => {
                self.format_array(ptr, shape, *elem_shape, f, depth, visited)
            }
            Innards::Transparent(inner_shape) => {
                self.format_transparent(ptr, shape, *inner_shape, f, depth, visited)
            }
            Innards::Enum { variants, repr: _ } => {
                self.format_enum(ptr, shape, variants, f, depth, visited)
            }
        }
    }

    /// Format a scalar value
    fn format_scalar(
        &self,
        ptr: *mut u8,
        scalar: Scalar,
        f: &mut impl Write,
        color: color::RGB,
    ) -> fmt::Result {
        // Use Scalar::get_contents for safe access to the scalar value
        let contents = unsafe { scalar.get_contents(ptr) };

        // Apply color if needed
        if self.use_colors {
            color.write_fg(f)?;
        }

        // Format the content
        match contents {
            ScalarContents::String(s) => {
                write!(f, "\"")?;
                for c in s.escape_debug() {
                    write!(f, "{}", c)?;
                }
                write!(f, "\"")?;
            }
            ScalarContents::Bytes(b) => {
                write!(f, "b\"")?;
                for &byte in b.iter().take(64) {
                    write!(f, "\\x{:02x}", byte)?;
                }
                if b.len() > 64 {
                    write!(f, "...")?;
                }
                write!(f, "\"")?;
            }
            ScalarContents::I8(v) => write!(f, "{}", v)?,
            ScalarContents::I16(v) => write!(f, "{}", v)?,
            ScalarContents::I32(v) => write!(f, "{}", v)?,
            ScalarContents::I64(v) => write!(f, "{}", v)?,
            ScalarContents::I128(v) => write!(f, "{}", v)?,
            ScalarContents::U8(v) => write!(f, "{}", v)?,
            ScalarContents::U16(v) => write!(f, "{}", v)?,
            ScalarContents::U32(v) => write!(f, "{}", v)?,
            ScalarContents::U64(v) => write!(f, "{}", v)?,
            ScalarContents::U128(v) => write!(f, "{}", v)?,
            ScalarContents::F32(v) => write!(f, "{}", v)?,
            ScalarContents::F64(v) => write!(f, "{}", v)?,
            ScalarContents::Boolean(v) => write!(f, "{}", v)?,
            ScalarContents::Nothing => write!(f, "()")?,
            ScalarContents::Unknown => write!(f, "<unknown scalar>")?,
            // Handle future variants that might be added to the non-exhaustive enum
            _ => write!(f, "<unknown scalar type>")?,
        }

        // Reset color if needed
        if self.use_colors {
            ansi::write_reset(f)?;
        }

        Ok(())
    }

    /// Format a struct
    fn format_struct(
        &self,
        ptr: *mut u8,
        shape: Shape,
        fields: &'static [shapely_core::Field],
        f: &mut impl Write,
        depth: usize,
        visited: &mut HashSet<*mut u8>,
    ) -> fmt::Result {
        // Check for cycles
        if !visited.insert(ptr) {
            self.write_type_name(f, &shape.to_string())?;
            self.write_punctuation(f, " { ")?;
            self.write_comment(f, "/* cycle detected */")?;
            self.write_punctuation(f, " }")?;
            return Ok(());
        }

        // Print the struct name
        self.write_type_name(f, &shape.to_string())?;
        self.write_punctuation(f, " {")?;

        if fields.is_empty() {
            self.write_punctuation(f, " }")?;
            visited.remove(&ptr);
            return Ok(());
        }

        writeln!(f)?;

        // Print each field
        for field in fields {
            // Indent
            write!(f, "{:width$}", "", width = (depth + 1) * self.indent_size)?;

            // Field name
            write!(f, "{}: ", self.style_field_name(field.name))?;

            // Check if field is sensitive
            if field.flags.is_sensitive() {
                // For sensitive fields, display [REDACTED] instead of the actual value
                write!(f, "{}", self.style_redacted("[REDACTED]"))?;
            } else {
                // Field value - compute the field address
                let field_ptr = unsafe { ptr.add(field.offset) };
                self.format_value(field_ptr, field.shape, f, depth + 1, visited)?;
            }

            writeln!(f, "{}", self.style_punctuation(","))?;
        }

        // Closing brace with proper indentation
        write!(
            f,
            "{:width$}{}",
            "",
            self.style_punctuation("}"),
            width = depth * self.indent_size
        )?;

        // Remove from visited set when we're done with this struct
        visited.remove(&ptr);

        Ok(())
    }

    /// Format a HashMap
    fn format_hashmap(
        &self,
        _ptr: *mut u8,
        shape: Shape,
        _value_shape: ShapeDesc,
        f: &mut impl Write,
        depth: usize,
        _visited: &mut HashSet<*mut u8>,
    ) -> fmt::Result {
        // In a real implementation, we would need to iterate over the HashMap
        // For now, we'll just print a placeholder

        write!(f, "{}", self.style_type_name(&shape.to_string()))?;
        write!(f, "{}", self.style_punctuation(" {"))?;
        writeln!(f)?;

        // Indent
        write!(f, "{:width$}", "", width = (depth + 1) * self.indent_size)?;
        write!(f, "{}", self.style_comment("/* HashMap contents */"))?;
        writeln!(f)?;

        // Closing brace with proper indentation
        write!(
            f,
            "{:width$}{}",
            "",
            self.style_punctuation("}"),
            width = depth * self.indent_size
        )
    }

    /// Format an array
    fn format_array(
        &self,
        _ptr: *mut u8,
        shape: Shape,
        _elem_shape: ShapeDesc,
        f: &mut impl Write,
        depth: usize,
        _visited: &mut HashSet<*mut u8>,
    ) -> fmt::Result {
        // In a real implementation, we would need to iterate over the array
        // For now, we'll just print a placeholder

        write!(f, "{}", self.style_type_name(&shape.to_string()))?;
        write!(f, "{}", self.style_punctuation(" ["))?;
        writeln!(f)?;

        // Indent
        write!(f, "{:width$}", "", width = (depth + 1) * self.indent_size)?;
        write!(f, "{}", self.style_comment("/* Array contents */"))?;
        writeln!(f)?;

        // Closing bracket with proper indentation
        write!(
            f,
            "{:width$}{}",
            "",
            self.style_punctuation("]"),
            width = depth * self.indent_size
        )
    }

    /// Format a transparent wrapper
    fn format_transparent(
        &self,
        ptr: *mut u8,
        shape: Shape,
        inner_shape: ShapeDesc,
        f: &mut impl Write,
        depth: usize,
        visited: &mut HashSet<*mut u8>,
    ) -> fmt::Result {
        // Print the wrapper type name
        write!(f, "{}", self.style_type_name(&shape.to_string()))?;
        write!(f, "{}", self.style_punctuation("("))?;

        // Format the inner value
        self.format_value(ptr, inner_shape, f, depth, visited)?;

        // Closing parenthesis
        write!(f, "{}", self.style_punctuation(")"))
    }

    /// Formats an enum value
    fn format_enum(
        &self,
        _ptr: *mut u8,
        shape: Shape,
        _variants: &'static [shapely_core::Variant],
        f: &mut impl Write,
        depth: usize,
        _visited: &mut HashSet<*mut u8>,
    ) -> fmt::Result {
        // Basic enum rendering for now - just show the type name with {} placeholder
        // since we don't have runtime variant/field access implemented yet
        self.write_type_name(f, &format!("{}", shape))?;
        writeln!(f, " {{")?;
        if let Some(max_depth) = self.max_depth {
            if depth >= max_depth {
                writeln!(
                    f,
                    "{}{}",
                    " ".repeat(self.indent_size),
                    self.style_comment("// Enum contents omitted due to depth limit")
                )?;
                writeln!(f, "}}")?;
                return Ok(());
            }
        }
        writeln!(
            f,
            "{}{}",
            " ".repeat(self.indent_size),
            self.style_comment(
                format!(
                    "// Enum with {} variants (variant access not yet implemented)",
                    _variants.len()
                )
                .as_str()
            )
        )?;
        writeln!(f, "}}")?;
        Ok(())
    }

    /// Write styled type name to formatter
    fn write_type_name<W: fmt::Write>(&self, f: &mut W, name: &str) -> fmt::Result {
        if self.use_colors {
            ansi::write_bold(f)?;
            write!(f, "{}", name)?;
            ansi::write_reset(f)
        } else {
            write!(f, "{}", name)
        }
    }

    /// Style a type name and return it as a string
    fn style_type_name(&self, name: &str) -> String {
        let mut result = String::new();
        self.write_type_name(&mut result, name).unwrap();
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

    /// Style a field name and return it as a string
    fn style_field_name(&self, name: &str) -> String {
        let mut result = String::new();
        self.write_field_name(&mut result, name).unwrap();
        result
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

    #[test]
    fn test_style_methods() {
        let printer_with_colors = PrettyPrinter::new().with_colors(true);
        let printer_without_colors = PrettyPrinter::new().with_colors(false);

        // With colors
        assert_eq!(
            printer_with_colors.style_type_name("Test"),
            format!("{}Test{}", ansi::BOLD, ansi::RESET)
        );

        // Without colors
        assert_eq!(printer_without_colors.style_type_name("Test"), "Test");
    }
}
