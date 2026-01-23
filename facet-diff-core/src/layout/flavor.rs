//! Diff output flavors (Rust, JSON, XML).
//!
//! Each flavor knows how to present struct fields and format values
//! according to its format's conventions.

use std::borrow::Cow;
use std::fmt::Write;

use facet_core::{Def, Field, PrimitiveType, Type};
use facet_reflect::Peek;

/// How a field should be presented in the diff output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldPresentation {
    /// Show as an inline attribute/field on the struct line.
    /// - Rust: `x: 10`
    /// - JSON: `"x": 10`
    /// - XML: `x="10"` (as attribute on opening tag)
    Attribute {
        /// The field name (possibly renamed)
        name: Cow<'static, str>,
    },

    /// Show as a nested child element.
    /// - XML: `<title>...</title>` as child element
    /// - Rust/JSON: same as Attribute (nested structs are inline)
    Child {
        /// The element/field name
        name: Cow<'static, str>,
    },

    /// Show as text content inside the parent.
    /// - XML: `<p>this text</p>`
    /// - Rust/JSON: same as Attribute
    TextContent,

    /// Show as multiple child elements (for sequences).
    /// - XML: `<Item/><Item/>` as siblings
    /// - Rust/JSON: same as Attribute (sequences are `[...]`)
    Children {
        /// The name for each item element
        item_name: Cow<'static, str>,
    },
}

/// A diff output flavor that knows how to format values and present fields.
pub trait DiffFlavor {
    /// Format a scalar/leaf value into a writer.
    ///
    /// The output should NOT include surrounding quotes for strings -
    /// the renderer will add appropriate syntax based on context.
    fn format_value(&self, peek: Peek<'_, '_>, w: &mut dyn Write) -> std::fmt::Result;

    /// Determine how a field should be presented.
    fn field_presentation(&self, field: &Field) -> FieldPresentation;

    /// Opening syntax for a struct/object.
    /// - Rust: `Point {`
    /// - JSON: `{`
    /// - XML: `<Point`
    fn struct_open(&self, name: &str) -> Cow<'static, str>;

    /// Closing syntax for a struct/object.
    /// - Rust: `}`
    /// - JSON: `}`
    /// - XML: `/>` (self-closing) or `</Point>`
    fn struct_close(&self, name: &str, self_closing: bool) -> Cow<'static, str>;

    /// Separator between fields.
    /// - Rust: `, `
    /// - JSON: `, `
    /// - XML: ` ` (space between attributes)
    fn field_separator(&self) -> &'static str;

