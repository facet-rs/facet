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
        // Still force monomorphization of all types even without JSON
        #[cfg(feature = "facet")]
        {
            use facet_bloatbench::facet_types::*;
            // Touch all struct types to force codegen
            let _ = std::hint::black_box(Struct000::default());
            let _ = std::hint::black_box(Struct001::default());
            let _ = std::hint::black_box(Struct002::default());
        }
        #[cfg(feature = "serde")]
        {
            use facet_bloatbench::serde_types::*;
            let _ = std::hint::black_box(Struct000::default());
            let _ = std::hint::black_box(Struct001::default());
            let _ = std::hint::black_box(Struct002::default());
        }
        println!("facet-bloatbench built without JSON features; touched types for codegen.");
    }
}
