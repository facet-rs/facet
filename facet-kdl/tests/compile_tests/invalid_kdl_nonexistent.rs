use facet::Facet;

#[derive(Facet)]
struct Config {
    #[facet(kdl::nonexistent)]
    field: String,
}

fn main() {
    // CURRENT BEHAVIOR: This compiles successfully even though 'nonexistent'
    // is not a valid KDL attribute.
    //
    // DESIRED BEHAVIOR: This should fail to compile with a helpful message:
    // error[E0277]: `nonexistent` is not a recognized KDL attribute
    //   --> src/main.rs:5:18
    //    |
    // 5  |     #[facet(kdl::nonexistent)]
    //    |                  ^^^^^^^^^^^ unknown attribute
    //    |
    //    = help: valid attributes are: `child`, `children`, `argument`, `property`, ...
}