    /// Trailing comma/separator (no trailing space).
    /// Used at end of lines when fields are broken across lines.
    /// - Rust: `,`
    /// - JSON: `,`
    /// - XML: `` (empty - no trailing separator)
    fn trailing_separator(&self) -> &'static str {
        ","
    }

    /// Opening syntax for a sequence/array.
    /// - Rust: `[`
    /// - JSON: `[`
    /// - XML: (wrapper element, handled differently)
    fn seq_open(&self) -> Cow<'static, str>;

    /// Closing syntax for a sequence/array.
    /// - Rust: `]`
    /// - JSON: `]`
    /// - XML: (wrapper element, handled differently)
    fn seq_close(&self) -> Cow<'static, str>;

    /// Separator between sequence items.
    /// - Rust: `, `
    /// - JSON: `,`
    /// - XML: (newlines/whitespace)
    fn item_separator(&self) -> &'static str;

    /// Format a sequence item value, optionally wrapping in element tags.
    /// - Rust: `0` (no wrapping)
    /// - JSON: `0` (no wrapping)
    /// - XML: `<i32>0</i32>` (wrapped in element)
    fn format_seq_item<'a>(&self, _item_type: &str, value: &'a str) -> Cow<'a, str> {
        // Default: no wrapping, just return the value
        Cow::Borrowed(value)
    }

    /// Opening for a sequence that is a struct field value.
    /// - Rust: `fieldname: [`
    /// - JSON: `"fieldname": [`
    /// - XML: `<fieldname>` (wrapper element, not attribute!)
    fn format_seq_field_open(&self, field_name: &str) -> String {
        // Default: use field prefix + seq_open
        format!(
            "{}{}",
            self.format_field_prefix(field_name),
            self.seq_open()
        )
    }

    /// Closing for a sequence that is a struct field value.
    /// - Rust: `]`
    /// - JSON: `]`
    /// - XML: `</fieldname>`
    fn format_seq_field_close(&self, _field_name: &str) -> Cow<'static, str> {
        // Default: just seq_close
        self.seq_close()
    }

    /// Format a comment (for collapsed items).
    /// - Rust: `/* ...5 more */`
    /// - JSON: `// ...5 more`
    /// - XML: `<!-- ...5 more -->`
    fn comment(&self, text: &str) -> String;

    /// Format a field assignment (name and value).
    /// - Rust: `name: value`
    /// - JSON: `"name": value`
    /// - XML: `name="value"`
    fn format_field(&self, name: &str, value: &str) -> String;

    /// Format just the field name with assignment operator.
    /// - Rust: `name: `
    /// - JSON: `"name": `
    /// - XML: `name="`
    fn format_field_prefix(&self, name: &str) -> String;

    /// Suffix after the value (if any).
    /// - Rust: `` (empty)
    /// - JSON: `` (empty)
    /// - XML: `"` (closing quote)
    fn format_field_suffix(&self) -> &'static str;

    /// Close the opening tag when there are children.
    /// - Rust: `` (empty - no separate closing for opening tag)
    /// - JSON: `` (empty)
    /// - XML: `>` (close the opening tag before children)
    fn struct_open_close(&self) -> &'static str {
        ""
    }

    /// Optional type name comment to show after struct_open.
    /// Rendered in muted color for readability.
    /// - Rust: None (type name is in struct_open)
    /// - JSON: Some("/* Point */")
    /// - XML: None
    fn type_comment(&self, _name: &str) -> Option<String> {
        None
    }

    /// Opening wrapper for a child element (nested struct field).
    /// - Rust: `field_name: ` (field prefix)
    /// - JSON: `"field_name": ` (field prefix)
    /// - XML: `` (empty - no wrapper, or could be `<field_name>\n`)
    fn format_child_open(&self, name: &str) -> Cow<'static, str> {
        // Default: use field prefix (works for Rust/JSON)
        Cow::Owned(self.format_field_prefix(name))
    }

    /// Closing wrapper for a child element (nested struct field).
    /// - Rust: `` (empty)
    /// - JSON: `` (empty)
    /// - XML: `` (empty, or `</field_name>` if wrapping)
    fn format_child_close(&self, _name: &str) -> Cow<'static, str> {
        Cow::Borrowed("")
    }
}

/// Rust-style output flavor.
///
/// Produces output like: `Point { x: 10, y: 20 }`
#[derive(Debug, Clone, Default)]
pub struct RustFlavor;

impl DiffFlavor for RustFlavor {
    fn format_value(&self, peek: Peek<'_, '_>, w: &mut dyn Write) -> std::fmt::Result {
        format_value_quoted(peek, w)
    }

    fn field_presentation(&self, field: &Field) -> FieldPresentation {
        // Rust flavor: all fields are attributes (key: value)
        FieldPresentation::Attribute {
            name: Cow::Borrowed(field.name),
        }
    }

    fn struct_open(&self, name: &str) -> Cow<'static, str> {
        Cow::Owned(format!("{} {{", name))
    }

    fn struct_close(&self, _name: &str, _self_closing: bool) -> Cow<'static, str> {
        Cow::Borrowed("}")
    }

    fn field_separator(&self) -> &'static str {
        ", "
    }

    fn seq_open(&self) -> Cow<'static, str> {
        Cow::Borrowed("[")
    }

    fn seq_close(&self) -> Cow<'static, str> {
        Cow::Borrowed("]")
    }

    fn item_separator(&self) -> &'static str {
        ", "
    }

    fn comment(&self, text: &str) -> String {
        format!("/* {} */", text)
    }

    fn format_field(&self, name: &str, value: &str) -> String {
        format!("{}: {}", name, value)
    }

    fn format_field_prefix(&self, name: &str) -> String {
        format!("{}: ", name)
    }

    fn format_field_suffix(&self) -> &'static str {
        ""
    }
}

