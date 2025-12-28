use facet::Facet;
use facet_format_kdl::to_string;

#[derive(Debug, Facet)]
struct Record {
    name: String,
}

fn main() {
    let record = Record {
        name: "facet".to_string(),
    };
    let result = to_string(&record).unwrap();
    println!("Serialized: {:?}", result);
    println!("Serialized (raw):");
    println!("{}", result);
}
