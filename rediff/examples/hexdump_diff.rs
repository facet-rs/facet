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

    // A single byte changed deep in a larger buffer.
    let big_old: Vec<u8> = (0u8..72).collect();
    let mut big_new = big_old.clone();
    big_new[0x24] = 0xff;

    println!("\n== One byte changed in a larger buffer ==");
    println!("{}", format_diff_default(&big_old.diff(&big_new)));

    // An inserted byte: a real binary diff localizes it instead of
    // marking everything after it as changed.
    let ins_old: Vec<u8> = (0u8..48).collect();
    let mut ins_new = ins_old.clone();
    ins_new.insert(0x14, 0xaa);

    println!("\n== One byte inserted (shift, not mass-rewrite) ==");
    println!("{}", format_diff_default(&ins_old.diff(&ins_new)));
}