/// JSON-style output flavor (JSONC with comments for type names).
///
/// Produces output like: `{ // Point\n  "x": 10, "y": 20\n}`
#[derive(Debug, Clone, Default)]
pub struct JsonFlavor;

impl DiffFlavor for JsonFlavor {
    fn format_value(&self, peek: Peek<'_, '_>, w: &mut dyn Write) -> std::fmt::Result {
        format_value_quoted(peek, w)
    }

    fn field_presentation(&self, field: &Field) -> FieldPresentation {
        // JSON flavor: all fields are attributes ("key": value)
        FieldPresentation::Attribute {
            name: Cow::Borrowed(field.name),
        }
    }

    fn struct_open(&self, _name: &str) -> Cow<'static, str> {
        Cow::Borrowed("{")
    }

    fn type_comment(&self, name: &str) -> Option<String> {
        Some(format!("/* {} */", name))
    }

    fn struct_close(&self, _name: &str, _self_closing: bool) -> Cow<'static, str> {
        Cow::Borrowed("}")
    }

    fn field_separator(&self) -> &'static str {
        ", "
    }

    fn seq_open(&self) -> Cow<'static, str> {
        Cow::Borrowed("[")
    }

    fn seq_close(&self) -> Cow<'static, str> {
        Cow::Borrowed("]")
    }

    fn item_separator(&self) -> &'static str {
        ", "
    }

    fn comment(&self, text: &str) -> String {
        format!("// {}", text)
    }

    fn format_field(&self, name: &str, value: &str) -> String {
        format!("\"{}\": {}", name, value)
    }

    fn format_field_prefix(&self, name: &str) -> String {
        format!("\"{}\": ", name)
    }

    fn format_field_suffix(&self) -> &'static str {
        ""
    }
}

/// XML-style output flavor.
///
/// Produces output like: `<Point x="10" y="20"/>`
///
/// Respects `#[facet(xml::attribute)]`, `#[facet(xml::element)]`, etc.
#[derive(Debug, Clone, Default)]
pub struct XmlFlavor;

impl DiffFlavor for XmlFlavor {
    fn format_value(&self, peek: Peek<'_, '_>, w: &mut dyn Write) -> std::fmt::Result {
        format_value_raw(peek, w)
    }

    fn field_presentation(&self, field: &Field) -> FieldPresentation {
        // Check for XML-specific attributes
        //
        // NOTE: We detect XML attributes by namespace string "xml" (e.g., `field.has_attr(Some("xml"), "attribute")`).
        // This works because the namespace is defined in the `define_attr_grammar!` macro in facet-xml
        // with `ns "xml"`, NOT by the import alias. So even if someone writes `use facet_xml as html;`
        // and uses `#[facet(html::attribute)]`, the namespace stored in the attribute is still "xml".
        // This should be tested to confirm, but not now.
        if field.has_attr(Some("xml"), "attribute") {
            FieldPresentation::Attribute {
                name: Cow::Borrowed(field.name),
            }
        } else if field.has_attr(Some("xml"), "elements") {
            FieldPresentation::Children {
                item_name: Cow::Borrowed(field.name),
            }
        } else if field.has_attr(Some("xml"), "text") {
            FieldPresentation::TextContent
        } else if field.has_attr(Some("xml"), "element") {
            FieldPresentation::Child {
                name: Cow::Borrowed(field.name),
            }
        } else {
            // Default: treat as child element (XML's default for non-attributed fields)
            // In XML, fields without explicit annotation typically become child elements
            FieldPresentation::Child {
                name: Cow::Borrowed(field.name),
            }
        }
    }

