//! SVG style attribute parsing and structured representation.

use facet::Facet;
use lightningcss::declaration::DeclarationBlock;
use lightningcss::printer::PrinterOptions;
use lightningcss::stylesheet::ParserOptions;

/// A color value
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    /// No color (transparent)
    None,
    /// RGB color
    Rgb { r: u8, g: u8, b: u8 },
    /// Named color
    Named(String),
}

impl Color {
    /// Parse a color from a string
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") {
            return Color::None;
        }

        // Try parsing rgb(r,g,b)
        if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 3 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].trim().parse::<u8>(),
                    parts[1].trim().parse::<u8>(),
                    parts[2].trim().parse::<u8>(),
                ) {
                    return Color::Rgb { r, g, b };
                }
            }
        }

        // Try parsing hex color
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return Color::Rgb { r, g, b };
                }
            } else if hex.len() == 3 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..1], 16),
                    u8::from_str_radix(&hex[1..2], 16),
                    u8::from_str_radix(&hex[2..3], 16),
                ) {
                    // Expand 3-digit hex: #abc -> #aabbcc
                    return Color::Rgb {
                        r: r * 17,
                        g: g * 17,
                        b: b * 17,
                    };
                }
            }
        }

        // Named color - normalize common names to RGB
        match s.to_lowercase().as_str() {
            "black" => Color::Rgb { r: 0, g: 0, b: 0 },
            "white" => Color::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            "red" => Color::Rgb { r: 255, g: 0, b: 0 },
            "green" => Color::Rgb { r: 0, g: 128, b: 0 },
            "blue" => Color::Rgb { r: 0, g: 0, b: 255 },
            "yellow" => Color::Rgb {
                r: 255,
                g: 255,
                b: 0,
            },
            "cyan" => Color::Rgb {
                r: 0,
                g: 255,
                b: 255,
            },
            "magenta" => Color::Rgb {
                r: 255,
                g: 0,
                b: 255,
            },
            "gray" | "grey" => Color::Rgb {
                r: 128,
                g: 128,
                b: 128,
            },
            _ => Color::Named(s.to_string()),
        }
    }

    /// Serialize to string in rgb() format like C pikchr
    pub fn to_string(&self) -> String {
        match self {
            Color::None => "none".to_string(),
            Color::Rgb { r, g, b } => format!("rgb({},{},{})", r, g, b),
            Color::Named(n) => n.clone(),
        }
    }
}

/// Structured SVG style attribute with lightningcss AST
/// We store the serialized CSS string to avoid lifetime issues with DeclarationBlock
#[derive(Facet, Debug, Clone, PartialEq, Default)]
#[facet(traits(Default))]
pub struct SvgStyle {
    // Store as String to avoid lifetime complications
    // Parse on-demand when needed
    css: String,
}

impl SvgStyle {
    /// Get the parsed declarations (owned, for manipulation)
    /// Note: Returns an owned DeclarationBlock with 'static lifetime
    pub fn declarations(&self) -> Result<DeclarationBlock<'static>, StyleParseError> {
        if self.css.is_empty() {
            return Ok(DeclarationBlock::default());
        }
        let parser_options = ParserOptions::default();
        // Parse from owned string to get 'static lifetime
        DeclarationBlock::parse_string(self.css.clone().leak(), parser_options)
            .map_err(|e| StyleParseError::ParseError(format!("{:?}", e)))
    }

    /// Update from a DeclarationBlock
    pub fn set_declarations(&mut self, declarations: DeclarationBlock<'_>) {
        self.css = Self::serialize_declarations(&declarations);
    }

    fn serialize_declarations(declarations: &DeclarationBlock<'_>) -> String {
        if declarations.is_empty() {
            String::new()
        } else {
            let mut dest = String::new();
            for (i, decl) in declarations.declarations.iter().enumerate() {
                if i > 0 {
                    dest.push(';');
                }
                let printer_options = PrinterOptions::default();
                if let Ok(css) = decl.to_css_string(false, printer_options) {
                    dest.push_str(&css);
                }
            }
            if !dest.is_empty() {
                dest.push(';');
            }
            dest
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// Parse style from a CSS-like string using lightningcss
    pub fn parse(s: &str) -> Result<Self, StyleParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Ok(SvgStyle::default());
        }

        // Validate by parsing with lightningcss
        let parser_options = ParserOptions::default();
        let declarations = DeclarationBlock::parse_string(s, parser_options)
            .map_err(|e| StyleParseError::ParseError(format!("{:?}", e)))?;

        // Normalize by serializing back
        Ok(SvgStyle {
            css: Self::serialize_declarations(&declarations),
        })
    }

    /// Serialize to CSS string
    pub fn to_string(&self) -> String {
        self.css.clone()
    }

    /// Check if the style has no declarations
    pub fn is_empty(&self) -> bool {
        self.css.is_empty()
    }
}

