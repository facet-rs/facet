//! phon codec throughput on a copy of vox's `GnarlyPayload` — the realistic,
//! deeply-nested RPC payload vox-bench uses to compare codecs. The types are
//! copied (owned, `#[derive(Facet)]`) so this builds with no vox/figue deps; the
//! shapes match `spec-proto` exactly: `u64`/`String`/`Option`/`Vec<u8>`/
//! `Vec<u32>`/`Vec<String>`/`Vec<Vec<u8>>`/`Vec<struct>`/nested structs/a
//! `#[repr(u8)]` enum with struct payloads. No maps, no borrowed fields.
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
    println!("  {label:<16} {ns:>9.1} ns/op");
    ns
}

fn main() {
    let n_entries = 16usize;
    let iters = 200_000u64;

    let d = phon::derive::of::<GnarlyPayload>().unwrap();
    let reg = Registry::new(d.schemas.clone());
    let program = typed::lower(&d.descriptor, &reg).unwrap();

    let payload = make_gnarly_payload(n_entries);
    let base = core::ptr::from_ref(&payload).cast::<u8>();
    let wire = unsafe { typed::encode_with(&program, base) };

    // Round-trip correctness check before timing anything.
    {
        let mut slot = MaybeUninit::<GnarlyPayload>::uninit();
        unsafe { typed::decode_with(&program, &wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
        let back = unsafe { slot.assume_init() };
        assert_eq!(back, payload, "phon round-trip mismatch");
    }

    println!(
        "GnarlyPayload {{ {n_entries} entries }}  ->  {} wire bytes  ({} schemas)\n",
        wire.len(),
        d.schemas.len()
    );

    println!("decode:");
    let di = bench("interpreter", iters, || decode_interp(&program, &wire));
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    let dj = {
        use phon_jit::native::NativeDecode;
        let jit = NativeDecode::compile(&program);
        bench("jit", iters, || {
            let mut slot = MaybeUninit::<GnarlyPayload>::uninit();
            unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let v = unsafe { slot.assume_init() };
            black_box(&v);
        })
    };

    println!("\nencode:");
    let ei = bench("interpreter", iters, || encode_interp(&program, base));
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    let ej = {
        use phon_jit::native::NativeEncode;
        let jit = NativeEncode::compile(&program);
        bench("jit", iters, || {
            let bytes = unsafe { jit.run(base) };
            black_box(&bytes);
        })
    };

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    println!("\nspeedup (jit vs interpreter):  decode {:.2}x   encode {:.2}x", di / dj, ei / ej);
    #[cfg(not(all(feature = "jit", target_os = "macos", target_arch = "aarch64")))]
    let _ = (di, ei);
}
