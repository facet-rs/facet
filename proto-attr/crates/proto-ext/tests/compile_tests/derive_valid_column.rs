// Valid: derive with column attribute on field
use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(name = "user_id", primary_key))]
    id: i64,

    #[faket(proto_ext::column(name = "user_name"))]
    name: String,
}

fn main() {}