/// Error parsing style
#[derive(Debug, Clone, PartialEq)]
pub enum StyleParseError {
    MalformedDeclaration(String),
    ParseError(String),
    SerializeError(String),
}

impl std::fmt::Display for StyleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleParseError::MalformedDeclaration(decl) => {
                write!(f, "malformed style declaration: {}", decl)
            }
            StyleParseError::ParseError(err) => {
                write!(f, "CSS parse error: {}", err)
            }
            StyleParseError::SerializeError(err) => {
                write!(f, "CSS serialize error: {}", err)
            }
        }
    }
}

impl std::error::Error for StyleParseError {}

/// Proxy type for SvgStyle - serializes as a string
#[derive(Facet, Clone, Debug)]
#[facet(transparent)]
pub struct SvgStyleProxy(pub String);

impl TryFrom<SvgStyleProxy> for SvgStyle {
    type Error = StyleParseError;
    fn try_from(proxy: SvgStyleProxy) -> Result<Self, Self::Error> {
        if proxy.0.is_empty() {
            Ok(SvgStyle::default())
        } else {
            SvgStyle::parse(&proxy.0)
        }
    }
}

impl TryFrom<&SvgStyle> for SvgStyleProxy {
    type Error = std::convert::Infallible;
    fn try_from(v: &SvgStyle) -> Result<Self, Self::Error> {
        Ok(SvgStyleProxy(v.to_string()))
    }
}

