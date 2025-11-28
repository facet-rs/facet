use facet::Facet;

#[derive(Facet)]
struct Parent {
    #[facet(kdl::chld)] // typo: should be "child"
    child: Child,
}

#[derive(Facet)]
struct Child {
    name: String,
}

fn main() {
    // CURRENT BEHAVIOR: This compiles successfully even though 'chld'
    // is a typo for 'child'.
    //
    // DESIRED BEHAVIOR: This should fail to compile with a suggestion:
    // error[E0277]: `chld` is not a recognized KDL attribute
    //   --> src/main.rs:5:18
    //    |
    // 5  |     #[facet(kdl::chld)]
    //    |                  ^^^^ unknown attribute, did you mean `child`?
}
