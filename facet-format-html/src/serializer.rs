//! HTML serializer implementing `FormatSerializer`.

extern crate alloc;

use alloc::{borrow::Cow, string::String, vec::Vec};
use std::io::Write;

use facet_core::Facet;
use facet_format::{FieldOrdering, FormatSerializer, ScalarValue, SerializeError, serialize_root};
use facet_reflect::Peek;

/// A function that formats a floating-point number to a writer.
pub type FloatFormatter = fn(f64, &mut dyn Write) -> std::io::Result<()>;

/// HTML5 void elements that don't have closing tags.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// HTML5 boolean attributes that are written without a value when true.
const BOOLEAN_ATTRIBUTES: &[&str] = &[
    "allowfullscreen",
    "async",
    "autofocus",
    "autoplay",
    "checked",
    "controls",
    "default",
    "defer",
    "disabled",
    "formnovalidate",
    "hidden",
    "inert",
    "ismap",
    "itemscope",
    "loop",
    "multiple",
    "muted",
    "nomodule",
    "novalidate",
    "open",
    "playsinline",
    "readonly",
    "required",
    "reversed",
    "selected",
    "shadowrootclonable",
    "shadowrootdelegatesfocus",
    "shadowrootserializable",
];

/// Options for HTML serialization.
#[derive(Clone)]
pub struct SerializeOptions {
    /// Whether to pretty-print with indentation (default: false for minified output)
    pub pretty: bool,
    /// Indentation string for pretty-printing (default: "  ")
    pub indent: Cow<'static, str>,
    /// Custom formatter for floating-point numbers (f32 and f64).
    pub float_formatter: Option<FloatFormatter>,
    /// Whether to use self-closing syntax for void elements (default: false)
    /// When false: `<br>`, when true: `<br />`
    pub self_closing_void: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            pretty: false,
            indent: Cow::Borrowed("  "),
            float_formatter: None,
            self_closing_void: false,
        }
    }
}

impl core::fmt::Debug for SerializeOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SerializeOptions")
            .field("pretty", &self.pretty)
            .field("indent", &self.indent)
            .field("float_formatter", &self.float_formatter.map(|_| "..."))
            .field("self_closing_void", &self.self_closing_void)
            .finish()
    }
}

impl SerializeOptions {
    /// Create new default options (minified output).
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable pretty-printing with default indentation.
    pub fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }

    /// Set a custom indentation string (implies pretty-printing).
    pub fn indent(mut self, indent: impl Into<Cow<'static, str>>) -> Self {
        self.indent = indent.into();
        self.pretty = true;
        self
    }

    /// Set a custom formatter for floating-point numbers.
    pub fn float_formatter(mut self, formatter: FloatFormatter) -> Self {
        self.float_formatter = Some(formatter);
        self
    }

    /// Use self-closing syntax for void elements (`<br />` instead of `<br>`).
    pub fn self_closing_void(mut self, value: bool) -> Self {
        self.self_closing_void = value;
        self
    }
}

/// Error type for HTML serialization.
#[derive(Debug)]
pub struct HtmlSerializeError {
    msg: &'static str,
}

impl core::fmt::Display for HtmlSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.msg)
    }
}

impl std::error::Error for HtmlSerializeError {}

#[derive(Debug)]
enum Ctx {
    Root,
    Struct { close: Option<String> },
    Seq { close: Option<String> },
}

/// HTML serializer with configurable output options.
pub struct HtmlSerializer {
    out: Vec<u8>,
    stack: Vec<Ctx>,
    pending_field: Option<String>,
    /// True if the current field is an attribute
    pending_is_attribute: bool,
    /// True if the current field is text content
    pending_is_text: bool,
    /// True if the current field is an elements list
    pending_is_elements: bool,
    /// Buffered attributes for the current element (name, value)
    pending_attributes: Vec<(String, String)>,
    /// True if we've written the opening root tag
    root_tag_written: bool,
    /// Name to use for the root element
    root_element_name: Option<String>,
    /// Deferred element tag - wait to write opening tag until we've collected attributes
    deferred_open_tag: Option<(String, String)>,
    /// Stack of elements state (true = in elements list)
    elements_stack: Vec<bool>,
    /// Serialization options
    options: SerializeOptions,
    /// Current indentation depth
    depth: usize,
}

