use facet::Facet;
use facet_pretty::FacetPretty;

#[derive(Debug, Facet)]
struct Person {
    name: String,
    age: u32,
    address: Address,
}

#[derive(Debug, Facet)]
struct Address {
    street: String,
    city: String,
    country: String,
}

fn main() {
    let address = Address {
        street: "123 Main St".to_string(),
        city: "Wonderland".to_string(),
        country: "Imagination".to_string(),
    };

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
        address,
    };

    println!("Default pretty-printing:");
    println!("{}", person.pretty());
}
