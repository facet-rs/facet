use facet::Facet;

#[derive(Debug, PartialEq, Facet)]
struct Simple {
    value: i32,
}

fn main() {
    let original = Simple { value: 42 };
    let bytes = facet_format_postcard::to_vec(&original).expect("serialize");
    println!("Serialized bytes: {:?}", bytes);

    let decoded: Simple = facet_format_postcard::from_slice(&bytes).expect("deserialize");
    println!("Decoded: {:?}", decoded);
    assert_eq!(decoded, original);

    // Also test raw i32
    let num: i32 = 123;
    let num_bytes = facet_format_postcard::to_vec(&num).expect("serialize i32");
    println!("i32 bytes: {:?}", num_bytes);

    let decoded_num: i32 = facet_format_postcard::from_slice(&num_bytes).expect("deserialize i32");
    println!("Decoded i32: {}", decoded_num);
    assert_eq!(decoded_num, num);
}
