//! HTML serializer implementing `DomSerializer`.

extern crate alloc;

use alloc::{borrow::Cow, string::String, vec::Vec};
use std::io::Write;

use facet_core::{Def, Facet, ScalarType};
use facet_dom::{DomSerializeError, DomSerializer};
use facet_reflect::Peek;

/// A function that formats a floating-point number to a writer.
pub type FloatFormatter = fn(f64, &mut dyn Write) -> std::io::Result<()>;

/// HTML5 void elements that don't have closing tags.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// HTML5 elements where whitespace is significant (preformatted content).
const WHITESPACE_SENSITIVE_ELEMENTS: &[&str] = &["pre", "code", "textarea", "script", "style"];

/// HTML5 raw text elements where content should NOT be HTML-escaped.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style"];

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

/// HTML5 phrasing/inline elements that should NOT cause block formatting.
const INLINE_ELEMENTS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "br", "cite", "code", "data", "dfn", "em", "i", "kbd", "mark",
    "q", "ruby", "rt", "rp", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u",
    "var", "wbr", "img", "picture", "audio", "video", "canvas", "iframe", "embed", "object", "svg",
    "math", "button", "input", "label", "select", "textarea", "output", "meter", "progress",
    "details", "summary",
];

fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.iter().any(|&v| v.eq_ignore_ascii_case(name))
}

fn is_boolean_attribute(name: &str) -> bool {
    BOOLEAN_ATTRIBUTES
        .iter()
        .any(|&v| v.eq_ignore_ascii_case(name))
}

fn is_whitespace_sensitive(name: &str) -> bool {
    WHITESPACE_SENSITIVE_ELEMENTS
        .iter()
        .any(|&v| v.eq_ignore_ascii_case(name))
}

fn is_raw_text_element(name: &str) -> bool {
    RAW_TEXT_ELEMENTS
        .iter()
        .any(|&v| v.eq_ignore_ascii_case(name))
}

fn is_inline_element(name: &str) -> bool {
    INLINE_ELEMENTS
        .iter()
        .any(|&v| v.eq_ignore_ascii_case(name))
}

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
    pub const fn pretty(mut self) -> Self {
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
    pub const fn self_closing_void(mut self, value: bool) -> Self {
        self.self_closing_void = value;
        self
    }
}

/// Error type for HTML serialization.
#[derive(Debug)]
pub struct HtmlSerializeError {
    msg: Cow<'static, str>,
}

impl core::fmt::Display for HtmlSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for HtmlSerializeError {}

/// Element state tracking for proper formatting
struct ElementState {
    /// Element tag name (for closing tag)
    tag: String,
    /// True if this is a void element
    is_void: bool,
    /// True if we're inside a whitespace-sensitive element
    in_preformatted: bool,
    /// True if we're inside a raw text element
    in_raw_text: bool,
    /// True if this is an inline element
    is_inline: bool,
    /// True if we've written any content (for close tag formatting)
    has_content: bool,
    /// True if we've written block content (child elements)
    has_block_content: bool,
    /// True if we need to write a newline before the next block child
    /// (deferred from children_start to avoid newlines when content is only text)
    needs_newline_before_block: bool,
}

