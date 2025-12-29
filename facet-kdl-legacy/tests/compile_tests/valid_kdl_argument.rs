use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Server {
    #[facet(kdl::argument)]
    name: String,
    host: String,
    port: u16,
}

fn main() {}
