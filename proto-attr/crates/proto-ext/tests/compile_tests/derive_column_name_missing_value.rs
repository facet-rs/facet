// Error: name field requires a value
use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(name))]
    id: i64,
}

fn main() {}
