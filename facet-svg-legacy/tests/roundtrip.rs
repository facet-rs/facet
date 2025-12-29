use facet_svg_legacy::Svg;
use std::path::Path;

fn svg_roundtrip_test(path: &Path) -> datatest_stable::Result<()> {
    let fixture_path = path;
    let svg_str = std::fs::read_to_string(fixture_path)?;

    // Load the original SVG
    let svg1: Svg = facet_xml_legacy::from_str(&svg_str)
        .map_err(|e| format!("Failed to parse SVG from {}: {}", fixture_path.display(), e))?;

    // Serialize it back to XML
    let serialized1 =
        facet_xml_legacy::to_string(&svg1).map_err(|e| format!("Failed to serialize SVG: {}", e))?;

    println!("\n=== Serialized SVG for {} ===", fixture_path.display());
    println!("{}", serialized1);
    println!("=== End ===\n");

    // Deserialize again
    let svg2: Svg = facet_xml_legacy::from_str(&serialized1)
        .map_err(|e| format!("Failed to re-parse serialized SVG: {}", e))?;

    // Serialize the second one
    let serialized2 =
        facet_xml_legacy::to_string(&svg2).map_err(|e| format!("Failed to serialize SVG again: {}", e))?;

    // The two serializations should be identical - this verifies perfect roundtrip
    assert_eq!(
        serialized1,
        serialized2,
        "Serialized SVG should be identical after roundtrip for {}",
        fixture_path.display()
    );

    Ok(())
}

datatest_stable::harness! {
    { test = svg_roundtrip_test, root = "tests/fixtures", pattern = r".*\.svg$" },
}
