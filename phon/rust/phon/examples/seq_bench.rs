//! Decode throughput: the `phon-engine` interpreter vs the `phon-jit`
//! copy-and-patch JIT, on a `#[derive(Facet)]` struct with a `Vec` field. Both
//! consume the same lowered `MemProgram` and the same facet-bound thunks, so this
//! is an apples-to-apples interpreter-vs-JIT decode comparison (allocation and
//! drop of the `Vec` included on both sides).
//!
//! Run: `cargo run -p phon --release --features jit --example seq_bench`

use std::hint::black_box;
use std::mem::MaybeUninit;
use std::time::Instant;

use facet::Facet;
use phon_engine::{Registry, typed};
use phon_ir::MemProgram;

#[derive(Facet)]
struct Msg {
    a: u64,
    b: u32,
    items: Vec<u32>,
}

const ITERS: u64 = 5_000_000;

fn decode_interp(program: &MemProgram, wire: &[u8]) {
    let mut slot = MaybeUninit::<Msg>::uninit();
    unsafe { typed::decode_with(program, wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
    let msg = unsafe { slot.assume_init() };
    black_box(&msg);
    // msg (and its Vec) drops here — the allocation is paid every iteration.
}

fn main() {
    let n_items = 32usize;
    let d = phon::derive::of::<Msg>().unwrap();
    let reg = Registry::new(d.schemas.clone());
    let program = typed::lower(&d.descriptor, &reg).unwrap();

    let msg = Msg {
        a: 0x1122_3344_5566_7788,
        b: 0xCAFE_F00D,
        items: (0..n_items as u32).collect(),
    };
    let wire =
        unsafe { typed::encode(core::ptr::from_ref(&msg).cast::<u8>(), &d.descriptor, &reg) }
            .unwrap();
    println!(
        "Msg {{ a: u64, b: u32, items: Vec<u32> x{n_items} }}  ->  {} wire bytes\n",
        wire.len()
    );

    for _ in 0..100_000 {
        decode_interp(&program, &wire);
    }
    let t = Instant::now();
    for _ in 0..ITERS {
        decode_interp(&program, &wire);
    }
    let dt_interp = t.elapsed();
    println!(
        "interpreter: {:>7.2} ns/op",
        dt_interp.as_nanos() as f64 / ITERS as f64
    );

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    {
        use phon_jit::native::NativeDecode;
        let jit = NativeDecode::compile(&program);
        let decode_jit = |jit: &NativeDecode, wire: &[u8]| {
            let mut slot = MaybeUninit::<Msg>::uninit();
            unsafe { jit.run(wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let msg = unsafe { slot.assume_init() };
            black_box(&msg);
        };
        for _ in 0..100_000 {
            decode_jit(&jit, &wire);
        }
        let t = Instant::now();
        for _ in 0..ITERS {
            decode_jit(&jit, &wire);
        }
        let dt_jit = t.elapsed();
        println!("jit:         {:>7.2} ns/op", dt_jit.as_nanos() as f64 / ITERS as f64);
        println!(
            "speedup:     {:.2}x",
            dt_interp.as_secs_f64() / dt_jit.as_secs_f64()
        );
    }
}
