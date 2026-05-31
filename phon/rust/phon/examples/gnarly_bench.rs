//! phon codec throughput on a copy of vox's `GnarlyPayload` — the realistic,
//! deeply-nested RPC payload vox-bench uses to compare codecs. The types are
//! copied (owned, `#[derive(Facet)]`) so this builds with no vox/figue deps; the
//! shapes match `spec-proto` exactly: `u64`/`String`/`Option`/`Vec<u8>`/
//! `Vec<u32>`/`Vec<String>`/`Vec<Vec<u8>>`/`Vec<struct>`/nested structs/a
//! `#[repr(u8)]` enum with struct payloads.
//!
//! A BORROWED mirror (`*Borrowed<'a>`) replaces every owned `String`/`Vec<u8>`
//! leaf with a zero-copy `&str`/`&[u8]` borrowing the input — same wire, no per-leaf
//! allocation on decode. The bench measures owned vs borrowed decode for both the
//! interpreter and the copy-and-patch JIT, showing the alloc-floor lift borrowing
//! buys.
//!
//! Measures phon's interpreter and copy-and-patch JIT for both encode and decode,
//! to hold against vox-jit (Cranelift) and vox-postcard (reflective) run from
//! vox-bench on the same payload.
//!
//! Run: `cargo run -p phon --release --features jit --example gnarly_bench`

use std::hint::black_box;
use std::mem::MaybeUninit;
use std::time::Instant;

use facet::Facet;
use phon_engine::{Registry, typed};
use phon_ir::MemProgram;