impl HtmlSerializer {
    /// Create a new HTML serializer with default options (minified).
    pub fn new() -> Self {
        Self::with_options(SerializeOptions::default())
    }

    /// Create a new HTML serializer with the given options.
    pub fn with_options(options: SerializeOptions) -> Self {
        Self {
            out: Vec::new(),
            stack: vec![Ctx::Root],
            pending_field: None,
            pending_is_attribute: false,
            pending_is_text: false,
            pending_is_elements: false,
            pending_attributes: Vec::new(),
            root_tag_written: false,
            root_element_name: None,
            deferred_open_tag: None,
            elements_stack: Vec::new(),
            options,
            depth: 0,
        }
    }

    /// Finish serialization and return the output bytes.
    pub fn finish(mut self) -> Vec<u8> {
        // Flush any pending deferred tag
        self.flush_deferred_open_tag();

        // Close any remaining non-root elements
        while let Some(ctx) = self.stack.pop() {
            match ctx {
                Ctx::Root => break,
                Ctx::Struct { close } | Ctx::Seq { close } => {
                    if let Some(name) = close
                        && !is_void_element(&name)
                    {
                        self.write_close_tag(&name, true);
                    }
                }
            }
        }

        self.out
    }

    fn flush_deferred_open_tag(&mut self) {
        if let Some((element_name, _close_name)) = self.deferred_open_tag.take() {
            self.write_indent();
            self.out.push(b'<');
            self.out.extend_from_slice(element_name.as_bytes());

            // Write buffered attributes
            let attrs: Vec<_> = self.pending_attributes.drain(..).collect();
            for (attr_name, attr_value) in attrs {
                // Handle boolean attributes
                if is_boolean_attribute(&attr_name) {
                    if attr_value == "true" || attr_value == "1" || attr_value == attr_name {
                        self.out.push(b' ');
                        self.out.extend_from_slice(attr_name.as_bytes());
                    }
                    // Skip false/empty boolean attributes
                    continue;
                }

                self.out.push(b' ');
                self.out.extend_from_slice(attr_name.as_bytes());
                self.out.extend_from_slice(b"=\"");
                self.write_attr_escaped(&attr_value);
                self.out.push(b'"');
            }

            if is_void_element(&element_name) {
                if self.options.self_closing_void {
                    self.out.extend_from_slice(b" />");
                } else {
                    self.out.push(b'>');
                }
            } else {
                self.out.push(b'>');
            }
            self.write_newline();
            self.depth += 1;
        }
    }

    fn write_open_tag(&mut self, name: &str) {
        self.write_indent();
        self.out.push(b'<');
        self.out.extend_from_slice(name.as_bytes());

        // Write buffered attributes
        let attrs: Vec<_> = self.pending_attributes.drain(..).collect();
        for (attr_name, attr_value) in attrs {
            // Handle boolean attributes
            if is_boolean_attribute(&attr_name) {
                if attr_value == "true" || attr_value == "1" || attr_value == attr_name {
                    self.out.push(b' ');
                    self.out.extend_from_slice(attr_name.as_bytes());
                }
                // Skip false/empty boolean attributes
                continue;
            }

            self.out.push(b' ');
            self.out.extend_from_slice(attr_name.as_bytes());
            self.out.extend_from_slice(b"=\"");
            self.write_attr_escaped(&attr_value);
            self.out.push(b'"');
        }

        if is_void_element(name) {
            if self.options.self_closing_void {
                self.out.extend_from_slice(b" />");
            } else {
                self.out.push(b'>');
            }
        } else {
            self.out.push(b'>');
        }
    }

