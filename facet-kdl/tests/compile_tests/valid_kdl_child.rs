use facet::Facet;

#[derive(Facet)]
struct Parent {
    #[facet(kdl::child)]
    child: Child,
}

#[derive(Facet)]
struct Child {
    name: String,
}

fn main() {
    // This should compile successfully
}
