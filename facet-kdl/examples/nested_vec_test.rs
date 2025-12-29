use facet::Facet;
use facet_kdl::{from_str, to_string};

#[derive(Facet, Debug, PartialEq)]
struct NestedVecWrapper {
    matrix: Vec<Vec<i32>>,
}

fn main() {
    let value = NestedVecWrapper {
        matrix: vec![vec![1, 2], vec![3, 4, 5]],
    };

    let serialized = to_string(&value).unwrap();
    println!("Serialized:");
    println!("{}", serialized);
    println!();

    // Try to deserialize back
    println!("Attempting to deserialize back...");
    match from_str::<NestedVecWrapper>(&serialized) {
        Ok(v) => println!("Success: {:?}", v),
        Err(e) => println!("Error: {}", e),
    }
}
