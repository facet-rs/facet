use facet_svg::Svg;

// Basic SVG Element Tests

#[test]
fn test_parse_simple_circle() {
    let svg_str = include_str!("fixtures/basic/circle.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse circle SVG");

    assert_eq!(svg.width, Some("100".to_string()));
    assert_eq!(svg.height, Some("100".to_string()));
    assert_eq!(svg.view_box, Some("0 0 100 100".to_string()));
    assert_eq!(svg.children.len(), 1);
}

#[test]
fn test_parse_multiple_shapes() {
    let svg_str = include_str!("fixtures/basic/multiple_shapes.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse multi-shape SVG");

    assert_eq!(svg.children.len(), 3);
}

#[test]
fn test_parse_grouped_elements() {
    let svg_str = include_str!("fixtures/basic/grouped_elements.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse grouped SVG");

    assert_eq!(svg.children.len(), 1);
}

#[test]
fn test_parse_text_elements() {
    let svg_str = include_str!("fixtures/basic/text.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse text SVG");

    assert_eq!(svg.children.len(), 1);
}

#[test]
fn test_parse_path_element() {
    let svg_str = include_str!("fixtures/basic/path.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse path SVG");

    assert_eq!(svg.children.len(), 1);
}

#[test]
fn test_parse_style_element() {
    let svg_str = include_str!("fixtures/basic/style.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse style SVG");

    assert!(!svg.children.is_empty());
}

#[test]
fn test_parse_defs_element() {
    let svg_str = include_str!("fixtures/basic/defs.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse defs SVG");

    assert!(!svg.children.is_empty());
}

#[test]
fn test_parse_polygon_and_polyline() {
    let svg_str = include_str!("fixtures/basic/polygon_polyline.svg");
    let svg: Svg =
        facet_svg::from_str(svg_str).expect("Failed to parse polygon/polyline SVG");

    assert_eq!(svg.children.len(), 2);
}

#[test]
fn test_parse_ellipse() {
    let svg_str = include_str!("fixtures/basic/ellipse.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse ellipse SVG");

    assert_eq!(svg.children.len(), 1);
}

#[test]
fn test_parse_minimal_svg() {
    let svg_str = include_str!("fixtures/basic/minimal.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse minimal SVG");

    assert_eq!(svg.children.len(), 1);
}

#[test]
fn test_parse_dashed_lines() {
    let svg_str = include_str!("fixtures/basic/dashed_lines.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse dashed SVG");

    assert_eq!(svg.children.len(), 2);
}

#[test]
fn test_parse_nested_groups() {
    let svg_str = include_str!("fixtures/basic/nested_groups.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse nested groups SVG");

    assert_eq!(svg.children.len(), 1);
}

// Pikchr-style Diagram Tests

#[test]
fn test_parse_pikchr_style_diagram() {
    let svg_str = include_str!("fixtures/pikchr/diagram.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse pikchr-style SVG");

    // Should have the main group
    assert!(!svg.children.is_empty());
}

// Advanced SVG Element Tests

#[test]
fn test_bootstrap_icon() {
    let svg_str = include_str!("fixtures/advanced/bootstrap_icon.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse Bootstrap icon SVG");
    assert_eq!(svg.width, Some("16".to_string()));
    assert_eq!(svg.height, Some("16".to_string()));
}

#[test]
fn test_feather_icon_style() {
    let svg_str = include_str!("fixtures/advanced/feather_icon.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse Feather icon SVG");
    assert_eq!(svg.children.len(), 2); // circle and path
}

#[test]
fn test_svg_with_use_element() {
    let svg_str = include_str!("fixtures/advanced/use_element.svg");
    match facet_svg::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("Use element parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("Use element failed to parse: {}", e);
        }
    }
}

#[test]
fn test_svg_with_filters() {
    let svg_str = include_str!("fixtures/advanced/filters.svg");
    match facet_svg::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("Filter elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("Filter elements failed: {}", e);
        }
    }
}

#[test]
fn test_svg_with_tspan() {
    let svg_str = include_str!("fixtures/advanced/tspan.svg");
    match facet_svg::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("Tspan elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("Tspan elements failed: {}", e);
        }
    }
}

