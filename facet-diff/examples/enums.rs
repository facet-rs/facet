#![allow(unused)]

use facet::Facet;
use facet_diff::FacetDiff;

#[derive(Facet)]
#[repr(C)]
enum Update {
    Remove(usize),
    Add(usize),
}

#[derive(Facet)]
#[repr(C)]
enum NewUpdate {
    Remove(usize),
    Add(usize),
    Unknown,
}

#[derive(Facet)]
#[repr(C)]
enum MaybeUpdate {
    Remove(Option<usize>),
    Add(Option<usize>),
}

fn main() {
    let a = Update::Remove(0);
    let b = Update::Remove(1);
    let c = Update::Add(0);
    let d = NewUpdate::Remove(0);
    let e = NewUpdate::Unknown;
    let f = MaybeUpdate::Remove(None);
    let g = MaybeUpdate::Remove(Some(0));
    let h = MaybeUpdate::Remove(Some(1));

    let mut diff = a.diff(&b);
    println!("{diff}");

    diff = a.diff(&c);
    println!("{diff}");

    diff = b.diff(&c);
    println!("{diff}");

    diff = a.diff(&d);
    println!("{diff}");

    diff = a.diff(&e);
    println!("{diff}");

    diff = a.diff(&f);
    println!("{diff}");

    diff = a.diff(&g);
    println!("{diff}");

    diff = f.diff(&g);
    println!("{diff}");

    diff = g.diff(&h);
    println!("{diff}");
}
