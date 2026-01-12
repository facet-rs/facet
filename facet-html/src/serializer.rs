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

/// HTML5 elements where whitespace is significant (preformatted content).
/// These elements should NOT have indentation or newlines added during serialization.
const WHITESPACE_SENSITIVE_ELEMENTS: &[&str] = &["pre", "code", "textarea", "script", "style"];

/// HTML5 raw text elements where content should NOT be HTML-escaped.
/// The HTML5 spec defines these as elements whose content is treated as raw text.
/// See: <https://html.spec.whatwg.org/multipage/parsing.html#raw-text-elements>
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
/// These elements can appear inline within text and shouldn't have newlines around them.
const INLINE_ELEMENTS: &[&str] = &[
    // Text-level semantics
    "a", "abbr", "b", "bdi", "bdo", "br", "cite", "code", "data", "dfn", "em", "i", "kbd", "mark",
    "q", "ruby", "rt", "rp", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u",
    "var", "wbr",
    // Embedded content (inline)
    "img", "picture", "audio", "video", "canvas", "iframe", "embed", "object", "svg", "math",
    // Form elements that can appear inline
    "button", "input", "label", "select", "textarea", "output", "meter", "progress",
    // Interactive
    "details", "summary",
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
    Struct {
        close: Option<String>,
        /// True if we've written any content inside this struct (text or child elements)
        has_content: bool,
        /// True if we've written block content (child elements) that requires newlines
        has_block_content: bool,
        /// True if we're inside a whitespace-sensitive element (pre, code, etc.)
        in_preformatted: bool,
        /// True if we're inside a raw text element (script, style) where content shouldn't be escaped
        in_raw_text: bool,
    },
    Seq {
        close: Option<String>,
        /// True if we're inside a whitespace-sensitive element (pre, code, etc.)
        in_preformatted: bool,
        /// True if we're inside a raw text element (script, style) where content shouldn't be escaped
        in_raw_text: bool,
    },
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
    /// When set, we're about to serialize an externally-tagged enum inside xml::elements.
    /// The next begin_struct() should be skipped (it's the wrapper struct), and the
    /// following field_key(variant_name) should also be skipped because variant_metadata
    /// already set up pending_field with the variant name.
    skip_enum_wrapper: Option<String>,
    /// When true, the next scalar value is a tag name for a custom element
    pending_is_tag: bool,
    /// Serialization options
    options: SerializeOptions,
    /// Current indentation depth
    depth: usize,
    /// DOCTYPE declaration to emit before the root element (e.g., "html" for `<!DOCTYPE html>`)
    pending_doctype: Option<String>,
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
            skip_enum_wrapper: None,
            pending_is_tag: false,
            options,
            depth: 0,
            pending_doctype: None,
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
                Ctx::Struct {
                    close,
                    has_block_content,
                    ..
                } => {
                    if let Some(name) = close
                        && !is_void_element(&name)
                    {
                        self.write_close_tag(&name, has_block_content);
                    }
                }
                Ctx::Seq { close, .. } => {
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

    /// Flush the deferred open tag.
    ///
    /// If `inline` is true, the content will be inline (text), so we don't add
    /// a newline after the opening tag. If false, content is block-level (child
    /// elements) so we add a newline and increase indentation.
    fn flush_deferred_open_tag_with_mode(&mut self, inline: bool) {
        if let Some((element_name, _close_name)) = self.deferred_open_tag.take() {
            // Emit DOCTYPE declaration before the root element if present
            if let Some(doctype) = self.pending_doctype.take() {
                self.out.extend_from_slice(b"<!DOCTYPE ");
                self.out.extend_from_slice(doctype.as_bytes());
                self.out.push(b'>');
                self.write_newline();
            }

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

            // Only add newline and increase depth for block content
            if !inline {
                self.write_newline();
                self.depth += 1;
            }

            // If this was the root element, mark it as written
            if self.root_element_name.as_deref() == Some(&element_name) {
                self.root_tag_written = true;
            }
        }
    }

    fn flush_deferred_open_tag(&mut self) {
        self.flush_deferred_open_tag_with_mode(false)
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

    /// Write a closing tag.
    ///
    /// - `indent_before`: if true, decrement depth, add newline if needed, and write indent before the tag
    /// - `newline_after`: if true, write a newline after the tag
    fn write_close_tag_ex(&mut self, name: &str, indent_before: bool, newline_after: bool) {
        if is_void_element(name) {
            return; // Void elements have no closing tag
        }
        if indent_before {
            self.depth = self.depth.saturating_sub(1);
            // Add newline before indent only if output doesn't already end with newline
            // (e.g., after inline content that didn't add newline, but not after block content that did)
            if !self.out.ends_with(b"\n") {
                self.write_newline();
            }
            self.write_indent();
        }
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(name.as_bytes());
        self.out.push(b'>');
        if newline_after {
            self.write_newline();
        }
    }

    fn write_close_tag(&mut self, name: &str, block: bool) {
        self.write_close_tag_ex(name, block, block)
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
        #[cfg(feature = "fast")]
        return zmij::Buffer::new().format(v).to_string();
        #[cfg(not(feature = "fast"))]
        v.to_string()
    }

    /// Check if we're currently inside a whitespace-sensitive element.
    fn in_preformatted(&self) -> bool {
        for ctx in self.stack.iter().rev() {
            match ctx {
                Ctx::Struct {
                    in_preformatted: true,
                    ..
                }
                | Ctx::Seq {
                    in_preformatted: true,
                    ..
                } => return true,
                _ => {}
            }
        }
        false
    }

    /// Check if we're currently inside a raw text element (script, style).
    fn in_raw_text(&self) -> bool {
        for ctx in self.stack.iter().rev() {
            match ctx {
                Ctx::Struct {
                    in_raw_text: true, ..
                }
                | Ctx::Seq {
                    in_raw_text: true, ..
                } => return true,
                _ => {}
            }
        }
        false
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
        // Handle tag field for custom elements BEFORE flushing deferred tag
        // The tag value sets the element name for the current deferred element
        if self.pending_is_tag {
            self.pending_is_tag = false;
            self.pending_field.take();
            // Update the deferred tag with the custom element's tag name
            if let Some((ref mut element_name, ref mut _close_name)) = self.deferred_open_tag {
                *element_name = value.to_string();
                *_close_name = value.to_string();
            } else {
                // If there's no deferred tag yet, set up pending_field for when begin_struct is called
                self.pending_field = Some(value.to_string());
            }
            // Also update the close tag in the current struct context
            if let Some(Ctx::Struct { close, .. }) = self.stack.last_mut() {
                *close = Some(value.to_string());
            }
            return Ok(());
        }

        // Handle attribute values BEFORE flushing deferred tag
        // Attributes need to be buffered, not written as content
        if self.pending_is_attribute
            && let Some(attr_name) = self.pending_field.take()
        {
            self.pending_is_attribute = false;
            // Special handling for "doctype" pseudo-attribute on the root element
            // This is emitted as <!DOCTYPE {value}> before the opening tag, not as an attribute
            if attr_name == "doctype" && matches!(self.stack.last(), Some(Ctx::Struct { .. })) {
                self.pending_doctype = Some(value.to_string());
                return Ok(());
            }
            self.pending_attributes.push((attr_name, value.to_string()));
            return Ok(());
        }

        // Handle text content - flush deferred tag first (inline mode), then write text
        if self.pending_is_text {
            // Use inline mode so we don't add newline after opening tag
            self.flush_deferred_open_tag_with_mode(true);
            self.pending_is_text = false;
            self.pending_field.take();
            // In raw text elements (script, style), content should NOT be escaped
            if self.in_raw_text() {
                self.out.extend_from_slice(value.as_bytes());
            } else {
                self.write_text_escaped(value);
            }

            // Mark parent struct as having content (but NOT block content)
            if let Some(Ctx::Struct { has_content, .. }) = self.stack.last_mut() {
                *has_content = true;
            }
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

        // If we're inside an xml::elements list and no pending field is set,
        // use the shape's element name. However, if variant_metadata already
        // set a pending_field (for enums), don't override it.
        if self.elements_stack.last() == Some(&true)
            && self.pending_field.is_none()
            && self.skip_enum_wrapper.is_none()
        {
            self.pending_field = Some(element_name.to_string());
        }

        Ok(())
    }

    fn field_metadata(&mut self, field_item: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        // For flattened map entries (field is None), treat as attributes
        if let Some(field) = field_item.field {
            self.pending_is_attribute = field.is_attribute();
            self.pending_is_text = field.is_text();
            self.pending_is_elements = field.is_elements();
            self.pending_is_tag = field.is_tag();
        } else {
            // Flattened map entries are attributes
            self.pending_is_attribute = true;
            self.pending_is_text = false;
            self.pending_is_elements = false;
            self.pending_is_tag = false;
        }
        Ok(())
    }

    fn variant_metadata(
        &mut self,
        variant: &'static facet_core::Variant,
    ) -> Result<(), Self::Error> {
        // If we're inside an xml::elements list, set the pending field to the variant name
        // and mark that we should skip the externally-tagged wrapper struct.
        //
        // For externally-tagged enums, the serialization flow is:
        //   1. variant_metadata(variant) - we're here
        //   2. begin_struct() - creates wrapper struct (we want to SKIP this)
        //   3. field_key(variant.name) - sets field name (we want to SKIP this)
        //   4. shared_serialize(inner) - serializes the actual content
        //
        // We set pending_field to the variant name, and skip_enum_wrapper to tell
        // begin_struct() to not create an element, and field_key() to not override
        // the pending_field we just set.
        if self.elements_stack.last() == Some(&true) {
            // Check if this variant is marked as text content (e.g., #[facet(html::text)])
            // Text variants should be serialized as text content, not as elements.
            if variant.is_text() {
                self.pending_is_text = true;
                self.skip_enum_wrapper = Some(variant.name.to_string());
            } else if variant.is_custom_element() {
                // Custom element variant - DON'T set pending_field yet.
                // The tag name will come from the html::tag field in the struct.
                // Set skip flag so we skip the wrapper struct machinery.
                self.skip_enum_wrapper = Some(variant.name.to_string());
            } else {
                // Get the element name from the variant (respecting rename attribute)
                let element_name = variant
                    .get_builtin_attr("rename")
                    .and_then(|attr| attr.get_as::<&str>().copied())
                    .unwrap_or(variant.name);
                self.pending_field = Some(element_name.to_string());
                // Set the skip flag with the variant name so field_key knows what to skip
                self.skip_enum_wrapper = Some(variant.name.to_string());
            }
        }
        Ok(())
    }

    fn preferred_field_order(&self) -> FieldOrdering {
        FieldOrdering::AttributesFirst
    }

    fn begin_struct(&mut self) -> Result<(), Self::Error> {
        // Check if this struct will create an element (has pending_field) or is flattened (no pending_field)
        // Flattened structs (like GlobalAttrs) don't create elements and shouldn't trigger formatting changes
        let has_element = self.pending_field.is_some();

        // Check if this element is inline (phrasing content) - inline elements shouldn't
        // cause block formatting in their parent
        let is_inline = self
            .pending_field
            .as_ref()
            .map(|name| is_inline_element(name))
            .unwrap_or(false);

        // Only flush deferred tag and mark content if this struct creates an element.
        // Flattened structs (no pending_field) are just adding attributes, not content.
        if has_element {
            // Flush any deferred tag from parent before starting a new struct
            // Use inline mode if this child element is inline (so parent doesn't get newline after opening tag)
            self.flush_deferred_open_tag_with_mode(is_inline);

            // Mark nearest ancestor struct as having content (and block content if not inline)
            // We need to find the Struct even if there's a Seq in between (for elements lists)
            for ctx in self.stack.iter_mut().rev() {
                if let Ctx::Struct {
                    has_content,
                    has_block_content,
                    ..
                } = ctx
                {
                    *has_content = true;
                    // Only mark as block content if the child element is a block element
                    if !is_inline {
                        *has_block_content = true;
                    }
                    break;
                }
            }
        }

        // If we're skipping the enum wrapper struct (for xml::elements enum serialization),
        // just push a struct context without creating any element.
        // Keep the elements_stack state - we're still inside the elements list.
        if self.skip_enum_wrapper.is_some() {
            // Propagate the current elements state to maintain the "in elements" context
            let in_elements = self.elements_stack.last().copied().unwrap_or(false);
            // Propagate preformatted and raw text context from parent
            let in_preformatted = self.in_preformatted();
            let in_raw_text = self.in_raw_text();
            self.elements_stack.push(in_elements);
            self.stack.push(Ctx::Struct {
                close: None,
                has_content: false,
                has_block_content: false,
                in_preformatted,
                in_raw_text,
            });
            return Ok(());
        }

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
                let element_name = self.root_element_name.clone();
                let in_preformatted = element_name
                    .as_ref()
                    .map(|n| is_whitespace_sensitive(n))
                    .unwrap_or(false);
                let in_raw_text = element_name
                    .as_ref()
                    .map(|n| is_raw_text_element(n))
                    .unwrap_or(false);
                if let Some(name) = element_name.clone() {
                    self.deferred_open_tag = Some((name.clone(), name));
                }
                self.stack.push(Ctx::Struct {
                    close: element_name,
                    has_content: false,
                    has_block_content: false,
                    in_preformatted,
                    in_raw_text,
                });
                Ok(())
            }
            Some(Ctx::Struct { .. }) | Some(Ctx::Seq { .. }) => {
                // Nested struct - defer the opening tag
                // Check if parent is preformatted/raw_text, or if this element is
                let parent_preformatted = self.in_preformatted();
                let parent_raw_text = self.in_raw_text();
                let close = if let Some(field_name) = self.pending_field.take() {
                    self.deferred_open_tag = Some((field_name.clone(), field_name.clone()));
                    Some(field_name)
                } else {
                    None
                };
                let in_preformatted = parent_preformatted
                    || close
                        .as_ref()
                        .map(|n| is_whitespace_sensitive(n))
                        .unwrap_or(false);
                let in_raw_text = parent_raw_text
                    || close
                        .as_ref()
                        .map(|n| is_raw_text_element(n))
                        .unwrap_or(false);
                self.stack.push(Ctx::Struct {
                    close,
                    has_content: false,
                    has_block_content: false,
                    in_preformatted,
                    in_raw_text,
                });
                Ok(())
            }
            None => Err(HtmlSerializeError {
                msg: "serializer state missing context",
            }),
        }
    }

    fn end_struct(&mut self) -> Result<(), Self::Error> {
        self.elements_stack.pop();

        if let Some(Ctx::Struct {
            close,
            has_content,
            has_block_content,
            ..
        }) = self.stack.pop()
        {
            // Flush any remaining deferred tag (in case struct had only attributes or empty content)
            // Use inline mode if we never had any content
            self.flush_deferred_open_tag_with_mode(!has_content && !has_block_content);

            if let Some(name) = close
                && !is_void_element(&name)
            {
                // Check if this element is inline - inline elements shouldn't have
                // newlines added around them
                let is_inline = is_inline_element(&name);

                // Check if we're in a block context by looking at ancestor structs.
                // A Seq (elements list) itself doesn't determine block-ness - we need
                // to find the nearest Struct ancestor and check its has_block_content.
                let parent_is_block = self.stack.iter().rev().any(|ctx| {
                    matches!(
                        ctx,
                        Ctx::Struct {
                            has_block_content: true,
                            ..
                        }
                    )
                });

                // If we had block content, indent before closing tag
                // Only add newline after if we had block content or parent has block content,
                // AND this element is not inline
                let newline_after = (has_block_content || parent_is_block) && !is_inline;
                self.write_close_tag_ex(&name, has_block_content, newline_after);
            }
        }
        Ok(())
    }

    fn begin_seq(&mut self) -> Result<(), Self::Error> {
        // If this is an elements list, DON'T flush the deferred tag yet.
        // Wait until we have actual items to determine if we have block content.
        if self.pending_is_elements {
            self.pending_is_elements = false;
            self.elements_stack.push(true);
            self.pending_field.take(); // Consume the field name
            // Propagate preformatted and raw text context from parent
            let in_preformatted = self.in_preformatted();
            let in_raw_text = self.in_raw_text();
            self.stack.push(Ctx::Seq {
                close: None,
                in_preformatted,
                in_raw_text,
            });
            return Ok(());
        }

        // For non-elements sequences, flush normally
        self.flush_deferred_open_tag();
        self.ensure_root_tag_written();

        // Mark parent struct as having block content (sequences are block content)
        if let Some(Ctx::Struct {
            has_content,
            has_block_content,
            ..
        }) = self.stack.last_mut()
        {
            *has_content = true;
            *has_block_content = true;
        }

        // Propagate preformatted and raw text context from parent
        let parent_preformatted = self.in_preformatted();
        let parent_raw_text = self.in_raw_text();
        let close = if let Some(field_name) = self.pending_field.take() {
            self.write_open_tag(&field_name);
            self.write_newline();
            self.depth += 1;
            Some(field_name)
        } else {
            None
        };
        let in_preformatted = parent_preformatted
            || close
                .as_ref()
                .map(|n| is_whitespace_sensitive(n))
                .unwrap_or(false);
        let in_raw_text = parent_raw_text
            || close
                .as_ref()
                .map(|n| is_raw_text_element(n))
                .unwrap_or(false);
        self.elements_stack.push(false);
        self.stack.push(Ctx::Seq {
            close,
            in_preformatted,
            in_raw_text,
        });
        Ok(())
    }

    fn end_seq(&mut self) -> Result<(), Self::Error> {
        self.elements_stack.pop();
        if let Some(Ctx::Seq { close, .. }) = self.stack.pop()
            && let Some(name) = close
        {
            self.write_close_tag(&name, true);
        }
        Ok(())
    }

    fn field_key(&mut self, key: &str) -> Result<(), Self::Error> {
        // If we're skipping the enum wrapper, check if this is the variant name field_key
        // that we should skip (variant_metadata already set up pending_field)
        if let Some(ref variant_name) = self.skip_enum_wrapper
            && key == variant_name
        {
            // Clear the skip flag - the wrapper struct's field_key is now consumed
            // The next begin_struct will be the actual content struct
            self.skip_enum_wrapper = None;
            return Ok(());
        }
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
            ScalarValue::Char(c) => {
                let mut buf = [0u8; 4];
                self.write_scalar_string(c.encode_utf8(&mut buf))
            }
            ScalarValue::I64(v) => self.write_scalar_string(&v.to_string()),
            ScalarValue::U64(v) => self.write_scalar_string(&v.to_string()),
            ScalarValue::F64(v) => {
                let s = self.format_float(v);
                self.write_scalar_string(&s)
            }
            ScalarValue::Str(s) | ScalarValue::StringlyTyped(s) => self.write_scalar_string(&s),
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

/// Check if an element is whitespace-sensitive (preformatted content).
fn is_whitespace_sensitive(name: &str) -> bool {
    WHITESPACE_SENSITIVE_ELEMENTS
        .iter()
        .any(|&v| v.eq_ignore_ascii_case(name))
}

/// Check if an element is a raw text element (content should not be HTML-escaped).
fn is_raw_text_element(name: &str) -> bool {
    RAW_TEXT_ELEMENTS
        .iter()
        .any(|&v| v.eq_ignore_ascii_case(name))
}

/// Check if an element is an inline/phrasing element (should not cause block formatting).
fn is_inline_element(name: &str) -> bool {
    INLINE_ELEMENTS
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
    use facet_xml as xml;

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
        // Text-only elements should be inline (no newlines)
        let div = SimpleDiv {
            class: Some("test".into()),
            id: None,
            text: "Content".into(),
        };

        let html = to_string_pretty(&div).unwrap();
        assert_eq!(
            html, "<div class=\"test\">Content</div>",
            "Text-only elements should be inline"
        );
    }

    #[test]
    fn test_pretty_print_nested() {
        // Nested elements should have newlines and indentation
        let container = Container {
            class: Some("outer".into()),
            children: vec![
                Child::P(Paragraph {
                    text: "First".into(),
                }),
                Child::P(Paragraph {
                    text: "Second".into(),
                }),
            ],
        };

        let html = to_string_pretty(&container).unwrap();
        assert!(
            html.contains('\n'),
            "Expected newlines in pretty output: {}",
            html
        );
        assert!(
            html.contains("  <p>"),
            "Expected indented child elements: {}",
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

    /// Test nested elements using xml::elements with enum variants
    #[derive(Debug, Facet)]
    #[facet(rename = "div")]
    struct Container {
        #[facet(xml::attribute, default)]
        class: Option<String>,
        #[facet(xml::elements, default)]
        children: Vec<Child>,
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum Child {
        #[facet(rename = "p")]
        P(#[expect(dead_code)] Paragraph),
        #[facet(rename = "span")]
        Span(#[expect(dead_code)] Span),
    }

    #[derive(Debug, Facet)]
    struct Paragraph {
        #[facet(xml::text, default)]
        text: String,
    }

    #[derive(Debug, Facet)]
    struct Span {
        #[facet(xml::attribute, default)]
        class: Option<String>,
        #[facet(xml::text, default)]
        text: String,
    }

    #[test]
    fn test_nested_elements_with_enums() {
        let container = Container {
            class: Some("wrapper".into()),
            children: vec![
                Child::P(Paragraph {
                    text: "Hello".into(),
                }),
                Child::Span(Span {
                    class: Some("highlight".into()),
                    text: "World".into(),
                }),
            ],
        };

        let html = to_string(&container).unwrap();
        let expected =
            r#"<div class="wrapper"><p>Hello</p><span class="highlight">World</span></div>"#;
        assert_eq!(html, expected);
    }

    #[test]
    fn test_nested_elements_pretty_print() {
        let container = Container {
            class: Some("wrapper".into()),
            children: vec![
                Child::P(Paragraph {
                    text: "Hello".into(),
                }),
                Child::Span(Span {
                    class: Some("highlight".into()),
                    text: "World".into(),
                }),
            ],
        };

        let html = to_string_pretty(&container).unwrap();
        // Note: trailing newline is expected for pretty output
        let expected = "<div class=\"wrapper\">\n  <p>Hello</p>\n  <span class=\"highlight\">World</span>\n</div>\n";
        assert_eq!(html, expected);
    }

    #[test]
    fn test_empty_container() {
        let container = Container {
            class: Some("empty".into()),
            children: vec![],
        };

        let html = to_string(&container).unwrap();
        assert_eq!(html, r#"<div class="empty"></div>"#);

        let html_pretty = to_string_pretty(&container).unwrap();
        // Empty container should still be inline since no block content
        assert_eq!(html_pretty, r#"<div class="empty"></div>"#);
    }

    #[test]
    fn test_deeply_nested() {
        // Container with a span that has its own nested content
        #[derive(Debug, Facet)]
        #[facet(rename = "article")]
        struct Article {
            #[facet(xml::elements, default)]
            sections: Vec<Section>,
        }

        #[derive(Debug, Facet)]
        #[facet(rename = "section")]
        struct Section {
            #[facet(xml::attribute, default)]
            id: Option<String>,
            #[facet(xml::elements, default)]
            paragraphs: Vec<Para>,
        }

        #[derive(Debug, Facet)]
        #[facet(rename = "p")]
        struct Para {
            #[facet(xml::text, default)]
            text: String,
        }

        let article = Article {
            sections: vec![Section {
                id: Some("intro".into()),
                paragraphs: vec![
                    Para {
                        text: "First para".into(),
                    },
                    Para {
                        text: "Second para".into(),
                    },
                ],
            }],
        };

        let html = to_string(&article).unwrap();
        assert_eq!(
            html,
            r#"<article><section id="intro"><p>First para</p><p>Second para</p></section></article>"#
        );

        let html_pretty = to_string_pretty(&article).unwrap();
        assert_eq!(
            html_pretty,
            "<article>\n  <section id=\"intro\">\n    <p>First para</p>\n    <p>Second para</p>\n  </section>\n</article>\n"
        );
    }

    #[test]
    fn test_event_handlers() {
        use facet_html_dom::{Button, GlobalAttrs};

        let button = Button {
            attrs: GlobalAttrs {
                onclick: Some("handleClick()".into()),
                onmouseover: Some("highlight(this)".into()),
                ..Default::default()
            },
            type_: Some("button".into()),
            children: vec![facet_html_dom::PhrasingContent::Text("Click me".into())],
            ..Default::default()
        };

        let html = to_string(&button).unwrap();
        assert!(
            html.contains(r#"onclick="handleClick()""#),
            "Expected onclick handler, got: {}",
            html
        );
        assert!(
            html.contains(r#"onmouseover="highlight(this)""#),
            "Expected onmouseover handler, got: {}",
            html
        );
        assert!(
            html.contains("Click me"),
            "Expected button text, got: {}",
            html
        );
    }

    #[test]
    fn test_event_handlers_with_escaping() {
        use facet_html_dom::{Div, FlowContent, GlobalAttrs};

        let div = Div {
            attrs: GlobalAttrs {
                onclick: Some(r#"alert("Hello \"World\"")"#.into()),
                ..Default::default()
            },
            children: vec![FlowContent::Text("Test".into())],
        };

        let html = to_string(&div).unwrap();
        // The quotes inside the onclick value should be escaped
        assert!(
            html.contains("onclick="),
            "Expected onclick attr, got: {}",
            html
        );
        assert!(
            html.contains("&quot;"),
            "Expected escaped quotes in onclick, got: {}",
            html
        );
    }

    #[test]
    fn test_doctype_roundtrip() {
        use crate::parser::HtmlParser;
        use facet_format::FormatDeserializer;
        use facet_html_dom::Html;

        // Parse HTML with DOCTYPE
        let input = br#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body></body>
</html>"#;

        let parser = HtmlParser::new(input);
        let mut deserializer = FormatDeserializer::new(parser);
        let parsed: Html = deserializer.deserialize().unwrap();

        // Verify DOCTYPE was captured
        assert_eq!(
            parsed.doctype,
            Some("html".to_string()),
            "DOCTYPE should be captured during parsing"
        );

        // Serialize back to HTML
        let output = to_string(&parsed).unwrap();

        // Verify DOCTYPE is present in output
        assert!(
            output.starts_with("<!DOCTYPE html>"),
            "Output should start with DOCTYPE declaration, got: {}",
            output
        );

        // Parse the output again to verify roundtrip
        let parser2 = HtmlParser::new(output.as_bytes());
        let mut deserializer2 = FormatDeserializer::new(parser2);
        let reparsed: Html = deserializer2.deserialize().unwrap();

        assert_eq!(
            reparsed.doctype,
            Some("html".to_string()),
            "DOCTYPE should survive roundtrip"
        );
    }
}
