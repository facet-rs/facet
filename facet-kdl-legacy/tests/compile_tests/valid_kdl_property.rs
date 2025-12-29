use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

fn main() {}
