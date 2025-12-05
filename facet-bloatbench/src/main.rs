#![allow(unused_variables)]

#[cfg(feature = "json")]
use facet_bloatbench as bench;

fn main() {
    #[cfg(all(feature = "facet", feature = "facet_json"))]
    {
        let bytes = bench::facet_json_roundtrip();
        println!("facet_json_roundtrip bytes={bytes}");
    }

    #[cfg(all(feature = "serde", feature = "serde_json"))]
    {
        let bytes = bench::serde_json_roundtrip();
        println!("serde_json_roundtrip bytes={bytes}");
    }

    #[cfg(not(feature = "json"))]
    {
        println!("facet-bloatbench built without JSON features; nothing to run.");
    }
}
