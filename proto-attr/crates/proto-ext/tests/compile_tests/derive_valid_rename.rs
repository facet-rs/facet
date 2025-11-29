// Valid: derive with rename attribute
use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::rename("new_name"))]
struct Foo {
    x: i32,
}

fn main() {}
