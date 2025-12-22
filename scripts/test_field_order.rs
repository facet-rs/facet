use facet::Facet;

#[derive(Facet)]
struct TestStruct {
    zebra: u64,
    apple: String,
    middle: u32,
}

fn main() {
    let shape = TestStruct::SHAPE;
    println!("Field order in Shape:");
    if let facet_core::Type::User(facet_core::UserType::Struct(st)) = &shape.ty {
        for (i, field) in st.fields.iter().enumerate() {
            println!("  {}: {} at offset {}", i, field.name, field.offset);
        }
    }

    println!("\nExpected Rust memory layout:");
    println!("  zebra at offset 0");
    println!("  apple at offset 8");
    println!("  middle at offset 32");
}
