//! Visual demo of rediff's hex-dump byte diffing.
//!
//! Run with: `cargo run --example hexdump_diff`

use facet::Facet;
use rediff::{FacetDiff, check_same_report, format_diff_default};

#[derive(Facet)]
struct Packet {
    id: u32,
    payload: Vec<u8>,
}

fn main() {
    // A small change deep in a buffer.
    let old = Packet {
        id: 7,
        payload: vec![0xde, 0xad, 0xbe, 0xef],
    };
    let new = Packet {
        id: 7,
        payload: vec![0xde, 0xad, 0xca, 0xfe],
    };

    println!("== Display (tree) path ==");
    println!("{}", format_diff_default(&old.diff(&new)));

    println!("\n== DiffReport (layout) path ==");
    let report = check_same_report(&old, &new);
    if let Some(report) = report.diff() {
        println!("{}", report.render_ansi_rust());
    }

    // A larger buffer with collapsed unchanged rows.
    let big_old: Vec<u8> = (0u8..72).collect();
    let mut big_new = big_old.clone();
    big_new[64] = 0xff;

    println!("\n== Larger buffer (collapsed rows) ==");
    println!("{}", format_diff_default(&big_old.diff(&big_new)));
}