/// HTML serializer with configurable output options.
pub struct HtmlSerializer {
    out: Vec<u8>,
    /// Stack of element states
    element_stack: Vec<ElementState>,
    /// True if we're collecting attributes (between element_start and children_start)
    collecting_attributes: bool,
    /// Pending DOCTYPE to emit before first element
    pending_doctype: Option<String>,
    /// True if the current field is an attribute
    pending_is_attribute: bool,
    /// True if the current field is text content
    pending_is_text: bool,
    /// True if the current field is an elements list
    pending_is_elements: bool,
    /// True if the current field is a tag name
    pending_is_tag: bool,
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
            element_stack: Vec::new(),
            collecting_attributes: false,
            pending_doctype: None,
            pending_is_attribute: false,
            pending_is_text: false,
            pending_is_elements: false,
            pending_is_tag: false,
            options,
            depth: 0,
        }
    }

    /// Finish serialization and return the output bytes.
    pub fn finish(self) -> Vec<u8> {
        self.out
    }

    fn in_preformatted(&self) -> bool {
        self.element_stack.iter().any(|s| s.in_preformatted)
    }

    fn in_raw_text(&self) -> bool {
        self.element_stack.iter().any(|s| s.in_raw_text)
    }

    fn write_indent(&mut self) {
        if self.options.pretty && !self.in_preformatted() {
            for _ in 0..self.depth {
                self.out.extend_from_slice(self.options.indent.as_bytes());
            }
        }
    }

    fn write_newline(&mut self) {
        if self.options.pretty && !self.in_preformatted() {
            self.out.push(b'\n');
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

    fn clear_field_state_impl(&mut self) {
        self.pending_is_attribute = false;
        self.pending_is_text = false;
        self.pending_is_elements = false;
        self.pending_is_tag = false;
    }
}

impl Default for HtmlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

/// Write a scalar value to a string buffer.
/// Returns Some(string) if the value is a scalar, None otherwise.
fn scalar_to_string(
    value: Peek<'_, '_>,
    float_formatter: Option<FloatFormatter>,
) -> Option<String> {
    // Unwrap transparent wrappers
    let value = value.innermost_peek();

    // Handle Option<T>
    if let Def::Option(_) = &value.shape().def
        && let Ok(opt) = value.into_option()
    {
        return match opt.value() {
            Some(inner) => scalar_to_string(inner, float_formatter),
            None => None,
        };
    }

    let scalar_type = value.scalar_type()?;

    let s = match scalar_type {
        ScalarType::Unit => "null".to_string(),
        ScalarType::Bool => {
            let b = value.get::<bool>().ok()?;
            if *b { "true" } else { "false" }.to_string()
        }
        ScalarType::Char => {
            let c = value.get::<char>().ok()?;
            c.to_string()
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => value.as_str()?.to_string(),
        ScalarType::F32 => {
            let v = *value.get::<f32>().ok()?;
            format_float(v as f64, float_formatter)
        }
        ScalarType::F64 => {
            let v = *value.get::<f64>().ok()?;
            format_float(v, float_formatter)
        }
        ScalarType::U8 => value.get::<u8>().ok()?.to_string(),
        ScalarType::U16 => value.get::<u16>().ok()?.to_string(),
        ScalarType::U32 => value.get::<u32>().ok()?.to_string(),
        ScalarType::U64 => value.get::<u64>().ok()?.to_string(),
        ScalarType::U128 => value.get::<u128>().ok()?.to_string(),
        ScalarType::USize => value.get::<usize>().ok()?.to_string(),
        ScalarType::I8 => value.get::<i8>().ok()?.to_string(),
        ScalarType::I16 => value.get::<i16>().ok()?.to_string(),
        ScalarType::I32 => value.get::<i32>().ok()?.to_string(),
        ScalarType::I64 => value.get::<i64>().ok()?.to_string(),
        ScalarType::I128 => value.get::<i128>().ok()?.to_string(),
        ScalarType::ISize => value.get::<isize>().ok()?.to_string(),
        _ => return None,
    };

    Some(s)
}

fn format_float(v: f64, formatter: Option<FloatFormatter>) -> String {
    if let Some(fmt) = formatter {
        let mut buf = Vec::new();
        if fmt(v, &mut buf).is_ok()
            && let Ok(s) = String::from_utf8(buf)
        {
            return s;
        }
    }
    v.to_string()
}

impl DomSerializer for HtmlSerializer {
    type Error = HtmlSerializeError;

    fn element_start(&mut self, tag: &str, _namespace: Option<&str>) -> Result<(), Self::Error> {
        // Emit DOCTYPE before the first element if pending
        if let Some(doctype) = self.pending_doctype.take() {
            self.out.extend_from_slice(b"<!DOCTYPE ");
            self.out.extend_from_slice(doctype.as_bytes());
            self.out.push(b'>');
            self.write_newline();
        }

        // Mark parent as having content
        let is_inline = is_inline_element(tag);
        let needs_deferred_newline = if let Some(parent) = self.element_stack.last_mut() {
            parent.has_content = true;
            if !is_inline {
                parent.has_block_content = true;
                // Check if we need to write the deferred newline now
                if parent.needs_newline_before_block {
                    parent.needs_newline_before_block = false;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // Write the deferred newline (after releasing the borrow)
        if needs_deferred_newline {
            self.write_newline();
        }

        // Write indentation for block elements
        if !is_inline || self.element_stack.is_empty() {
            self.write_indent();
        }

        // Write opening tag
        self.out.push(b'<');
        self.out.extend_from_slice(tag.as_bytes());

        // Determine element properties
        let parent_preformatted = self.in_preformatted();
        let parent_raw_text = self.in_raw_text();

        self.element_stack.push(ElementState {
            tag: tag.to_string(),
            is_void: is_void_element(tag),
            in_preformatted: parent_preformatted || is_whitespace_sensitive(tag),
            in_raw_text: parent_raw_text || is_raw_text_element(tag),
            is_inline,
            has_content: false,
            has_block_content: false,
            needs_newline_before_block: false,
        });

        self.collecting_attributes = true;
        Ok(())
    }

    fn attribute(
        &mut self,
        name: &str,
        value: Peek<'_, '_>,
        _namespace: Option<&str>,
    ) -> Result<(), Self::Error> {
        if !self.collecting_attributes {
            return Err(HtmlSerializeError {
                msg: Cow::Borrowed("attribute() called after children_start()"),
            });
        }

        // Skip _tag pseudo-attribute (it's just for element name, already used)
        if name == "_tag" {
            return Ok(());
        }

        // Handle doctype pseudo-attribute: prepend DOCTYPE before the opening tag
        if name == "doctype" {
            if let Some(value_str) = scalar_to_string(value, self.options.float_formatter) {
                // Prepend DOCTYPE before the element we already started
                let mut new_out = Vec::new();
                new_out.extend_from_slice(b"<!DOCTYPE ");
                new_out.extend_from_slice(value_str.as_bytes());
                new_out.push(b'>');
                if self.options.pretty {
                    new_out.push(b'\n');
                }
                new_out.append(&mut self.out);
                self.out = new_out;
            }
            return Ok(());
        }

        // Get the scalar value
        let Some(value_str) = scalar_to_string(value, self.options.float_formatter) else {
            return Ok(()); // None or non-scalar - skip
        };

        // Handle boolean attributes
        if is_boolean_attribute(name) {
            if value_str == "true" || value_str == "1" || value_str == name {
                self.out.push(b' ');
                self.out.extend_from_slice(name.as_bytes());
            }
            // false/empty boolean attributes are omitted
            return Ok(());
        }

        // Regular attribute
        self.out.push(b' ');
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(b"=\"");
        self.write_attr_escaped(&value_str);
        self.out.push(b'"');

        Ok(())
    }

    fn children_start(&mut self) -> Result<(), Self::Error> {
        self.collecting_attributes = false;

        let Some(state) = self.element_stack.last_mut() else {
            return Ok(());
        };

        if state.is_void {
            // Void element - close tag immediately
            if self.options.self_closing_void {
                self.out.extend_from_slice(b" />");
            } else {
                self.out.push(b'>');
            }
        } else {
            self.out.push(b'>');
        }

        // For non-void, non-inline elements, defer the newline until we see block content.
        // This avoids `<p>\ntext</p>` when content is only text.
        if !state.is_void && !state.is_inline {
            state.needs_newline_before_block = true;
            self.depth += 1;
        }

        Ok(())
    }

    fn children_end(&mut self) -> Result<(), Self::Error> {
        // Nothing to do here - closing handled in element_end
        Ok(())
    }

    fn element_end(&mut self, _tag: &str) -> Result<(), Self::Error> {
        let Some(state) = self.element_stack.pop() else {
            return Ok(());
        };

        if state.is_void {
            // Void elements have no closing tag (already closed in children_start)
            // Only add newline if there's a parent (no trailing newline at root)
            if !state.is_inline && !self.element_stack.is_empty() {
                self.write_newline();
            }
            return Ok(());
        }

        // Decrease depth and indent for block elements with block content
        if !state.is_inline && state.has_block_content {
            self.depth = self.depth.saturating_sub(1);
            self.write_indent();
        } else if !state.is_inline {
            self.depth = self.depth.saturating_sub(1);
        }

        // Write closing tag
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(state.tag.as_bytes());
        self.out.push(b'>');

        // Newline after block elements, but only if there's a parent element
        // (no trailing newline at the root level)
        if !state.is_inline && !self.element_stack.is_empty() {
            self.write_newline();
        }

        Ok(())
    }

    fn text(&mut self, content: &str) -> Result<(), Self::Error> {
        // Mark parent as having content
        if let Some(parent) = self.element_stack.last_mut() {
            parent.has_content = true;
        }

        // In raw text elements, don't escape
        if self.in_raw_text() {
            self.out.extend_from_slice(content.as_bytes());
        } else {
            self.write_text_escaped(content);
        }

        Ok(())
    }

    fn struct_metadata(&mut self, _shape: &facet_core::Shape) -> Result<(), Self::Error> {
        Ok(())
    }

    fn field_metadata(&mut self, field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        let Some(field_def) = field.field else {
            // Flattened map entries are attributes
            self.pending_is_attribute = true;
            return Ok(());
        };

        self.pending_is_attribute = field_def.get_attr(Some("html"), "attribute").is_some();
        self.pending_is_text = field_def.get_attr(Some("html"), "text").is_some();
        self.pending_is_elements = field_def.get_attr(Some("html"), "elements").is_some();
        self.pending_is_tag = field_def.get_attr(Some("html"), "tag").is_some();

        Ok(())
    }

    fn variant_metadata(
        &mut self,
        _variant: &'static facet_core::Variant,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_attribute_field(&self) -> bool {
        self.pending_is_attribute
    }

    fn is_text_field(&self) -> bool {
        self.pending_is_text
    }

    fn is_elements_field(&self) -> bool {
        self.pending_is_elements
    }

    fn is_tag_field(&self) -> bool {
        self.pending_is_tag
    }

    fn clear_field_state(&mut self) {
        self.clear_field_state_impl();
    }

    fn format_float(&self, value: f64) -> String {
        format_float(value, self.options.float_formatter)
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("html")
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Serialize a value to an HTML string with default options (minified).
pub fn to_string<'facet, T>(value: &T) -> Result<String, DomSerializeError<HtmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_string_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to a pretty-printed HTML string.
pub fn to_string_pretty<'facet, T>(
    value: &T,
) -> Result<String, DomSerializeError<HtmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_string_with_options(value, &SerializeOptions::default().pretty())
}

/// Serialize a value to an HTML string with custom options.
pub fn to_string_with_options<'facet, T>(
    value: &T,
    options: &SerializeOptions,
) -> Result<String, DomSerializeError<HtmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec_with_options(value, options)?;
    String::from_utf8(bytes).map_err(|_| {
        DomSerializeError::Unsupported(Cow::Borrowed("invalid UTF-8 in serialized output"))
    })
}

/// Serialize a value to HTML bytes with default options.
pub fn to_vec<'facet, T>(value: &T) -> Result<Vec<u8>, DomSerializeError<HtmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    to_vec_with_options(value, &SerializeOptions::default())
}

/// Serialize a value to HTML bytes with custom options.
pub fn to_vec_with_options<'facet, T>(
    value: &T,
    options: &SerializeOptions,
) -> Result<Vec<u8>, DomSerializeError<HtmlSerializeError>>
where
    T: Facet<'facet> + ?Sized,
{
    let mut serializer = HtmlSerializer::with_options(options.clone());
    facet_dom::serialize(&mut serializer, Peek::new(value))?;
    Ok(serializer.finish())
}

// Note: Unit tests for serialization are in tests/serializer_test.rs
// to avoid macro export issues when using `html::attribute` etc. within the crate.
