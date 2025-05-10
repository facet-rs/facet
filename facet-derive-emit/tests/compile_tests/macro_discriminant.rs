use facet::Facet;

macro_rules! test_macro {
    ($a:literal, $b:literal) => {
        ($a << 8) | ($b)
    };
}

#[repr(u16)]
#[derive(Facet)]
enum TestEnum {
    Value1 = test_macro!(1, 2),
    Value2 = test_macro!(3, 4),
}

fn main() {
    // This program should not compile successfully due to the issue #378
    // The Facet derive macro should fail when handling non-literal enum discriminants
}
