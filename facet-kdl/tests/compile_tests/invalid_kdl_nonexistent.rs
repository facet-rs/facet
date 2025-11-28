use facet::Facet;
use facet_kdl as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::nonexistent)]
    field: String,
}

fn main() {}
