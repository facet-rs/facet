// Error: unknown attribute with typo
use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::skp)] // typo: should be "skip"
struct Foo {
    x: i32,
}

fn main() {}
