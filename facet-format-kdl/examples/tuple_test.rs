use facet::Facet;
use facet_format_kdl::{from_str, to_string};

#[derive(Facet, Debug, PartialEq)]
struct TupleWrapper {
    triple: (String, i32, bool),
}

fn main() {
    let value = TupleWrapper {
        triple: ("hello".to_string(), 42, true),
    };

    let serialized = to_string(&value).unwrap();
    println!("Serialized:");
    println!("{}", serialized);
    println!();

    // Try to deserialize back
    println!("Attempting to deserialize back...");
    match from_str::<TupleWrapper>(&serialized) {
        Ok(v) => println!("Success: {:?}", v),
        Err(e) => println!("Error: {}", e),
    }
}
