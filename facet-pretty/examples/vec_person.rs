use facet::Facet;
use facet_pretty::FacetPretty;

#[derive(Debug, Facet)]
struct Person {
    name: String,
}

fn main() {
    let alice = Person {
        name: "Alice".to_string(),
    };
    let bob = Person {
        name: "Bob".to_string(),
    };
    let carol = Person {
        name: "Carol".to_string(),
    };

    println!("{}", vec![alice, bob, carol].pretty());
}
