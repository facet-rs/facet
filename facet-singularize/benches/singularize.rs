use divan::{Bencher, black_box};
use facet_singularize::{is_singular_of, singularize};

fn main() {
    divan::main();
}

const IE_EXCEPTIONS: &[&str] = &[
    "movies", "cookies", "pies", "ties", "brownies", "rookies", "selfies",
];
const IES_TO_Y: &[&str] = &["dependencies", "categories", "policies", "ponies", "babies"];
const SIMPLE_S: &[&str] = &[
    "items", "samples", "users", "configs", "servers", "handlers",
];

#[divan::bench]
fn bench_singularize_ie_exception(bencher: Bencher) {
    bencher.bench(|| {
        for word in IE_EXCEPTIONS {
            black_box(singularize(black_box(word)));
        }
    });
}

#[divan::bench]
fn bench_singularize_ies_to_y(bencher: Bencher) {
    bencher.bench(|| {
        for word in IES_TO_Y {
            black_box(singularize(black_box(word)));
        }
    });
}

#[divan::bench]
fn bench_singularize_simple_s(bencher: Bencher) {
    bencher.bench(|| {
        for word in SIMPLE_S {
            black_box(singularize(black_box(word)));
        }
    });
}

#[divan::bench]
fn bench_is_singular_of_ie_exception(bencher: Bencher) {
    bencher.bench(|| {
        for word in IE_EXCEPTIONS {
            let singular = singularize(word);
            black_box(is_singular_of(black_box(&singular), black_box(word)));
        }
    });
}
