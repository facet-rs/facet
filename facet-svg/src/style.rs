//! SVG style attribute parsing and structured representation.

use facet::Facet;
use std::collections::BTreeMap;

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

/// Structured SVG style attribute with BTreeMap for automatic sorting
#[derive(Facet, Debug, Clone, PartialEq, Default)]
#[facet(traits(Default))]
pub struct SvgStyle {
    pub properties: BTreeMap<String, String>,
}

impl SvgStyle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a property to the style (builder pattern)
    pub fn add(mut self, key: &str, value: &str) -> Self {
        self.properties.insert(key.to_string(), value.to_string());
        self
    }

    /// Parse style from a CSS-like string into BTreeMap with automatic sorting
    pub fn parse(s: &str) -> Result<Self, StyleParseError> {
        let mut properties = BTreeMap::new();

        for declaration in s.split(';') {
            let declaration = declaration.trim();
            if declaration.is_empty() {
                continue;
            }

            if let Some((key, value)) = declaration.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim().to_string();
                properties.insert(key, value);
            } else {
                panic!("malformed style declaration: {}", declaration);
            }
        }

        Ok(SvgStyle { properties })
    }

    /// Serialize to CSS-like string with properties in alphabetical order
    pub fn to_string(&self) -> String {
        if self.properties.is_empty() {
            String::new()
        } else {
            let declarations: Vec<String> = self
                .properties
                .iter()
                .map(|(key, value)| format!("{}: {}", key, value))
                .collect();
            format!("{};", declarations.join(";"))
        }
    }

    /// Add a property to the style (builder pattern)
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }
}

/// Error parsing style
#[derive(Debug, Clone, PartialEq)]
pub enum StyleParseError {
    MalformedDeclaration(String),
}

impl std::fmt::Display for StyleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleParseError::MalformedDeclaration(decl) => {
                write!(f, "malformed style declaration: {}", decl)
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
        assert_eq!(style.properties.get("fill"), Some(&"none".to_string()));
        assert_eq!(
            style.properties.get("stroke-width"),
            Some(&"2.16".to_string())
        );
        assert_eq!(
            style.properties.get("stroke"),
            Some(&"rgb(0,0,0)".to_string())
        );
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
    fn test_style_alphabetical_sorting() {
        let input = "stroke:rgb(0,0,0);fill:none;stroke-width:2.16";
        let style = SvgStyle::parse(input).unwrap();
        let serialized = style.to_string();
        // Should be sorted alphabetically: fill;stroke;stroke-width
        // Note: to_string adds spaces after colons
        assert_eq!(
            serialized,
            "fill: none;stroke: rgb(0,0,0);stroke-width: 2.16;"
        );
    }

    #[test]
    fn test_color_normalization() {
        // "black" and "rgb(0,0,0)" should compare equal
        let c1 = Color::parse("black");
        let c2 = Color::parse("rgb(0,0,0)");
        assert_eq!(c1, c2);
    }
}
