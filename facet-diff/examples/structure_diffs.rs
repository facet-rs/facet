use facet::Facet;
use facet_diff::FacetDiff;

#[derive(Facet)]
struct User<'a> {
    name: &'a str,
    age: u8,
}

#[derive(Facet)]
struct Student<'a> {
    name: &'a str,
    age: u8,
    faculty: &'a str,
}

#[derive(Facet)]
struct NewUser<'a> {
    name: &'a str,
    age: u8,
    address: Address,
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
    county: String,
}

fn main() {
    let alice = User {
        name: "Alice",
        age: 30,
    };

    let bob = Student {
        name: "Bob",
        age: 30,
        faculty: "Computer Science",
    };

    let diff = alice.diff(&bob);

    println!("{diff}");

    let diff = bob.diff(&alice);

    println!("{diff}");

    let address = Address {
        street: "123 Main St".to_string(),
        city: "Wonderland".to_string(),
        county: "Imagination".to_string(),
    };

    let alice = NewUser {
        name: "Alice",
        age: 31,
        address,
    };

    let diff = bob.diff(&alice);

    println!("{diff}");
}
