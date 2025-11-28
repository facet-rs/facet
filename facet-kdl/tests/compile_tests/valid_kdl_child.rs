use facet::Facet;
use facet_kdl as kdl;

#[derive(Facet)]
struct Parent {
    #[facet(kdl::child)]
    child: Child,
}

#[derive(Facet)]
struct Child {
    name: String,
}

fn main() {}
