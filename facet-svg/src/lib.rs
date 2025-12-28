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
mod points;

pub use path::{PathCommand, PathData, PathDataProxy};
pub use points::{Point, Points, PointsProxy, is_empty_points};
pub use style::{Color, SvgStyle, SvgStyleProxy, is_empty_style};

/// SVG namespace URI
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Root SVG element
#[derive(Facet, Debug, Clone, Default)]
#[facet(
    xml::ns_all = "http://www.w3.org/2000/svg",
    rename = "svg",
    rename_all = "camelCase"
)]
pub struct Svg {
    // Note: xmlns is handled by ns_all, not as a separate field
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub height: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
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
    #[facet(rename = "use")]
    Use(Use),
    #[facet(rename = "image")]
    Image(Image),
    #[facet(rename = "title")]
    Title(Title),
    #[facet(rename = "desc")]
    Desc(Desc),
    #[facet(rename = "symbol")]
    Symbol(Symbol),
}

/// SVG group element (`<g>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Group {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub id: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub class: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
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
    #[facet(xml::attribute, rename = "type", default, skip_serializing_if = Option::is_none)]
    pub type_: Option<String>,
    #[facet(xml::text)]
    pub content: String,
}

/// SVG rect element (`<rect>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Rect {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub x: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub y: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub width: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub height: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub rx: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub ry: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG circle element (`<circle>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Circle {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub cx: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub cy: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub r: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG ellipse element (`<ellipse>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Ellipse {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub cx: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub cy: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub rx: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub ry: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG line element (`<line>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Line {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub x1: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub y1: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub x2: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub y2: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG path element (`<path>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Path {
    #[facet(xml::attribute, proxy = PathDataProxy, default, skip_serializing_if = Option::is_none)]
    pub d: Option<PathData>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG polygon element (`<polygon>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Polygon {
    #[facet(xml::attribute, proxy = PointsProxy, skip_serializing_if = is_empty_points)]
    pub points: Points,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG polyline element (`<polyline>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Polyline {
    #[facet(xml::attribute, proxy = PointsProxy, skip_serializing_if = is_empty_points)]
    pub points: Points,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_dasharray: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG text element (`<text>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "kebab-case")]
pub struct Text {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub x: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub y: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub transform: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub stroke_width: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub font_family: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub font_style: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub font_weight: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub font_size: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub text_anchor: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub dominant_baseline: Option<String>,
    #[facet(xml::text)]
    pub content: String,
}

/// SVG use element (`<use>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Use {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub x: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub y: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub width: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub height: Option<f64>,
    #[facet(xml::attribute, rename = "xlink:href", default, skip_serializing_if = Option::is_none)]
    pub href: Option<String>,
}

/// SVG image element (`<image>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Image {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub x: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub y: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub width: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub height: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub href: Option<String>,
    #[facet(xml::attribute, proxy = SvgStyleProxy, skip_serializing_if = is_empty_style)]
    pub style: SvgStyle,
}

/// SVG title element (`<title>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Title {
    #[facet(xml::text)]
    pub content: String,
}

/// SVG description element (`<desc>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
pub struct Desc {
    #[facet(xml::text)]
    pub content: String,
}

/// SVG symbol element (`<symbol>`)
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename_all = "camelCase")]
pub struct Symbol {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub id: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub view_box: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub width: Option<String>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
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

        let c: Svg = facet_xml::from_str(c_svg).unwrap();
        let rust: Svg = facet_xml::from_str(rust_svg).unwrap();

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
