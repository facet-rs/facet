use facet::Facet;

#[derive(Facet)]
struct Foo {
    #[facet(skip_serializing_if = Option::is_some)]
    elems: Vec<i32>,
}

fn main() {}
