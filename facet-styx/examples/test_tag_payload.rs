use facet::Facet;

#[derive(Facet, Debug)]
#[facet(untagged)]
#[repr(u8)]
#[allow(dead_code)]
enum Value {
    #[facet(rename = "optional")]
    Optional(Vec<Value>),
    Simple(String),
}

fn main() {
    // Test: @optional(@string) should deserialize as Optional([Simple("string")])
    let source = "@optional(@string)";
    let result: Result<Value, _> = facet_styx::from_str(source);
    println!("Result: {result:?}");

    // Simpler test: just a string
    let source2 = "hello";
    let result2: Result<Value, _> = facet_styx::from_str(source2);
    println!("Result2: {result2:?}");
}