    fn struct_open(&self, name: &str) -> Cow<'static, str> {
        Cow::Owned(format!("<{}", name))
    }

    fn struct_close(&self, name: &str, self_closing: bool) -> Cow<'static, str> {
        if self_closing {
            Cow::Borrowed("/>")
        } else {
            Cow::Owned(format!("</{}>", name))
        }
    }

    fn field_separator(&self) -> &'static str {
        " "
    }

    fn seq_open(&self) -> Cow<'static, str> {
        // XML sequences don't need wrapper elements - items render as siblings
        Cow::Borrowed("")
    }

    fn seq_close(&self) -> Cow<'static, str> {
        // XML sequences don't need wrapper elements - items render as siblings
        Cow::Borrowed("")
    }

    fn item_separator(&self) -> &'static str {
        " "
    }

    fn format_seq_item<'a>(&self, item_type: &str, value: &'a str) -> Cow<'a, str> {
        // Wrap each item in an element tag: <i32>0</i32>
        Cow::Owned(format!("<{}>{}</{}>", item_type, value, item_type))
    }

    fn comment(&self, text: &str) -> String {
        format!("<!-- {} -->", text)
    }

    fn format_field(&self, name: &str, value: &str) -> String {
        format!("{}=\"{}\"", name, value)
    }

    fn format_field_prefix(&self, name: &str) -> String {
        format!("{}=\"", name)
    }

    fn format_field_suffix(&self) -> &'static str {
        "\""
    }

    fn struct_open_close(&self) -> &'static str {
        ">"
    }

    fn format_child_open(&self, _name: &str) -> Cow<'static, str> {
        // XML: nested elements don't use attribute-style prefix
        // The nested element tag is self-describing
        Cow::Borrowed("")
    }

    fn format_child_close(&self, _name: &str) -> Cow<'static, str> {
        Cow::Borrowed("")
    }

    fn trailing_separator(&self) -> &'static str {
        // XML doesn't use trailing commas/separators
        ""
    }

    fn format_seq_field_open(&self, _field_name: &str) -> String {
        // XML: sequences render items directly without wrapper elements
        // The items are children of the parent element
        String::new()
    }

    fn format_seq_field_close(&self, _field_name: &str) -> Cow<'static, str> {
        // XML: sequences render items directly without wrapper elements
        Cow::Borrowed("")
    }
}

/// Value formatting with quotes for strings (Rust/JSON style).
fn format_value_quoted(peek: Peek<'_, '_>, w: &mut dyn Write) -> std::fmt::Result {
    use facet_core::{PointerType, TextualType};

    let shape = peek.shape();

    match (shape.def, shape.ty) {
        // Strings: write with quotes
        (_, Type::Primitive(PrimitiveType::Textual(TextualType::Str))) => {
            write!(w, "\"{}\"", peek.get::<str>().unwrap())
        }
        // String type (owned)
        (Def::Scalar, _) if shape.id == <String as facet_core::Facet>::SHAPE.id => {
            write!(w, "\"{}\"", peek.get::<String>().unwrap())
        }
        // Reference to str (&str) - check if target is str
        (_, Type::Pointer(PointerType::Reference(ptr)))
            if matches!(
                ptr.target.ty,
                Type::Primitive(PrimitiveType::Textual(TextualType::Str))
            ) =>
        {
            // Use Display which will show the string content
            write!(w, "\"{}\"", peek)
        }
        // Booleans
        (Def::Scalar, Type::Primitive(PrimitiveType::Boolean)) => {
            let b = peek.get::<bool>().unwrap();
            write!(w, "{}", if *b { "true" } else { "false" })
        }
        // Chars: show with single quotes for Rust
        (Def::Scalar, Type::Primitive(PrimitiveType::Textual(TextualType::Char))) => {
            write!(w, "'{}'", peek.get::<char>().unwrap())
        }
        // Everything else: use Display if available, else Debug
        _ => {
            if shape.is_display() {
                write!(w, "{}", peek)
            } else if shape.is_debug() {
                write!(w, "{:?}", peek)
            } else {
                write!(w, "<{}>", shape.type_identifier)
            }
        }
    }
}

