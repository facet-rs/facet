use facet::Facet;

#[derive(Facet)]
struct Server {
    #[facet(kdl::argument)]
    name: String,
    host: String,
    port: u16,
}

fn main() {
    // This should compile successfully
}
