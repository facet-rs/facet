use facet_diff::FacetDiff;

fn main() {
    let a = [1, 2, 3];
    let b = [2, 3, 1];

    let diff = a.diff(&b);
    println!("{diff}");

    let b = b.as_slice();
    let a = &a;
    let diff = a.diff(&b);
    println!("{diff}");

    let a = vec![1, 2, 3, 4];
    let b = vec![4, 5, 6, 7];
    let diff = a.diff(&b);
    println!("{diff}");

    let b = [5, 6, 4, 7];
    let diff = a.diff(&b);
    println!("{diff}");

    let a = (1, 2, 3);
    let b = (2, 3, 1, 4);
    let diff = a.diff(&b);
    println!("{diff}");

    let a = ((1, 2), (3, 4));
    let b = ((1, 2, 3), (4, 5));
    let diff = a.diff(&b);
    println!("{diff}");
}
