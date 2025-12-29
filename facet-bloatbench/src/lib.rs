#![allow(dead_code)]
#![allow(clippy::all)]

mod generated;

#[cfg(any(feature = "facet", feature = "serde"))]
pub use generated::*;

// Public probes to force monomorphization in both stacks.

#[cfg(feature = "facet")]
pub fn facet_json_roundtrip() -> usize {
    use generated::facet_types::*;
    let mut total = 0usize;

    // Touch a handful of structs to pull serializer/deserializer instantiations.
    macro_rules! touch {
        ($ty:ty) => {{
            let v: $ty = Default::default();
            let s = facet_json_legacy::to_string(&v);
            total += s.len();
            let _: $ty = facet_json_legacy::from_str(&s).expect("facet_json deserialize");
        }};
    }

    touch!(Struct000);
    touch!(Struct001);
    touch!(Struct002);

    total
}

#[cfg(feature = "serde")]
pub fn serde_json_roundtrip() -> usize {
    use generated::serde_types::*;
    let mut total = 0usize;

    macro_rules! touch {
        ($ty:ty) => {{
            let v: $ty = Default::default();
            let s = serde_json::to_string(&v).expect("serde_json serialize");
            total += s.len();
            let _: $ty = serde_json::from_str(&s).expect("serde_json deserialize");
        }};
    }

    touch!(Struct000);
    touch!(Struct001);
    touch!(Struct002);

    total
}
