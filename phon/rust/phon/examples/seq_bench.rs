//! Decode AND encode throughput: the `phon-engine` interpreter vs the `phon-jit`
//! copy-and-patch JIT, on a `#[derive(Facet)]` struct with a `Vec` field. Both
//! consume the same lowered `MemProgram` and the same facet-bound thunks, so this
//! is an apples-to-apples interpreter-vs-JIT comparison. Allocation (the output
//! `Vec` on encode, the field `Vec` on decode) is paid on both sides.
//!
//! Run: `cargo run -p phon --release --features jit --example seq_bench`

use std::hint::black_box;
use std::mem::MaybeUninit;
use std::time::Instant;

use facet::Facet;
use phon_engine::{Registry, typed};
use phon_ir::Lowered;

#[derive(Facet)]
struct Msg {
    a: u64,
    b: u32,
    items: Vec<u32>,
}

const ITERS: u64 = 5_000_000;

fn decode_interp(program: &Lowered, wire: &[u8]) {
    let mut slot = MaybeUninit::<Msg>::uninit();
    unsafe { typed::decode_with(program, wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
    let msg = unsafe { slot.assume_init() };
    black_box(&msg);
}

fn encode_interp(program: &Lowered, base: *const u8) {
    let bytes = unsafe { typed::encode_with(program, base) };
    black_box(&bytes);
}

fn bench(label: &str, mut f: impl FnMut()) -> f64 {
    for _ in 0..100_000 {
        f();
    }
    let t = Instant::now();
    for _ in 0..ITERS {
        f();
    }
    let ns = t.elapsed().as_nanos() as f64 / ITERS as f64;
    println!("  {label:<14} {ns:>7.2} ns/op");
    ns
}

fn main() {
    let n_items = 32usize;
    let d = phon::derive::of::<Msg>().unwrap();
    let reg = Registry::new(d.schemas.clone());
    let program = typed::lower_typed(&d.descriptor, &d.descriptor_blocks, &reg).unwrap();

    let msg = Msg {
        a: 0x1122_3344_5566_7788,
        b: 0xCAFE_F00D,
        items: (0..n_items as u32).collect(),
    };
    let base = core::ptr::from_ref(&msg).cast::<u8>();
    let wire = unsafe { typed::encode_with(&program, base) };
    println!(
        "Msg {{ a: u64, b: u32, items: Vec<u32> x{n_items} }}  ->  {} wire bytes\n",
        wire.len()
    );

    println!("decode:");
    let _di = bench("interpreter", || decode_interp(&program, &wire));
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    let dj = {
        use phon_jit::native::NativeDecode;
        let jit = NativeDecode::compile(&program);
        bench("jit", || {
            let mut slot = MaybeUninit::<Msg>::uninit();
            unsafe { jit.run(&wire, slot.as_mut_ptr().cast::<u8>()) }.unwrap();
            let msg = unsafe { slot.assume_init() };
            black_box(&msg);
        })
    };

    println!("\nencode:");
    let _ei = bench("interpreter", || encode_interp(&program, base));
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    let ej = {
        use phon_jit::native::NativeEncode;
        let jit = NativeEncode::compile(&program);
        bench("jit", || {
            let bytes = unsafe { jit.run(base) };
            black_box(&bytes);
        })
    };

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    {
        println!(
            "\nspeedup (jit vs interpreter):  decode {:.2}x   encode {:.2}x",
            _di / dj,
            _ei / ej
        );
    }
}
