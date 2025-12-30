//! Test that i32 can be serialized/deserialized with facet-format-postcard

#[test]
fn test_i32_roundtrip() {
    let num: i32 = 123;
    let bytes = rapace::facet_postcard::to_vec(&num).expect("serialize i32");
    println!("i32 serialized to {} bytes: {:?}", bytes.len(), bytes);

    let decoded: i32 = rapace::facet_postcard::from_slice(&bytes).expect("deserialize i32");
    println!("Decoded i32: {}", decoded);
    assert_eq!(decoded, num);
}

#[test]
fn test_tuple_roundtrip() {
    let tuple: (i32, i32) = (5, 3);
    let bytes = rapace::facet_postcard::to_vec(&tuple).expect("serialize tuple");
    println!("Tuple serialized to {} bytes: {:?}", bytes.len(), bytes);

    let decoded: (i32, i32) =
        rapace::facet_postcard::from_slice(&bytes).expect("deserialize tuple");
    println!("Decoded tuple: {:?}", decoded);
    assert_eq!(decoded, tuple);
}

#[test]
fn test_unit_roundtrip() {
    let unit: () = ();
    let bytes = rapace::facet_postcard::to_vec(&unit).expect("serialize ()");
    println!("() serialized to {} bytes: {:?}", bytes.len(), bytes);

    let decoded: () = rapace::facet_postcard::from_slice(&bytes).expect("deserialize ()");
    println!("Decoded (): {:?}", decoded);
    // No assertion needed - successful deserialization validates the roundtrip
    let _ = (decoded, unit);
}
