use facet::Facet;
use facet_pretty::FacetPretty;

#[derive(Facet)]
struct CustomStruct<T> {
    t: T,
}

#[derive(Facet)]
struct NotDebug;

fn main() {
    let cs = CustomStruct { t: 32 };
    eprintln!("{}", cs.pretty());

    let cs = CustomStruct { t: NotDebug };
    eprintln!("{}", cs.pretty());
}
