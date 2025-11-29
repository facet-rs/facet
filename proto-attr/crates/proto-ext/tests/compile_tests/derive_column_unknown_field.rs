// Error: unknown field in column
use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(nam = "id"))] // typo: should be "name"
    id: i64,
}

fn main() {}
