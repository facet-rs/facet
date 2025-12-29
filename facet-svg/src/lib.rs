//! Facet-derived types for SVG parsing and serialization.
//!
//! This crate provides strongly-typed SVG elements that can be deserialized
//! from XML using `facet-xml`.
//!
//! # Example
//!
//! ```rust
//! use facet_svg::Svg;
//!
//! let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
//!     <rect x="10" y="10" width="80" height="80" fill="blue"/>
//! </svg>"#;
//!
//! let svg: Svg = facet_svg::from_str(svg_str).unwrap();
//! ```

use facet::Facet;
use facet_format::FormatDeserializer;
use facet_xml as xml;
use facet_xml::{XmlParser, to_vec};

mod path;
mod points;

pub use path::{PathCommand, PathData, PathDataProxy};
pub use points::{Point, Points, PointsProxy};

/// SVG namespace URI
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Error type for SVG parsing
pub type Error = facet_format::DeserializeError<facet_xml::XmlError>;

/// Error type for SVG serialization
pub type SerializeError = facet_format::SerializeError<facet_xml::XmlSerializeError>;

/// Deserialize an SVG from a string.
pub fn from_str<'input, T>(xml: &'input str) -> Result<T, Error>
where
    T: Facet<'input>,
{
    let parser = XmlParser::new(xml.as_bytes());
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

/// Serialize an SVG value to a string.
pub fn to_string<'facet, T>(value: &T) -> Result<String, SerializeError>
where
    T: Facet<'facet> + ?Sized,
{
    let bytes = to_vec(value)?;
    // SAFETY: XmlSerializer produces valid UTF-8
    Ok(String::from_utf8(bytes).expect("XmlSerializer produces valid UTF-8"))
}

/// Root SVG element
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename = "svg",
    rename_all = "camelCase",
    skip_all_unless_truthy
)]
pub struct Svg {
    // Note: xmlns is handled by ns_all, not as a separate field
    #[facet(xml::attribute)]
    pub width: Option<String>,
    #[facet(xml::attribute)]
    pub height: Option<String>,
    #[facet(xml::attribute)]
    pub view_box: Option<String>,
    #[facet(xml::elements)]
    pub children: Vec<SvgNode>,
}

/// Any SVG node we care about
#[derive(Facet, Debug, Clone)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "lowercase")]
#[repr(u8)]
pub enum SvgNode {
    G(Group),
    Defs(Defs),
    Style(Style),
    Rect(Rect),
    Circle(Circle),
    Ellipse(Ellipse),
    Line(Line),
    Path(Path),
    Polygon(Polygon),
    Polyline(Polyline),
    Text(Text),
    Use(Use),
    Image(Image),
    Title(Title),
    Desc(Desc),
    Symbol(Symbol),
}

/// SVG group element (`<g>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", skip_all_unless_truthy)]
pub struct Group {
    #[facet(xml::attribute)]
    pub id: Option<String>,
    #[facet(xml::attribute)]
    pub class: Option<String>,
    #[facet(xml::attribute)]
    pub transform: Option<String>,
    #[facet(xml::elements)]
    pub children: Vec<SvgNode>,
}

/// SVG defs element (`<defs>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Defs {
    #[facet(xml::elements)]
    pub children: Vec<SvgNode>,
}

/// SVG style element (`<style>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(skip_all_unless_truthy)]
pub struct Style {
    #[facet(xml::attribute, rename = "type")]
    pub type_: Option<String>,
    #[facet(xml::text)]
    pub content: Option<String>,
}