#[test]
fn test_svg_with_image_element() {
    let svg_str = include_str!("fixtures/advanced/image.svg");
    match facet_svg::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("Image elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("Image elements failed: {}", e);
        }
    }
}

#[test]
fn test_svg_with_marker() {
    let svg_str = include_str!("fixtures/advanced/marker.svg");
    match facet_svg::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("Marker elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("Marker elements failed: {}", e);
        }
    }
}

#[test]
fn test_svg_with_transform() {
    let svg_str = include_str!("fixtures/advanced/transform.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse transform SVG");
    assert_eq!(svg.children.len(), 1); // one group
}

#[test]
fn test_svg_with_metadata() {
    let svg_str = include_str!("fixtures/advanced/metadata.svg");
    match facet_svg::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("Metadata elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("Metadata elements failed: {}", e);
        }
    }
}

#[test]
fn test_svg_with_aspect_ratio() {
    let svg_str = include_str!("fixtures/advanced/aspect_ratio.svg");
    match facet_svg::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            assert_eq!(svg.view_box, Some("0 0 100 100".to_string()));
            println!("Successfully parsed viewBox and preserveAspectRatio");
        }
        Err(e) => {
            println!("Failed to parse aspectRatio SVG: {}", e);
        }
    }
}

#[test]
fn test_svg_with_opacity() {
    let svg_str = include_str!("fixtures/advanced/opacity.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse opacity SVG");
    assert_eq!(svg.children.len(), 1);
}

#[test]
fn test_svg_use_element_supported() {
    let svg_str = include_str!("fixtures/advanced/use_with_href.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse use element SVG");
    assert_eq!(svg.children.len(), 2); // defs and use
    if let Some(facet_svg::SvgNode::Use(use_elem)) = svg.children.last() {
        assert_eq!(use_elem.x, Some(50.0));
        assert_eq!(use_elem.y, Some(50.0));
        println!(
            "Use element fully parsed with x={:?}, y={:?}",
            use_elem.x, use_elem.y
        );
    }
}

#[test]
fn test_svg_image_element_supported() {
    let svg_str = include_str!("fixtures/advanced/image_embedded.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse image element SVG");
    assert_eq!(svg.children.len(), 1);
    if let Some(facet_svg::SvgNode::Image(img)) = svg.children.first() {
        assert_eq!(img.x, Some(10.0));
        assert_eq!(img.y, Some(10.0));
        assert_eq!(img.width, Some(100.0));
        assert_eq!(img.height, Some(100.0));
        println!("Image element fully parsed with dimensions");
    }
}

#[test]
fn test_svg_title_element_supported() {
    let svg_str = include_str!("fixtures/advanced/title.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse title element SVG");
    assert_eq!(svg.children.len(), 2); // title and rect
    if let Some(facet_svg::SvgNode::Title(title)) = svg.children.first() {
        assert_eq!(title.content, Some("My Diagram".to_string()));
        println!("Title element fully parsed: {:?}", title.content);
    }
}

#[test]
fn test_svg_desc_element_supported() {
    let svg_str = include_str!("fixtures/advanced/desc.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse desc element SVG");
    assert_eq!(svg.children.len(), 2); // desc and rect
    if let Some(facet_svg::SvgNode::Desc(desc)) = svg.children.first() {
        assert_eq!(
            desc.content,
            Some("A simple diagram with one rectangle".to_string())
        );
        println!("Desc element fully parsed: {:?}", desc.content);
    }
}

#[test]
fn test_svg_symbol_element_supported() {
    let svg_str = include_str!("fixtures/advanced/symbol.svg");
    let svg: Svg = facet_svg::from_str(svg_str).expect("Failed to parse symbol element SVG");
    assert!(!svg.children.is_empty());
    if let Some(facet_svg::SvgNode::Defs(defs)) = svg.children.first()
        && let Some(facet_svg::SvgNode::Symbol(sym)) = defs.children.first()
    {
        assert_eq!(sym.id, Some("star".to_string()));
        assert_eq!(sym.view_box, Some("0 0 100 100".to_string()));
        println!("Symbol element fully parsed with id and viewBox");
    }
}