    fn write_close_tag(&mut self, name: &str, block: bool) {
        if is_void_element(name) {
            return; // Void elements have no closing tag
        }
        if block {
            self.depth = self.depth.saturating_sub(1);
            self.write_indent();
        }
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(name.as_bytes());
        self.out.push(b'>');
        if block {
            self.write_newline();
        }
    }

    fn write_text_escaped(&mut self, text: &str) {
        for b in text.as_bytes() {
            match *b {
                b'&' => self.out.extend_from_slice(b"&amp;"),
                b'<' => self.out.extend_from_slice(b"&lt;"),
                b'>' => self.out.extend_from_slice(b"&gt;"),
                _ => self.out.push(*b),
            }
        }
    }

    fn write_attr_escaped(&mut self, text: &str) {
        for b in text.as_bytes() {
            match *b {
                b'&' => self.out.extend_from_slice(b"&amp;"),
                b'<' => self.out.extend_from_slice(b"&lt;"),
                b'>' => self.out.extend_from_slice(b"&gt;"),
                b'"' => self.out.extend_from_slice(b"&quot;"),
                _ => self.out.push(*b),
            }
        }
    }

    fn format_float(&self, v: f64) -> String {
        if let Some(fmt) = self.options.float_formatter {
            let mut buf = Vec::new();
            if fmt(v, &mut buf).is_ok()
                && let Ok(s) = String::from_utf8(buf)
            {
                return s;
            }
        }
        v.to_string()
    }

    fn write_indent(&mut self) {
        if self.options.pretty {
            for _ in 0..self.depth {
                self.out.extend_from_slice(self.options.indent.as_bytes());
            }
        }
    }

    fn write_newline(&mut self) {
        if self.options.pretty {
            self.out.push(b'\n');
        }
    }

    fn ensure_root_tag_written(&mut self) {
        if !self.root_tag_written {
            let root_name = self
                .root_element_name
                .as_deref()
                .unwrap_or("div")
                .to_string();
            self.out.push(b'<');
            self.out.extend_from_slice(root_name.as_bytes());

            // Write buffered attributes
            let attrs: Vec<_> = self.pending_attributes.drain(..).collect();
            for (attr_name, attr_value) in attrs {
                if is_boolean_attribute(&attr_name) {
                    if attr_value == "true" || attr_value == "1" || attr_value == attr_name {
                        self.out.push(b' ');
                        self.out.extend_from_slice(attr_name.as_bytes());
                    }
                    continue;
                }

                self.out.push(b' ');
                self.out.extend_from_slice(attr_name.as_bytes());
                self.out.extend_from_slice(b"=\"");
                self.write_attr_escaped(&attr_value);
                self.out.push(b'"');
            }

            if is_void_element(&root_name) {
                if self.options.self_closing_void {
                    self.out.extend_from_slice(b" />");
                } else {
                    self.out.push(b'>');
                }
            } else {
                self.out.push(b'>');
                self.write_newline();
                self.depth += 1;
            }
            self.root_tag_written = true;
        }
    }

    fn open_value_element_if_needed(&mut self) -> Result<Option<String>, HtmlSerializeError> {
        self.flush_deferred_open_tag();
        self.ensure_root_tag_written();

        if let Some(field_name) = self.pending_field.take() {
            // Check if we're in elements mode - if so, don't wrap
            if self.elements_stack.last().copied().unwrap_or(false) {
                // In elements mode - the field name is the element tag
                self.write_open_tag(&field_name);
                return Ok(Some(field_name));
            }

            // Handle text content
            if self.pending_is_text {
                self.pending_is_text = false;
                return Ok(None); // Text content - no element wrapper
            }

            // Handle attributes - shouldn't get here for attributes
            if self.pending_is_attribute {
                self.pending_is_attribute = false;
                return Ok(None);
            }

            // Regular child element
            self.write_open_tag(&field_name);
            return Ok(Some(field_name));
        }
        Ok(None)
    }

