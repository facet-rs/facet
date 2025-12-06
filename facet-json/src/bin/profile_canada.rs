//! Standalone binary for profiling canada.json deserialization.
//! Run with: samply record cargo run --release -p facet-json --bin profile_canada

use facet::Facet;
use std::hint::black_box;

fn decompress_brotli(compressed: &[u8]) -> String {
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut std::io::Cursor::new(compressed), &mut decompressed).unwrap();
    String::from_utf8(decompressed).unwrap()
}

#[derive(Facet, Debug)]
struct Canada {
    #[facet(rename = "type")]
    type_: String,
    features: Vec<Feature>,
}

#[derive(Facet, Debug)]
struct Feature {
    #[facet(rename = "type")]
    type_: String,
    properties: Properties,
    geometry: Geometry,
}

#[derive(Facet, Debug)]
struct Properties {
    name: String,
}

#[derive(Facet, Debug)]
struct Geometry {
    #[facet(rename = "type")]
    type_: String,
    coordinates: Vec<Vec<Vec<f64>>>,
}

fn main() {
    eprintln!("Decompressing...");
    let json = decompress_brotli(include_bytes!("../../benches/corpus/canada.json.br"));
    eprintln!("JSON size: {} bytes", json.len());

    let iterations = 100;
    eprintln!("Running {iterations} iterations...");

    for i in 0..iterations {
        if i % 10 == 0 {
            eprintln!("  iteration {i}");
        }
        let result: Canada = facet_json::from_str(black_box(&json)).unwrap();
        black_box(result);
    }
    eprintln!("Done!");
}
