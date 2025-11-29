// Valid: derive with skip attribute
use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::skip)]
struct Foo {
    x: i32,
}

fn main() {}
