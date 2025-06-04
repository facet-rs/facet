use facet::Facet;

#[derive(Debug, Facet, PartialEq)]
struct Root {
    #[facet(default)]
    no_default: NoDefault,
}

#[derive(Debug, Facet, PartialEq)]
struct NoDefault(i32);

fn main() {}
