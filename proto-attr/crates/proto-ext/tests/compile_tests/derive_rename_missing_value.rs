// Error: rename requires a value
use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::rename)]
struct Foo {
    x: i32,
}

fn main() {}
