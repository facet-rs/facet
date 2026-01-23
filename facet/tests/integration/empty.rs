use facet::Facet;

#[test]
fn empty() {
    let shape = String::SHAPE;
    // on rust 1.86, this line is load-bearing.
    // cf. https://github.com/facet-rs/facet/issues/75
    println!("{shape}");
}
