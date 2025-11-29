// Error: skip doesn't take arguments
use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::skip("foo"))]
struct Foo {
    x: i32,
}

fn main() {}