    fn write_scalar_string(&mut self, value: &str) -> Result<(), HtmlSerializeError> {
        // Handle attribute values BEFORE flushing deferred tag
        // Attributes need to be buffered, not written as content
        if self.pending_is_attribute
            && let Some(attr_name) = self.pending_field.take()
        {
            self.pending_is_attribute = false;
            self.pending_attributes.push((attr_name, value.to_string()));
            return Ok(());
        }

        // Handle text content - flush deferred tag first, then write text
        if self.pending_is_text {
            self.flush_deferred_open_tag();
            self.pending_is_text = false;
            self.pending_field.take();
            self.write_text_escaped(value);
            return Ok(());
        }

        // Regular element content
        self.flush_deferred_open_tag();
        self.ensure_root_tag_written();
        let close = self.open_value_element_if_needed()?;
        self.write_text_escaped(value);
        if let Some(name) = close {
            self.write_close_tag(&name, false);
        }
        self.write_newline();
        Ok(())
    }
}

impl Default for HtmlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatSerializer for HtmlSerializer {
    type Error = HtmlSerializeError;

    fn struct_metadata(&mut self, shape: &facet_core::Shape) -> Result<(), Self::Error> {
        // Get the element name from the shape (respecting rename attribute)
        let element_name = shape
            .get_builtin_attr_value::<&str>("rename")
            .unwrap_or(shape.type_identifier);

        // If this is the root element (stack only has Root context), save the name
        if matches!(self.stack.last(), Some(Ctx::Root)) {
            self.root_element_name = Some(element_name.to_string());
        }
        Ok(())
    }

    fn field_metadata(&mut self, field_item: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        self.pending_is_attribute = field_item.field.is_attribute();
        self.pending_is_text = field_item.field.is_text();
        self.pending_is_elements = field_item.field.is_elements();
        Ok(())
    }

    fn preferred_field_order(&self) -> FieldOrdering {
        FieldOrdering::AttributesFirst
    }

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        // Flush any deferred tag from parent before starting a new struct
        self.flush_deferred_open_tag();

        // If we're starting a new struct that's an elements list item
        if self.pending_is_elements {
            self.pending_is_elements = false;
            self.elements_stack.push(true);
        } else {
            self.elements_stack.push(false);
        }

        match self.stack.last() {
            Some(Ctx::Root) => {
                // Root struct - defer the opening tag until we've collected attributes
                // The element name was set in struct_metadata
                if let Some(name) = self.root_element_name.clone() {
                    self.deferred_open_tag = Some((name.clone(), name));
                }
                self.stack.push(Ctx::Struct {
                    close: self.root_element_name.clone(),
                });
                Ok(())
            }
            Some(Ctx::Struct { .. }) | Some(Ctx::Seq { .. }) => {
                // Nested struct - defer the opening tag
                let close = if let Some(field_name) = self.pending_field.take() {
                    self.deferred_open_tag = Some((field_name.clone(), field_name.clone()));
                    Some(field_name)
                } else {
                    None
                };
                self.stack.push(Ctx::Struct { close });
                Ok(())
            }
            None => Err(HtmlSerializeError {
                msg: "serializer state missing context",
            }),
        }
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        // Flush any remaining deferred tag (in case struct had only attributes)
        self.flush_deferred_open_tag();
        self.elements_stack.pop();

        if let Some(Ctx::Struct { close }) = self.stack.pop()
            && let Some(name) = close
            && !is_void_element(&name)
        {
            self.write_close_tag(&name, true);
        }
        Ok(())
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        self.flush_deferred_open_tag();
        self.ensure_root_tag_written();

        // If this is an elements list, don't write a wrapper
        if self.pending_is_elements {
            self.pending_is_elements = false;
            self.elements_stack.push(true);
            self.pending_field.take(); // Consume the field name
            self.stack.push(Ctx::Seq { close: None });
            return Ok(());
        }

