use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Parent {
    #[facet(kdl::chld)] // typo: should be "child"
    child: Child,
}

#[derive(Facet)]
struct Child {
    name: String,
}

fn main() {}