// ---- copied from vox `spec-proto` (owned variants) -------------------------

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
enum GnarlyKind {
    File { mime: String, tags: Vec<String> } = 0,
    Directory { child_count: u32, children: Vec<String> } = 1,
    Symlink { target: String, hops: Vec<u32> } = 2,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct GnarlyAttr {
    key: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct GnarlyEntry {
    id: u64,
    parent: Option<u64>,
    name: String,
    path: String,
    attrs: Vec<GnarlyAttr>,
    chunks: Vec<Vec<u8>>,
    kind: GnarlyKind,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct GnarlyPayload {
    revision: u64,
    mount: String,
    entries: Vec<GnarlyEntry>,
    footer: Option<String>,
    digest: Vec<u8>,
}

// ---- borrowed mirror: every owned String/Vec<u8> leaf is a zero-copy &str/&[u8]
//      borrowing the input. The wire is IDENTICAL to the owned peer (same schema
//      primitives), so the owned-encoded bytes decode straight into these.

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
enum GnarlyKindBorrowed<'a> {
    File { mime: &'a str, tags: Vec<&'a str> } = 0,
    Directory { child_count: u32, children: Vec<&'a str> } = 1,
    Symlink { target: &'a str, hops: Vec<u32> } = 2,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct GnarlyAttrBorrowed<'a> {
    key: &'a str,
    value: &'a str,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct GnarlyEntryBorrowed<'a> {
    id: u64,
    parent: Option<u64>,
    name: &'a str,
    path: &'a str,
    attrs: Vec<GnarlyAttrBorrowed<'a>>,
    chunks: Vec<&'a [u8]>,
    kind: GnarlyKindBorrowed<'a>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct GnarlyPayloadBorrowed<'a> {
    revision: u64,
    mount: &'a str,
    entries: Vec<GnarlyEntryBorrowed<'a>>,
    footer: Option<&'a str>,
    digest: &'a [u8],
}

fn make_gnarly_payload(entry_count: usize) -> GnarlyPayload {
    let entries = (0..entry_count)
        .map(|i| {
            let kind = match i % 3 {
                0 => GnarlyKind::File {
                    mime: "application/octet-stream".to_string(),
                    tags: vec!["hot".to_string(), "indexed".to_string(), format!("rev{i}")],
                },
                1 => GnarlyKind::Directory {
                    child_count: (i * 7) as u32,
                    children: (0..4).map(|c| format!("child-{i}-{c}")).collect(),
                },
                _ => GnarlyKind::Symlink {
                    target: format!("/var/lib/target/{i:08x}"),
                    hops: vec![i as u32, (i * 3) as u32, (i * 5) as u32],
                },
            };
            GnarlyEntry {
                id: i as u64 * 0x1_0001,
                parent: if i == 0 { None } else { Some((i - 1) as u64) },
                name: format!("entry-{i:05}"),
                path: format!("/mnt/store/bucket-{i:03}/object-{i:08x}.bin"),
                attrs: vec![
                    GnarlyAttr { key: "owner".to_string(), value: format!("user{i}") },
                    GnarlyAttr { key: "etag".to_string(), value: format!("{i:016x}") },
                ],
                chunks: (0..3).map(|c| vec![(i + c) as u8; 16 + c * 8]).collect(),
                kind,
            }
        })
        .collect();
    GnarlyPayload {
        revision: 0xCAFE_F00D_0000_0001,
        mount: "/mnt/bench".to_string(),
        entries,
        footer: Some("benchmark footer".to_string()),
        digest: vec![0xABu8; 32],
    }
}

// ---- bench plumbing --------------------------------------------------------

fn decode_interp(program: &MemProgram, wire: &[u8]) {
    let mut slot = MaybeUninit::<GnarlyPayload>::uninit();
    unsafe { typed::decode_with(program, wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
    let v = unsafe { slot.assume_init() };
    black_box(&v);
}

// Decode into the BORROWED payload: the decoded `&str`/`&[u8]` borrow `wire`, so
// `wire` (held by the caller across the timed loop) must outlive each `v`.
fn decode_interp_borrowed(program: &MemProgram, wire: &[u8]) {
    let mut slot = MaybeUninit::<GnarlyPayloadBorrowed<'_>>::uninit();
    unsafe { typed::decode_with(program, wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
    let v = unsafe { slot.assume_init() };
    black_box(&v);
}

fn encode_interp(program: &MemProgram, base: *const u8) {
    let bytes = unsafe { typed::encode_with(program, base) };
    black_box(&bytes);
}

fn bench(label: &str, iters: u64, mut f: impl FnMut()) -> f64 {
    for _ in 0..(iters / 20).max(1000) {
        f();
    }
    let t = Instant::now();
    for _ in 0..iters {
        f();
    }
    let ns = t.elapsed().as_nanos() as f64 / iters as f64;
    println!("  {label:<24} {ns:>9.1} ns/op");
    ns
}

fn main() {
    let n_entries = 16usize;
    let iters = 200_000u64;

    let d = phon::derive::of::<GnarlyPayload>().unwrap();
    let reg = Registry::new(d.schemas.clone());
    let program = typed::lower(&d.descriptor, &reg).unwrap();

    // The borrowed mirror: same wire, so the owned-encoded bytes decode into it.
    let db = phon::derive::of::<GnarlyPayloadBorrowed>().unwrap();
    let regb = Registry::new(db.schemas.clone());
    let program_b = typed::lower(&db.descriptor, &regb).unwrap();

    let payload = make_gnarly_payload(n_entries);
    let base = core::ptr::from_ref(&payload).cast::<u8>();
    let wire = unsafe { typed::encode_with(&program, base) };

    // Round-trip correctness checks before timing anything.
    {
        let mut slot = MaybeUninit::<GnarlyPayload>::uninit();
        unsafe { typed::decode_with(&program, &wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, payload, "phon round-trip mismatch");
    }
    // The borrowed payload decodes from the SAME wire and round-trips to the same
    // logical value (proving wire identity + correct zero-copy borrows).
    {
        let mut slot = MaybeUninit::<GnarlyPayloadBorrowed<'_>>::uninit();
        unsafe { typed::decode_with(&program_b, &wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        // Re-encode the borrowed value and check it equals the owned wire.
        let rewire = unsafe { typed::encode_with(&program_b, core::ptr::from_ref(&back).cast::<u8>()) };
        assert_eq!(rewire, wire, "borrowed wire != owned wire");
        assert_eq!(back.mount, payload.mount, "borrowed decode mismatch (mount)");
        assert_eq!(back.digest, payload.digest.as_slice(), "borrowed decode mismatch (digest)");
        assert_eq!(back.entries.len(), payload.entries.len(), "borrowed entry count mismatch");
    }

    println!(
        "GnarlyPayload {{ {n_entries} entries }}  ->  {} wire bytes  ({} owned / {} borrowed schemas)\n",
        wire.len(),
        d.schemas.len(),
        db.schemas.len(),
    );

    println!("decode (owned vs borrowed / zero-copy):");
    let di = bench("interpreter owned", iters, || decode_interp(&program, &wire));
    let dib = bench("interpreter borrowed", iters, || decode_interp_borrowed(&program_b, &wire));
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    let (dj, djb) = {
        use phon_jit::native::NativeDecode;
        let jit = NativeDecode::compile(&program);
        let dj = bench("jit owned", iters, || {
            let mut slot = MaybeUninit::<GnarlyPayload>::uninit();
            unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let v = unsafe { slot.assume_init() };
            black_box(&v);
        });
        let jitb = NativeDecode::compile(&program_b);
        let djb = bench("jit borrowed", iters, || {
            let mut slot = MaybeUninit::<GnarlyPayloadBorrowed<'_>>::uninit();
            unsafe { jitb.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let v = unsafe { slot.assume_init() };
            black_box(&v);
        });
        (dj, djb)
    };

    println!("\nencode:");
    let ei = bench("interpreter owned", iters, || encode_interp(&program, base));
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    let ej = {
        use phon_jit::native::NativeEncode;
        let jit = NativeEncode::compile(&program);
        bench("jit owned", iters, || {
            let bytes = unsafe { jit.run(base) };
            black_box(&bytes);
        })
    };

    println!("\nspeedup (borrowed vs owned decode):  interpreter {:.2}x", di / dib);
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    {
        println!("                                     jit         {:.2}x", dj / djb);
        println!(
            "speedup (jit vs interpreter):  decode owned {:.2}x  borrowed {:.2}x  encode {:.2}x",
            di / dj,
            dib / djb,
            ei / ej,
        );
    }
    #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
    let _ = (di, dib, ei);
}
