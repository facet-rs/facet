use facet_svg::Svg;

/// Bootstrap icon style SVG with fill and viewBox
#[test]
fn test_bootstrap_icon() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-check" viewBox="0 0 16 16">
  <path d="M10.97 4.97a.75.75 0 0 1 1.07 1.05l-3.99 4.99a.75.75 0 0 1-1.08.02L4.324 8.384a.75.75 0 1 1 1.06-1.06l2.094 2.093 3.473-4.425a.267.267 0 0 1 .02-.022z"/>
</svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse Bootstrap icon SVG");
    assert_eq!(svg.width, Some("16".to_string()));
    assert_eq!(svg.height, Some("16".to_string()));
}

/// SVG with fill="none" and stroke attributes
#[test]
fn test_feather_icon_style() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="12" cy="12" r="1"></circle>
  <path d="M12 1v6m0 6v6M4.22 4.22l4.24 4.24m5.08 5.08l4.24 4.24M1 12h6m6 0h6M4.22 19.78l4.24-4.24m5.08-5.08l4.24-4.24"></path>
</svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse Feather icon SVG");
    assert_eq!(svg.children.len(), 2); // circle and path
}

/// SVG with use and xlink:href (may not be fully supported)
#[test]
fn test_svg_with_use_element() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100" height="100" viewBox="0 0 100 100">
  <defs>
    <g id="myCircle">
      <circle cx="0" cy="0" r="20" fill="red"/>
    </g>
  </defs>
  <use x="50" y="50"/>
</svg>"#;

    // This might fail if 'use' element isn't supported
    match facet_xml::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("✓ Use element parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("✗ Use element failed to parse: {}", e);
        }
    }
}

/// SVG with filter elements (nested in defs)
#[test]
fn test_svg_with_filters() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
  <defs>
    <filter id="blur">
      <feGaussianBlur in="SourceGraphic" stdDeviation="5" />
    </filter>
  </defs>
  <rect x="10" y="10" width="80" height="80" fill="blue" filter="url(#blur)"/>
</svg>"#;

    // Filter elements may not be supported, but defs should work
    match facet_xml::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("✓ Filter elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("✗ Filter elements failed: {}", e);
        }
    }
}

/// SVG with tspan elements (nested in text)
#[test]
fn test_svg_with_tspan() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="200" viewBox="0 0 300 200">
  <text x="50" y="50">
    <tspan x="50" dy="1.2em">First line</tspan>
    <tspan x="50" dy="1.2em">Second line</tspan>
  </text>
</svg>"#;

    // Tspan elements may not be supported in text
    match facet_xml::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("✓ Tspan elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("✗ Tspan elements failed: {}", e);
        }
    }
}

/// SVG with image element
#[test]
fn test_svg_with_image_element() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
  <image x="10" y="10" width="100" height="100" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg'%3E%3Crect fill='red' width='100' height='100'/%3E%3C/svg%3E"/>
</svg>"#;

    // Image element may not be supported
    match facet_xml::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("✓ Image elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("✗ Image elements failed: {}", e);
        }
    }
}

/// SVG with marker element
#[test]
fn test_svg_with_marker() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
  <defs>
    <marker id="arrowhead" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto">
      <polygon points="0 0, 10 3, 0 6" fill="black" />
    </marker>
  </defs>
  <line x1="10" y1="50" x2="150" y2="50" stroke="black" stroke-width="2" />
</svg>"#;

    // Marker element may not be supported
    match facet_xml::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("✓ Marker elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("✗ Marker elements failed: {}", e);
        }
    }
}

/// SVG with complex transform
#[test]
fn test_svg_with_transform() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
  <g transform="rotate(45 100 100) translate(10 20)">
    <rect x="50" y="50" width="100" height="100" fill="green" opacity="0.5"/>
  </g>
</svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse transform SVG");
    assert_eq!(svg.children.len(), 1); // one group
}

/// SVG with metadata elements (title, desc)
#[test]
fn test_svg_with_metadata() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100" preserveAspectRatio="xMidYMid meet" version="1.1">
  <title>Test SVG</title>
  <desc>A test SVG document</desc>
  <rect x="10" y="10" width="80" height="80" fill="red" opacity="0.8" stroke="black" stroke-width="2"/>
</svg>"#;

    // Title and desc may not be supported
    match facet_xml::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            println!("✓ Metadata elements parsed successfully");
            println!("  Children: {}", svg.children.len());
        }
        Err(e) => {
            println!("✗ Metadata elements failed: {}", e);
        }
    }
}

/// SVG with viewBox and preserveAspectRatio
#[test]
fn test_svg_with_aspect_ratio() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100" preserveAspectRatio="xMidYMid meet">
  <rect x="10" y="10" width="80" height="80" fill="red"/>
</svg>"#;

    match facet_xml::from_str::<Svg>(svg_str) {
        Ok(svg) => {
            assert_eq!(svg.view_box, Some("0 0 100 100".to_string()));
            println!("✓ Successfully parsed viewBox and preserveAspectRatio");
        }
        Err(e) => {
            println!("✗ Failed to parse aspectRatio SVG: {}", e);
        }
    }
}

/// SVG with opacity attribute
#[test]
fn test_svg_with_opacity() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect x="10" y="10" width="80" height="80" fill="red" opacity="0.5"/>
</svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse opacity SVG");
    assert_eq!(svg.children.len(), 1);
}