        let close = if let Some(field_name) = self.pending_field.take() {
            self.write_open_tag(&field_name);
            self.write_newline();
            self.depth += 1;
            Some(field_name)
        } else {
            None
        };
        self.elements_stack.push(false);
        self.stack.push(Ctx::Seq { close });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        self.elements_stack.pop();
        if let Some(Ctx::Seq { close }) = self.stack.pop()
            && let Some(name) = close
        {
            self.write_close_tag(&name, true);
        }
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        self.pending_field = Some(key.to_string());
        Ok(())
    }

    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error> {
        match scalar {
            ScalarValue::Null => {
                // Skip null values in HTML
                self.pending_field.take();
                self.pending_is_attribute = false;
                self.pending_is_text = false;
                Ok(())
            }
            ScalarValue::Bool(v) => {
                // Handle boolean attribute values BEFORE flushing deferred tag
                if self.pending_is_attribute
                    && let Some(attr_name) = self.pending_field.take()
                {
                    self.pending_is_attribute = false;
                    if v {
                        // For boolean attributes, just add the name
                        self.pending_attributes.push((attr_name.clone(), attr_name));
                    }
                    // false boolean attributes are omitted
                    return Ok(());
                }

                self.write_scalar_string(if v { "true" } else { "false" })
            }
            ScalarValue::I64(v) => self.write_scalar_string(&v.to_string()),
            ScalarValue::U64(v) => self.write_scalar_string(&v.to_string()),
            ScalarValue::F64(v) => {
                let s = self.format_float(v);
                self.write_scalar_string(&s)
            }
            ScalarValue::Str(s) => self.write_scalar_string(&s),
            ScalarValue::I128(v) => self.write_scalar_string(&v.to_string()),
            ScalarValue::U128(v) => self.write_scalar_string(&v.to_string()),
            ScalarValue::Bytes(_) => Err(HtmlSerializeError {
                msg: "binary data cannot be serialized to HTML",
            }),
        }
    }
}

/// Check if an element is a void element (no closing tag).
fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.iter().any(|&v| v.eq_ignore_ascii_case(name))
}

/// Check if an attribute is a boolean attribute.
fn is_boolean_attribute(name: &str) -> bool {
    BOOLEAN_ATTRIBUTES
        .iter()
        .any(|&v| v.eq_ignore_ascii_case(name))
}

// =============================================================================
// Public API
// =============================================================================

/// Serialize a value to an HTML string with default options (minified).
pub fn to_string<T: Facet<'static>>(
    value: &T,
) -> Result<String, SerializeError<HtmlSerializeError>> {
    to_string_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to a pretty-printed HTML string.
pub fn to_string_pretty<T: Facet<'static>>(
    value: &T,
) -> Result<String, SerializeError<HtmlSerializeError>> {
    to_string_with_options(value, &SerializeOptions::default().pretty())
}

/// Serialize a value to an HTML string with custom options.
pub fn to_string_with_options<T: Facet<'static>>(
    value: &T,
    options: &SerializeOptions,
) -> Result<String, SerializeError<HtmlSerializeError>> {
    let bytes = to_vec_with_options(value, options)?;
    String::from_utf8(bytes).map_err(|_| {
        SerializeError::Reflect(facet_reflect::ReflectError::InvalidOperation {
            operation: "to_string",
            reason: "invalid UTF-8 in serialized output",
        })
    })
}

/// Serialize a value to HTML bytes with default options.
pub fn to_vec<T: Facet<'static>>(value: &T) -> Result<Vec<u8>, SerializeError<HtmlSerializeError>> {
    to_vec_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to HTML bytes with custom options.
