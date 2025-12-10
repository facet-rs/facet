//! Diff output flavors (Rust, JSON, XML).
//!
//! Each flavor knows how to present struct fields and format values
//! according to its format's conventions.

use std::borrow::Cow;

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
    /// Format a scalar/leaf value as a string.
    ///
    /// The returned string should NOT include surrounding quotes for strings -
    /// the renderer will add appropriate syntax based on context.
    fn format_value(&self, peek: Peek<'_, '_>) -> String;

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
    /// - JSON: `,`
    /// - XML: ` ` (space between attributes)
    fn field_separator(&self) -> &'static str;

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

    /// Format a comment (for collapsed items).
    /// - Rust: `/* ...5 more */`
    /// - JSON: `// ...5 more`
    /// - XML: `<!-- ...5 more -->`
    fn comment(&self, text: &str) -> String;
}

/// Rust-style output flavor.
///
/// Produces output like: `Point { x: 10, y: 20 }`
#[derive(Debug, Clone, Default)]
pub struct RustFlavor;

impl DiffFlavor for RustFlavor {
    fn format_value(&self, peek: Peek<'_, '_>) -> String {
        format_value_common(peek)
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
}

/// JSON-style output flavor.
///
/// Produces output like: `{ "x": 10, "y": 20 }`
#[derive(Debug, Clone, Default)]
pub struct JsonFlavor;

impl DiffFlavor for JsonFlavor {
    fn format_value(&self, peek: Peek<'_, '_>) -> String {
        format_value_common(peek)
    }

    fn field_presentation(&self, field: &Field) -> FieldPresentation {
        // JSON flavor: all fields are attributes ("key": value)
        FieldPresentation::Attribute {
            name: Cow::Borrowed(field.name),
        }
    }

    fn struct_open(&self, _name: &str) -> Cow<'static, str> {
        // JSON doesn't show type names
        Cow::Borrowed("{")
    }

    fn struct_close(&self, _name: &str, _self_closing: bool) -> Cow<'static, str> {
        Cow::Borrowed("}")
    }

    fn field_separator(&self) -> &'static str {
        ","
    }

    fn seq_open(&self) -> Cow<'static, str> {
        Cow::Borrowed("[")
    }

    fn seq_close(&self) -> Cow<'static, str> {
        Cow::Borrowed("]")
    }

    fn item_separator(&self) -> &'static str {
        ","
    }

    fn comment(&self, text: &str) -> String {
        // JSON doesn't have comments, but we use // for readability
        format!("// {}", text)
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
    fn format_value(&self, peek: Peek<'_, '_>) -> String {
        format_value_common(peek)
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
        // XML sequences don't have explicit delimiters
        Cow::Borrowed("")
    }

    fn seq_close(&self) -> Cow<'static, str> {
        Cow::Borrowed("")
    }

    fn item_separator(&self) -> &'static str {
        ""
    }

    fn comment(&self, text: &str) -> String {
        format!("<!-- {} -->", text)
    }
}

/// Common value formatting - returns raw value without surrounding quotes.
/// The renderer adds appropriate syntax based on context.
fn format_value_common(peek: Peek<'_, '_>) -> String {
    use facet_core::TextualType;

    let shape = peek.shape();

    match (shape.def, shape.ty) {
        // Strings: return raw content (no quotes - renderer adds them in context)
        (_, Type::Primitive(PrimitiveType::Textual(TextualType::Str))) => {
            peek.get::<str>().unwrap().to_string()
        }
        // String type (owned)
        (Def::Scalar, _) if shape.id == <String as facet_core::Facet>::SHAPE.id => {
            peek.get::<String>().unwrap().to_string()
        }
        // Booleans
        (Def::Scalar, Type::Primitive(PrimitiveType::Boolean)) => {
            let b = peek.get::<bool>().unwrap();
            if *b { "true" } else { "false" }.to_string()
        }
        // Chars: show as-is (no quotes)
        (Def::Scalar, Type::Primitive(PrimitiveType::Textual(TextualType::Char))) => {
            peek.get::<char>().unwrap().to_string()
        }
        // Everything else: use Display if available, else Debug
        _ => {
            if shape.is_display() {
                format!("{}", peek)
            } else if shape.is_debug() {
                format!("{:?}", peek)
            } else {
                format!("<{}>", shape.type_identifier)
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

    #[test]
    fn test_xml_flavor_field_presentation_with_attrs() {
        use facet_xml as xml;

        #[derive(Facet)]
        struct Element {
            #[facet(xml::attribute)]
            id: String,
            #[facet(xml::element)]
            title: String,
            #[facet(xml::text)]
            content: String,
            #[facet(xml::elements)]
            items: Vec<String>,
        }

        let shape = <Element as Facet>::SHAPE;
        let flavor = XmlFlavor;

        let id_field = get_field(shape, "id");
        let title_field = get_field(shape, "title");
        let content_field = get_field(shape, "content");
        let items_field = get_field(shape, "items");

        assert_eq!(
            flavor.field_presentation(id_field),
            FieldPresentation::Attribute {
                name: Cow::Borrowed("id")
            }
        );

        assert_eq!(
            flavor.field_presentation(title_field),
            FieldPresentation::Child {
                name: Cow::Borrowed("title")
            }
        );

        assert_eq!(
            flavor.field_presentation(content_field),
            FieldPresentation::TextContent
        );

        assert_eq!(
            flavor.field_presentation(items_field),
            FieldPresentation::Children {
                item_name: Cow::Borrowed("items")
            }
        );
    }

    #[test]
    fn test_format_value_integers() {
        let value = 42i32;
        let peek = Peek::new(&value);

        assert_eq!(RustFlavor.format_value(peek), "42");
        assert_eq!(JsonFlavor.format_value(peek), "42");
        assert_eq!(XmlFlavor.format_value(peek), "42");
    }

    #[test]
    fn test_format_value_strings() {
        let value = "hello";
        let peek = Peek::new(&value);

        // All flavors return raw string content (no quotes)
        assert_eq!(RustFlavor.format_value(peek), "hello");
        assert_eq!(JsonFlavor.format_value(peek), "hello");
        assert_eq!(XmlFlavor.format_value(peek), "hello");
    }

    #[test]
    fn test_format_value_booleans() {
        let t = true;
        let f = false;

        assert_eq!(RustFlavor.format_value(Peek::new(&t)), "true");
        assert_eq!(RustFlavor.format_value(Peek::new(&f)), "false");
        assert_eq!(JsonFlavor.format_value(Peek::new(&t)), "true");
        assert_eq!(JsonFlavor.format_value(Peek::new(&f)), "false");
        assert_eq!(XmlFlavor.format_value(Peek::new(&t)), "true");
        assert_eq!(XmlFlavor.format_value(Peek::new(&f)), "false");
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
        assert_eq!(json.field_separator(), ",");
        assert_eq!(xml.field_separator(), " ");

        // seq_open/close
        assert_eq!(rust.seq_open(), "[");
        assert_eq!(rust.seq_close(), "]");
        assert_eq!(json.seq_open(), "[");
        assert_eq!(json.seq_close(), "]");
        assert_eq!(xml.seq_open(), "");
        assert_eq!(xml.seq_close(), "");

        // comment
        assert_eq!(rust.comment("5 more"), "/* 5 more */");
        assert_eq!(json.comment("5 more"), "// 5 more");
        assert_eq!(xml.comment("5 more"), "<!-- 5 more -->");
    }
}
