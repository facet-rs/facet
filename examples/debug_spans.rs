use facet::Facet;
use facet_pretty::{PathSegment, format_shape_with_spans};
use std::borrow::Cow;

#[derive(Facet)]
#[allow(dead_code)]
struct Config {
    name: String,
    max_retries: u8,
    enabled: bool,
}

fn main() {
    let fs = format_shape_with_spans(Config::SHAPE);

    println!("=== Formatted Text ===");
    for (i, line) in fs.text.lines().enumerate() {
        println!("{:3}: {}", i, line);
    }

    println!("\n=== Type Name Span ===");
    if let Some((start, end)) = fs.type_name_span {
        println!(
            "type_name_span: ({}, {}) = {:?}",
            start,
            end,
            &fs.text[start..end]
        );
    } else {
        println!("type_name_span: None");
    }

    println!("\n=== Field Spans ===");
    for (path, span) in &fs.spans {
        let path_str: Vec<_> = path
            .iter()
            .map(|p| match p {
                PathSegment::Field(name) => format!("Field({})", name),
                PathSegment::Variant(name) => format!("Variant({})", name),
                PathSegment::Index(i) => format!("Index({})", i),
            })
            .collect();
        println!("path {:?}:", path_str);
        println!(
            "  key: ({}, {}) = {:?}",
            span.key.0,
            span.key.1,
            &fs.text[span.key.0..span.key.1]
        );
        println!(
            "  value: ({}, {}) = {:?}",
            span.value.0,
            span.value.1,
            &fs.text[span.value.0..span.value.1]
        );
    }

    // Check what we're looking for
    let target_path = vec![PathSegment::Field(Cow::Borrowed("max_retries"))];
    println!("\n=== Looking up path for max_retries ===");
    if let Some(span) = fs.spans.get(&target_path) {
        println!(
            "Found! key: {:?}, value: {:?}",
            &fs.text[span.key.0..span.key.1],
            &fs.text[span.value.0..span.value.1]
        );
    } else {
        println!("Not found!");
    }
}