pub fn to_vec_with_options<T: Facet<'static>>(
    value: &T,
    options: &SerializeOptions,
) -> Result<Vec<u8>, SerializeError<HtmlSerializeError>> {
    let mut serializer = HtmlSerializer::with_options(options.clone());
    let peek = Peek::new(value);
    serialize_root(&mut serializer, peek)?;
    Ok(serializer.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_format_xml as xml;

    #[derive(Debug, Facet)]
    #[facet(rename = "div")]
    struct SimpleDiv {
        #[facet(xml::attribute, default)]
        class: Option<String>,
        #[facet(xml::attribute, default)]
        id: Option<String>,
        #[facet(xml::text, default)]
        text: String,
    }

    #[test]
    fn test_simple_element() {
        let div = SimpleDiv {
            class: Some("container".into()),
            id: Some("main".into()),
            text: "Hello, World!".into(),
        };

        let html = to_string(&div).unwrap();
        assert!(html.contains("<div"), "Expected <div, got: {}", html);
        assert!(
            html.contains("class=\"container\""),
            "Expected class attr, got: {}",
            html
        );
        assert!(
            html.contains("id=\"main\""),
            "Expected id attr, got: {}",
            html
        );
        assert!(
            html.contains("Hello, World!"),
            "Expected text content, got: {}",
            html
        );
        assert!(html.contains("</div>"), "Expected </div>, got: {}", html);
    }

    #[test]
    fn test_pretty_print() {
        let div = SimpleDiv {
            class: Some("test".into()),
            id: None,
            text: "Content".into(),
        };

        let html = to_string_pretty(&div).unwrap();
        assert!(
            html.contains('\n'),
            "Expected newlines in pretty output: {}",
            html
        );
    }

    #[derive(Debug, Facet)]
    #[facet(rename = "img")]
    struct Image {
        #[facet(xml::attribute)]
        src: String,
        #[facet(xml::attribute, default)]
        alt: Option<String>,
    }

    #[test]
    fn test_void_element() {
        let img = Image {
            src: "photo.jpg".into(),
            alt: Some("A photo".into()),
        };

        let html = to_string(&img).unwrap();
        assert!(html.contains("<img"), "Expected <img, got: {}", html);
        assert!(
            html.contains("src=\"photo.jpg\""),
            "Expected src attr, got: {}",
            html
        );
        assert!(
            html.contains("alt=\"A photo\""),
            "Expected alt attr, got: {}",
            html
        );
        // Void elements should not have a closing tag
        assert!(
            !html.contains("</img>"),
            "Should not have </img>, got: {}",
            html
        );
    }

    #[test]
    fn test_void_element_self_closing() {
        let img = Image {
            src: "photo.jpg".into(),
            alt: None,
        };

        let options = SerializeOptions::new().self_closing_void(true);
        let html = to_string_with_options(&img, &options).unwrap();
        assert!(html.contains("/>"), "Expected self-closing, got: {}", html);
    }

    #[derive(Debug, Facet)]
    #[facet(rename = "input")]
    struct Input {
        #[facet(xml::attribute, rename = "type")]
        input_type: String,
        #[facet(xml::attribute, default)]
        disabled: Option<bool>,
        #[facet(xml::attribute, default)]
        checked: Option<bool>,
    }

    #[test]
    fn test_boolean_attributes() {
        let input = Input {
            input_type: "checkbox".into(),
            disabled: Some(true),
            checked: Some(false),
        };

        let html = to_string(&input).unwrap();
        assert!(
            html.contains("type=\"checkbox\""),
            "Expected type attr, got: {}",
            html
        );
        assert!(
            html.contains("disabled"),
            "Expected disabled attr, got: {}",
            html
        );
        // false boolean attributes should be omitted
        assert!(
            !html.contains("checked"),
            "Should not have checked, got: {}",
            html
        );
    }

    #[test]
    fn test_escape_special_chars() {
        let div = SimpleDiv {
            class: None,
            id: None,
            text: "<script>alert('xss')</script>".into(),
        };

        let html = to_string(&div).unwrap();
        assert!(
            html.contains("&lt;script&gt;"),
            "Expected escaped script tag, got: {}",
            html
        );
        assert!(
            !html.contains("<script>"),
            "Should not have raw script tag, got: {}",
            html
        );
    }
}