/// SVG rect element (`<rect>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Rect {
    #[facet(xml::attribute)]
    pub x: Option<f64>,
    #[facet(xml::attribute)]
    pub y: Option<f64>,
    #[facet(xml::attribute)]
    pub width: Option<f64>,
    #[facet(xml::attribute)]
    pub height: Option<f64>,
    #[facet(xml::attribute)]
    pub rx: Option<f64>,
    #[facet(xml::attribute)]
    pub ry: Option<f64>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG circle element (`<circle>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Circle {
    #[facet(xml::attribute)]
    pub cx: Option<f64>,
    #[facet(xml::attribute)]
    pub cy: Option<f64>,
    #[facet(xml::attribute)]
    pub r: Option<f64>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG ellipse element (`<ellipse>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Ellipse {
    #[facet(xml::attribute)]
    pub cx: Option<f64>,
    #[facet(xml::attribute)]
    pub cy: Option<f64>,
    #[facet(xml::attribute)]
    pub rx: Option<f64>,
    #[facet(xml::attribute)]
    pub ry: Option<f64>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG line element (`<line>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Line {
    #[facet(xml::attribute)]
    pub x1: Option<f64>,
    #[facet(xml::attribute)]
    pub y1: Option<f64>,
    #[facet(xml::attribute)]
    pub x2: Option<f64>,
    #[facet(xml::attribute)]
    pub y2: Option<f64>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG path element (`<path>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Path {
    #[facet(xml::attribute, proxy = PathDataProxy)]
    pub d: Option<PathData>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG polygon element (`<polygon>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Polygon {
    #[facet(xml::attribute, proxy = PointsProxy)]
    pub points: Points,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG polyline element (`<polyline>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Polyline {
    #[facet(xml::attribute, proxy = PointsProxy)]
    pub points: Points,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG text element (`<text>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "kebab-case",
    skip_all_unless_truthy
)]
pub struct Text {
    #[facet(xml::attribute)]
    pub x: Option<f64>,
    #[facet(xml::attribute)]
    pub y: Option<f64>,
    #[facet(xml::attribute)]
    pub transform: Option<String>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
    #[facet(xml::attribute)]
    pub font_family: Option<String>,
    #[facet(xml::attribute)]
    pub font_style: Option<String>,
    #[facet(xml::attribute)]
    pub font_weight: Option<String>,
    #[facet(xml::attribute)]
    pub font_size: Option<String>,
    #[facet(xml::attribute)]
    pub text_anchor: Option<String>,
    #[facet(xml::attribute)]
    pub dominant_baseline: Option<String>,
    #[facet(xml::text)]
    pub content: Option<String>,
}

/// SVG use element (`<use>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", skip_all_unless_truthy)]
pub struct Use {
    #[facet(xml::attribute)]
    pub x: Option<f64>,
    #[facet(xml::attribute)]
    pub y: Option<f64>,
    #[facet(xml::attribute)]
    pub width: Option<f64>,
    #[facet(xml::attribute)]
    pub height: Option<f64>,
    #[facet(xml::attribute, rename = "xlink:href")]
    pub href: Option<String>,
}

/// SVG image element (`<image>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", skip_all_unless_truthy)]
pub struct Image {
    #[facet(xml::attribute)]
    pub x: Option<f64>,
    #[facet(xml::attribute)]
    pub y: Option<f64>,
    #[facet(xml::attribute)]
    pub width: Option<f64>,
    #[facet(xml::attribute)]
    pub height: Option<f64>,
    #[facet(xml::attribute)]
    pub href: Option<String>,
    #[facet(xml::attribute)]
    pub style: Option<String>,
}

/// SVG title element (`<title>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Title {
    #[facet(xml::text)]
    pub content: Option<String>,
}

/// SVG description element (`<desc>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Desc {
    #[facet(xml::text)]
    pub content: Option<String>,
}

/// SVG symbol element (`<symbol>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename_all = "camelCase",
    skip_all_unless_truthy
)]
pub struct Symbol {
    #[facet(xml::attribute)]
    pub id: Option<String>,
    #[facet(xml::attribute)]
    pub view_box: Option<String>,
    #[facet(xml::attribute)]
    pub width: Option<String>,
    #[facet(xml::attribute)]
    pub height: Option<String>,
    #[facet(xml::elements)]
    pub children: Vec<SvgNode>,
}

// Re-export XML utilities for convenience
pub use facet_xml;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attributes_are_parsed() {
        let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <path d="M10,10L50,50" stroke="black"/>
        </svg>"#;

        let svg: Svg = from_str(xml).unwrap();

        println!("Parsed SVG: {:?}", svg);

        // These should NOT be None!
        assert!(svg.view_box.is_some(), "viewBox should be parsed");

        if let Some(SvgNode::Path(path)) = svg.children.first() {
            println!("Parsed Path: {:?}", path);
            assert!(path.d.is_some(), "path d attribute should be parsed");
            assert!(
                path.stroke.is_some(),
                "path stroke attribute should be parsed"
            );
        } else {
            panic!("Expected a Path element");
        }
    }

    #[test]
    fn test_svg_float_tolerance() {
        use facet_assert::{SameOptions, SameReport, check_same_with_report};

        // Simulate C vs Rust precision - C has 3 decimals, Rust has 10
        // (Without style difference to isolate float tolerance test)
        let c_svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <path d="M118.239,208.239L226.239,208.239Z"/>
        </svg>"#;

        let rust_svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <path d="M118.2387401575,208.2387401575L226.2387401575,208.2387401575Z"/>
        </svg>"#;

        let c: Svg = from_str(c_svg).unwrap();
        let rust: Svg = from_str(rust_svg).unwrap();

        eprintln!("C SVG: {:?}", c);
        eprintln!("Rust SVG: {:?}", rust);

        let tolerance = 0.002;
        let options = SameOptions::new().float_tolerance(tolerance);

        let result = check_same_with_report(&c, &rust, options);

        match &result {
            SameReport::Same => eprintln!("Result: Same"),
            SameReport::Different(report) => {
                eprintln!("Result: Different");
                eprintln!("XML diff:\n{}", report.render_ansi_xml());
            }
            SameReport::Opaque { type_name } => eprintln!("Result: Opaque({})", type_name),
        }

        assert!(
            matches!(result, SameReport::Same),
            "SVG values within float tolerance should be considered Same"
        );
    }
}
