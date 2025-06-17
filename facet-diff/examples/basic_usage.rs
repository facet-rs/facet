use facet_diff::FacetDiff;

fn main() {
    let borrowed = "Hello, World";
    let owned = String::from(borrowed);

    let diff = borrowed.diff(&owned);

    println!("{diff}");
}
