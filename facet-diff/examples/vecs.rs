use facet_diff::FacetDiff;

fn main() {
    let a = [1, 2, 3];
    let b = [2, 3, 1];

    let diff = a.diff(&b);
    println!("{diff}");
}
