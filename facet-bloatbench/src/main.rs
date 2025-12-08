#![allow(unused_variables)]

fn main() {
    #[cfg(feature = "facet")]
    {
        let bytes = facet_bloatbench::facet_json_roundtrip();
        println!("facet_json_roundtrip bytes={bytes}");
    }

    #[cfg(feature = "serde")]
    {
        let bytes = facet_bloatbench::serde_json_roundtrip();
        println!("serde_json_roundtrip bytes={bytes}");
    }

    #[cfg(not(any(feature = "facet", feature = "serde")))]
    {
        println!("facet-bloatbench built without facet or serde features");
    }
}