/// Value formatting without quotes (XML style - quotes come from attribute syntax).
fn format_value_raw(peek: Peek<'_, '_>, w: &mut dyn Write) -> std::fmt::Result {
    use facet_core::{DynValueKind, TextualType};

    let shape = peek.shape();

    match (shape.def, shape.ty) {
        // Strings: write raw content (no quotes)
        (_, Type::Primitive(PrimitiveType::Textual(TextualType::Str))) => {
            write!(w, "{}", peek.get::<str>().unwrap())
        }
        // String type (owned)
        (Def::Scalar, _) if shape.id == <String as facet_core::Facet>::SHAPE.id => {
            write!(w, "{}", peek.get::<String>().unwrap())
        }
        // Booleans
        (Def::Scalar, Type::Primitive(PrimitiveType::Boolean)) => {
            let b = peek.get::<bool>().unwrap();
            write!(w, "{}", if *b { "true" } else { "false" })
        }
        // Chars: show as-is
        (Def::Scalar, Type::Primitive(PrimitiveType::Textual(TextualType::Char))) => {
            write!(w, "{}", peek.get::<char>().unwrap())
        }
        // Dynamic values: handle based on their kind
        (Def::DynamicValue(_), _) => {
            // Write string without quotes for XML
            if let Ok(dv) = peek.into_dynamic_value()
                && dv.kind() == DynValueKind::String
                && let Some(s) = dv.as_str()
            {
                return write!(w, "{}", s);
            }
            // Fall back to Display for other dynamic values
            if shape.is_display() {
                write!(w, "{}", peek)
            } else if shape.is_debug() {
                write!(w, "{:?}", peek)
            } else {
                write!(w, "<{}>", shape.type_identifier)
            }
        }
        // Everything else: use Display if available, else Debug
        _ => {
            if shape.is_display() {
                write!(w, "{}", peek)
            } else if shape.is_debug() {
                write!(w, "{:?}", peek)
            } else {
                write!(w, "<{}>", shape.type_identifier)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_core::{Shape, Type, UserType};

    // Helper to get field from a struct shape
    fn get_field<'a>(shape: &'a Shape, name: &str) -> &'a Field {
        if let Type::User(UserType::Struct(st)) = shape.ty {
            st.fields.iter().find(|f| f.name == name).unwrap()
        } else {
            panic!("expected struct type")
        }
    }

    #[test]
    fn test_rust_flavor_field_presentation() {
        #[derive(Facet)]
        struct Point {
            x: i32,
            y: i32,
        }

        let shape = <Point as Facet>::SHAPE;
        let flavor = RustFlavor;

        let x_field = get_field(shape, "x");
        let y_field = get_field(shape, "y");

        // Rust flavor: all fields are attributes
        assert_eq!(
            flavor.field_presentation(x_field),
            FieldPresentation::Attribute {
                name: Cow::Borrowed("x")
            }
        );
        assert_eq!(
            flavor.field_presentation(y_field),
            FieldPresentation::Attribute {
                name: Cow::Borrowed("y")
            }
        );
    }

    #[test]
    fn test_json_flavor_field_presentation() {
        #[derive(Facet)]
        struct Point {
            x: i32,
            y: i32,
        }

        let shape = <Point as Facet>::SHAPE;
        let flavor = JsonFlavor;

        let x_field = get_field(shape, "x");

        // JSON flavor: all fields are attributes
        assert_eq!(
            flavor.field_presentation(x_field),
            FieldPresentation::Attribute {
                name: Cow::Borrowed("x")
            }
        );
    }

    #[test]
    fn test_xml_flavor_field_presentation_default() {
        // Without XML attributes, fields default to Child
        #[derive(Facet)]
        struct Book {
            title: String,
            author: String,
        }

        let shape = <Book as Facet>::SHAPE;
        let flavor = XmlFlavor;

        let title_field = get_field(shape, "title");

        // XML default: child element
        assert_eq!(
            flavor.field_presentation(title_field),
            FieldPresentation::Child {
                name: Cow::Borrowed("title")
            }
        );
    }

    fn format_to_string<F: DiffFlavor>(flavor: &F, peek: Peek<'_, '_>) -> String {
        let mut buf = String::new();
        flavor.format_value(peek, &mut buf).unwrap();
        buf
    }

    #[test]
    fn test_format_value_integers() {
        let value = 42i32;
        let peek = Peek::new(&value);

        assert_eq!(format_to_string(&RustFlavor, peek), "42");
        assert_eq!(format_to_string(&JsonFlavor, peek), "42");
        assert_eq!(format_to_string(&XmlFlavor, peek), "42");
    }

    #[test]
    fn test_format_value_strings() {
        let value = "hello";
        let peek = Peek::new(&value);

        // Rust/JSON add quotes around strings, XML doesn't (quotes come from attr syntax)
        assert_eq!(format_to_string(&RustFlavor, peek), "\"hello\"");
        assert_eq!(format_to_string(&JsonFlavor, peek), "\"hello\"");
        assert_eq!(format_to_string(&XmlFlavor, peek), "hello");
    }

    #[test]
    fn test_format_value_booleans() {
        let t = true;
        let f = false;

        assert_eq!(format_to_string(&RustFlavor, Peek::new(&t)), "true");
        assert_eq!(format_to_string(&RustFlavor, Peek::new(&f)), "false");
        assert_eq!(format_to_string(&JsonFlavor, Peek::new(&t)), "true");
        assert_eq!(format_to_string(&JsonFlavor, Peek::new(&f)), "false");
        assert_eq!(format_to_string(&XmlFlavor, Peek::new(&t)), "true");
        assert_eq!(format_to_string(&XmlFlavor, Peek::new(&f)), "false");
    }

    #[test]
    fn test_syntax_methods() {
        let rust = RustFlavor;
        let json = JsonFlavor;
        let xml = XmlFlavor;

        // struct_open
        assert_eq!(rust.struct_open("Point"), "Point {");
        assert_eq!(json.struct_open("Point"), "{");
        assert_eq!(xml.struct_open("Point"), "<Point");

        // type_comment (rendered separately in muted color)
        assert_eq!(rust.type_comment("Point"), None);
        assert_eq!(json.type_comment("Point"), Some("/* Point */".to_string()));
        assert_eq!(xml.type_comment("Point"), None);

        // struct_close (non-self-closing)
        assert_eq!(rust.struct_close("Point", false), "}");
        assert_eq!(json.struct_close("Point", false), "}");
        assert_eq!(xml.struct_close("Point", false), "</Point>");

        // struct_close (self-closing)
        assert_eq!(rust.struct_close("Point", true), "}");
        assert_eq!(json.struct_close("Point", true), "}");
        assert_eq!(xml.struct_close("Point", true), "/>");

        // field_separator
        assert_eq!(rust.field_separator(), ", ");
        assert_eq!(json.field_separator(), ", ");
        assert_eq!(xml.field_separator(), " ");

        // seq_open/close
        assert_eq!(rust.seq_open(), "[");
        assert_eq!(rust.seq_close(), "]");
        assert_eq!(json.seq_open(), "[");
        assert_eq!(json.seq_close(), "]");
        // XML sequences render items as siblings without wrapper elements
        assert_eq!(xml.seq_open(), "");
        assert_eq!(xml.seq_close(), "");

        // comment
        assert_eq!(rust.comment("5 more"), "/* 5 more */");
        assert_eq!(json.comment("5 more"), "// 5 more");
        assert_eq!(xml.comment("5 more"), "<!-- 5 more -->");

        // format_field
        assert_eq!(rust.format_field("x", "10"), "x: 10");
        assert_eq!(json.format_field("x", "10"), "\"x\": 10");
        assert_eq!(xml.format_field("x", "10"), "x=\"10\"");
    }
}
