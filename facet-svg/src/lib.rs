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
//! let svg: Svg = facet_xml::from_str(svg_str).unwrap();
//! ```

use facet::Facet;
use facet_xml as xml;

mod path;
mod style;

pub use path::{PathCommand, PathData, PathDataProxy};
pub use style::{Color, SvgStyle, SvgStyleProxy};

/// SVG namespace URI
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Root SVG element
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename = "svg")]
pub struct Svg {
    // Note: xmlns is handled by ns_all, not as a separate field
    #[facet(xml::attribute)]
    pub width: Option<String>,
    #[facet(xml::attribute)]
    pub height: Option<String>,
    #[facet(xml::attribute, rename = "viewBox")]
    pub view_box: Option<String>,
    #[facet(xml::elements)]
    pub children: Vec<SvgNode>,
}

/// Any SVG node we care about
#[derive(Facet, Debug, Clone)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
#[repr(u8)]
pub enum SvgNode {
    #[facet(rename = "g")]
    G(Group),
    #[facet(rename = "defs")]
    Defs(Defs),
    #[facet(rename = "style")]
    Style(Style),
    #[facet(rename = "rect")]
    Rect(Rect),
    #[facet(rename = "circle")]
    Circle(Circle),
    #[facet(rename = "ellipse")]
    Ellipse(Ellipse),
    #[facet(rename = "line")]
    Line(Line),
    #[facet(rename = "path")]
    Path(Path),
    #[facet(rename = "polygon")]
    Polygon(Polygon),
    #[facet(rename = "polyline")]
    Polyline(Polyline),
    #[facet(rename = "text")]
    Text(Text),
}

/// SVG group element (`<g>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
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
pub struct Style {
    #[facet(xml::attribute, rename = "type")]
    pub type_: Option<String>,
    #[facet(xml::text)]
    pub content: String,
}

/// SVG rect element (`<rect>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
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
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
}

/// SVG circle element (`<circle>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
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
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
}

/// SVG ellipse element (`<ellipse>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
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
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
}

/// SVG line element (`<line>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
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
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
}

/// SVG path element (`<path>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Path {
    #[facet(xml::attribute, proxy = PathDataProxy)]
    pub d: Option<PathData>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, rename = "stroke-width")]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, rename = "stroke-dasharray")]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
}

/// SVG polygon element (`<polygon>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Polygon {
    #[facet(xml::attribute)]
    pub points: Option<String>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, rename = "stroke-width")]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, rename = "stroke-dasharray")]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
}

/// SVG polyline element (`<polyline>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Polyline {
    #[facet(xml::attribute)]
    pub points: Option<String>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, rename = "stroke-width")]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, rename = "stroke-dasharray")]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
}

/// SVG text element (`<text>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Text {
    #[facet(xml::attribute)]
    pub x: Option<f64>,
    #[facet(xml::attribute)]
    pub y: Option<f64>,
    #[facet(xml::attribute)]
    pub fill: Option<String>,
    #[facet(xml::attribute)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, rename = "stroke-width")]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy)]
    pub style: SvgStyle,
    #[facet(xml::attribute, rename = "text-anchor")]
    pub text_anchor: Option<String>,
    #[facet(xml::attribute, rename = "dominant-baseline")]
    pub dominant_baseline: Option<String>,
    #[facet(xml::text)]
    pub content: String,
}

// Re-export XML utilities for convenience
pub use facet_xml;

/// Format a number like C pikchr does (%g equivalent).
/// C's %g format uses 6 significant digits by default and trims trailing zeros.
pub fn fmt_num(v: f64) -> String {
    // Use %g behavior: 6 significant digits, trim trailing zeros
    // Rust doesn't have %g, so we need to implement it manually
    if v == 0.0 {
        return "0".to_string();
    }

    // Determine magnitude to figure out how many decimal places we need
    let abs_v = v.abs();
    let log10 = abs_v.log10().floor() as i32;

    // For %g with 6 significant digits:
    // - If exponent < -4 or >= 6, use scientific notation (but C pikchr values rarely need this)
    // - Otherwise use fixed-point notation with enough decimals for 6 sig figs
    if log10 >= 6 || log10 < -4 {
        // Scientific notation case (rare in pikchr coordinates)
        let s = format!("{:.5e}", v);
        return s;
    }

    // Fixed-point: we need (6 - log10 - 1) decimal places for 6 significant digits
    // But ensure at least 0 decimal places
    let decimals = (5 - log10).max(0) as usize;
    let s = format!("{:.*}", decimals, v);

    // Trim trailing zeros and decimal point
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attributes_are_parsed() {
        let xml = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <path d="M10,10L50,50" stroke="black"/>
        </svg>"#;

        let svg: Svg = facet_xml::from_str(xml).unwrap();

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
}