/// Helper function for skip_serializing_if with SvgStyle
pub fn is_empty_style(s: &SvgStyle) -> bool {
    s.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_rgb() {
        assert_eq!(Color::parse("rgb(0,0,0)"), Color::Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            Color::parse("rgb(255,128,64)"),
            Color::Rgb {
                r: 255,
                g: 128,
                b: 64
            }
        );
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(Color::parse("black"), Color::Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(Color::parse("none"), Color::None);
    }

    #[test]
    fn test_parse_style() {
        let style = SvgStyle::parse("fill:none;stroke-width:2.16;stroke:rgb(0,0,0);").unwrap();
        assert!(!style.is_empty());

        let decls = style.declarations().unwrap();
        assert_eq!(decls.declarations.len(), 3);

        // Verify the serialized output includes normalized CSS
        let serialized = style.to_string();
        assert!(serialized.contains("fill"));
        assert!(serialized.contains("stroke-width"));
        assert!(serialized.contains("stroke"));
    }

    #[test]
    fn test_style_roundtrip() {
        let original = "fill:none;stroke-width:2.16;stroke:rgb(0,0,0);";
        let style = SvgStyle::parse(original).unwrap();
        let serialized = style.to_string();
        let reparsed = SvgStyle::parse(&serialized).unwrap();
        assert_eq!(style, reparsed);
    }

    #[test]
    fn test_style_parsing_order() {
        let input = "stroke:rgb(0,0,0);fill:none;stroke-width:2.16";
        let style = SvgStyle::parse(input).unwrap();
        let serialized = style.to_string();

        // lightningcss normalizes colors and adds units
        assert!(serialized.contains("stroke"));
        assert!(serialized.contains("fill"));
        assert!(serialized.contains("stroke-width"));
    }

    #[test]
    fn test_color_normalization() {
        // "black" and "rgb(0,0,0)" should compare equal
        let c1 = Color::parse("black");
        let c2 = Color::parse("rgb(0,0,0)");
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_css_manipulation() {
        use lightningcss::properties::Property;
        use lightningcss::properties::svg::SVGPaint;
        use lightningcss::traits::Parse;
        use lightningcss::values::color::CssColor;

        // Parse initial CSS
        let mut style = SvgStyle::parse("fill: red; stroke-width: 2px;").unwrap();
        let mut decls = style.declarations().unwrap();
        assert_eq!(decls.declarations.len(), 2);

        // Add a new property by modifying the DeclarationBlock
        let new_color = CssColor::parse_string("blue").unwrap();
        let svg_paint = SVGPaint::Color(new_color);
        decls.declarations.push(Property::Stroke(svg_paint));

        // Update the style with modified declarations
        style.set_declarations(decls);

        // Verify we now have 3 declarations
        let updated_decls = style.declarations().unwrap();
        assert_eq!(updated_decls.declarations.len(), 3);

        // Serialize and verify all properties are present
        let serialized = style.to_string();
        assert!(serialized.contains("fill"));
        assert!(serialized.contains("stroke-width"));
        assert!(serialized.contains("stroke"));
    }

    #[test]
    fn test_css_property_removal() {
        use lightningcss::properties::PropertyId;

        // Parse CSS with multiple properties
        let mut style = SvgStyle::parse("fill: red; stroke: blue; stroke-width: 2px;").unwrap();
        let mut decls = style.declarations().unwrap();
        assert_eq!(decls.declarations.len(), 3);

        // Remove the stroke property
        decls
            .declarations
            .retain(|prop| prop.property_id() != PropertyId::Stroke);

        // Update the style
        style.set_declarations(decls);

        // Verify we now have 2 declarations
        let updated_decls = style.declarations().unwrap();
        assert_eq!(updated_decls.declarations.len(), 2);

        // Serialize and verify stroke is gone
        let serialized = style.to_string();
        assert!(serialized.contains("fill"));
        assert!(!serialized.contains("stroke:") && !serialized.contains("stroke "));
        assert!(serialized.contains("stroke-width"));
    }

    #[test]
    fn test_complex_css_parsing() {
        // Test that lightningcss handles complex CSS that naive splitting can't
        let complex_css = r#"fill: url(#gradient); stroke: rgb(255, 128, 64); opacity: 0.5;"#;
        let style = SvgStyle::parse(complex_css).unwrap();

        // Should successfully parse all properties
        let decls = style.declarations().unwrap();
        assert_eq!(decls.declarations.len(), 3);

        // Should roundtrip correctly
        let serialized = style.to_string();
        let reparsed = SvgStyle::parse(&serialized).unwrap();
        assert_eq!(style, reparsed);
    }

    #[test]
    fn test_empty_style() {
        let empty = SvgStyle::default();
        assert!(empty.is_empty());
        assert_eq!(empty.to_string(), "");

        let parsed_empty = SvgStyle::parse("").unwrap();
        assert!(parsed_empty.is_empty());
        assert_eq!(parsed_empty.to_string(), "");
    }

    #[test]
    fn test_css_with_semicolons_in_values() {
        // This is where naive semicolon splitting would fail
        // data URLs can contain semicolons
        let css_with_data_url = r#"background-image: url("data:image/svg+xml;base64,...");"#;
        let style = SvgStyle::parse(css_with_data_url).unwrap();

        // Should parse as a single declaration
        let decls = style.declarations().unwrap();
        assert_eq!(decls.declarations.len(), 1);
    }
}
