use facet::Facet;
use facet_reflect::Peek;

fn debug<'facet, T: Facet<'facet>>(t: &T) -> String {
    let p = Peek::new(t);
    format!("{p:#?}")
}

#[derive(Facet, Debug)]
struct CustomStruct<T> {
    t: T,
}

fn main() {
    let actual = debug(&CustomStruct { t: 32 });
    let expected = format!("{:#?}", &CustomStruct { t: 32 });
    assert_eq!(actual, expected);
}
