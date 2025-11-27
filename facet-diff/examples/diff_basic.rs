use facet::Facet;
use facet_diff::FacetDiff;

#[derive(Facet)]
struct User<'a> {
    name: &'a str,
    age: u8,
}

#[derive(Facet)]
struct OwnedUser {
    name: String,
    age: u8,
}

fn main() {
    let alice = User {
        name: "Alice",
        age: 30,
    };

    let bob = User {
        name: "Bob",
        age: 30,
    };

    let owned_alice = OwnedUser {
        name: "Alice".to_string(),
        age: 30,
    };

    let diff = alice.diff(&owned_alice);

    println!("{diff}");

    let diff = alice.diff(&bob);

    println!("{diff}");
}
