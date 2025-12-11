use facet_svg::Svg;

/// Test parsing a simple circle diagram
#[test]
fn test_parse_simple_circle() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100">
        <circle cx="50" cy="50" r="40" fill="blue" stroke="black" stroke-width="2"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse circle SVG");

    assert_eq!(svg.width, Some("100".to_string()));
    assert_eq!(svg.height, Some("100".to_string()));
    assert_eq!(svg.view_box, Some("0 0 100 100".to_string()));
    assert_eq!(svg.children.len(), 1);
}

/// Test parsing an SVG with multiple shapes
#[test]
fn test_parse_multiple_shapes() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
        <rect x="10" y="10" width="80" height="80" fill="red"/>
        <circle cx="150" cy="50" r="30" fill="green"/>
        <line x1="10" y1="150" x2="190" y2="150" stroke="black" stroke-width="2"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse multi-shape SVG");

    assert_eq!(svg.children.len(), 3);
}

/// Test parsing an SVG with groups
#[test]
fn test_parse_grouped_elements() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
        <g id="main-group" transform="translate(10, 10)">
            <rect x="0" y="0" width="50" height="50" fill="blue"/>
            <circle cx="75" cy="25" r="20" fill="green"/>
        </g>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse grouped SVG");

    assert_eq!(svg.children.len(), 1);
}

/// Test parsing SVG with text elements
#[test]
fn test_parse_text_elements() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
        <text x="10" y="20" fill="black" text-anchor="start" dominant-baseline="hanging">Hello SVG</text>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse text SVG");

    assert_eq!(svg.children.len(), 1);
}

/// Test parsing SVG with path data
#[test]
fn test_parse_path_element() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <path d="M 10 10 L 90 90 Q 50 50 10 90 Z" stroke="black" stroke-width="2" fill="none"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse path SVG");

    assert_eq!(svg.children.len(), 1);
}

/// Test parsing SVG with style element
#[test]
fn test_parse_style_element() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <style type="text/css">
            .myclass { fill: red; stroke: blue; }
        </style>
        <rect class="myclass" x="10" y="10" width="80" height="80"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse style SVG");

    assert!(svg.children.len() >= 1);
}

/// Test parsing SVG with defs section
#[test]
fn test_parse_defs_element() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <defs>
            <linearGradient id="grad1" x1="0%" y1="0%" x2="100%" y2="0%">
                <stop offset="0%" style="stop-color:rgb(255,255,0);stop-opacity:1" />
                <stop offset="100%" style="stop-color:rgb(255,0,0);stop-opacity:1" />
            </linearGradient>
        </defs>
        <rect x="10" y="10" width="80" height="80" fill="url(#grad1)"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse defs SVG");

    assert!(svg.children.len() >= 1);
}

/// Test parsing polygon and polyline elements
#[test]
fn test_parse_polygon_and_polyline() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
        <polygon points="50,10 90,90 10,90" fill="lime" stroke="purple" stroke-width="2"/>
        <polyline points="10,10 20,20 30,30 40,20 50,30" fill="none" stroke="red" stroke-width="2"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse polygon/polyline SVG");

    assert_eq!(svg.children.len(), 2);
}

/// Test parsing ellipse element
#[test]
fn test_parse_ellipse() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
        <ellipse cx="100" cy="50" rx="80" ry="30" fill="yellow" stroke="black" stroke-width="2"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse ellipse SVG");

    assert_eq!(svg.children.len(), 1);
}

/// Test parsing complex pikchr-style diagram
#[test]
fn test_parse_pikchr_style_diagram() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 200">
        <g id="diagram">
            <!-- Background -->
            <rect x="0" y="0" width="300" height="200" fill="white" stroke="gray" stroke-width="1"/>

            <!-- Boxes -->
            <rect x="20" y="20" width="80" height="60" fill="lightblue" stroke="navy" stroke-width="2"/>
            <text x="60" y="50" text-anchor="middle" dominant-baseline="middle" fill="navy">Start</text>

            <rect x="200" y="20" width="80" height="60" fill="lightgreen" stroke="darkgreen" stroke-width="2"/>
            <text x="240" y="50" text-anchor="middle" dominant-baseline="middle" fill="darkgreen">End</text>

            <!-- Arrow -->
            <path d="M 100 50 L 200 50" stroke="black" stroke-width="2" fill="none" marker-end="url(#arrowhead)"/>

            <!-- Bottom annotation -->
            <text x="150" y="150" text-anchor="middle" fill="gray">Process Flow</text>
        </g>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse pikchr-style SVG");

    // Should have the main group
    assert!(svg.children.len() >= 1);
}

/// Test parsing SVG with multiple namespaced elements
#[test]
fn test_parse_minimal_svg() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg">
        <rect width="100" height="100" fill="red"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse minimal SVG");

    assert_eq!(svg.children.len(), 1);
}

/// Test parsing an SVG with stroke dasharray
#[test]
fn test_parse_dashed_lines() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="50">
        <line x1="10" y1="25" x2="190" y2="25" stroke="black" stroke-width="2" stroke-dasharray="5,5"/>
        <circle cx="100" cy="25" r="10" fill="none" stroke="blue" stroke-width="1" stroke-dasharray="2,3"/>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse dashed SVG");

    assert_eq!(svg.children.len(), 2);
}

/// Test parsing nested groups
#[test]
fn test_parse_nested_groups() {
    let svg_str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
        <g id="outer" transform="translate(10, 10)">
            <rect x="0" y="0" width="100" height="100" fill="lightgray"/>
            <g id="inner" transform="translate(20, 20)">
                <circle cx="0" cy="0" r="20" fill="red"/>
            </g>
        </g>
    </svg>"#;

    let svg: Svg = facet_xml::from_str(svg_str).expect("Failed to parse nested groups SVG");

    assert_eq!(svg.children.len(), 1);
}
