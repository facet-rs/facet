use facet::Facet;

#[derive(Facet)]
struct Config {
    #[facet(kdl::property)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

fn main() {
    // This should compile successfully
}
