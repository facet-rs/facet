//! A rough decode-throughput comparison: the portable threaded executor vs the
//! native copy-and-patch JIT, on a struct-shaped scalar program.
//!
//! Run with: `cargo run -p phon-jit --release --example bench`

use std::time::Instant;

use phon_ir::ir::{MemOp, MemProgram, fuse};
use phon_jit::compile_decode;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use phon_jit::native::NativeDecode;

const ITERS: u64 = 20_000_000;

fn time_threaded(program: &MemProgram, wire: &[u8], out: &mut [u8]) -> f64 {
    let dec = compile_decode(program);
    for _ in 0..100_000 {
        unsafe { dec.run(wire, out.as_mut_ptr()).unwrap() };
    }
    let t = Instant::now();
    for _ in 0..ITERS {
        unsafe { dec.run(wire, out.as_mut_ptr()).unwrap() };
    }
    t.elapsed().as_nanos() as f64 / ITERS as f64
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn time_jit(program: &MemProgram, wire: &[u8], out: &mut [u8]) -> f64 {
    let jit = NativeDecode::compile(program);
    for _ in 0..100_000 {
        unsafe { jit.run(wire, out.as_mut_ptr()).unwrap() };
    }
    let t = Instant::now();
    for _ in 0..ITERS {
        unsafe { jit.run(wire, out.as_mut_ptr()).unwrap() };
    }
    t.elapsed().as_nanos() as f64 / ITERS as f64
}

fn main() {
    // A struct of mixed-width scalars (offsets/aligns as a real layout).
    let program: MemProgram = vec![
        MemOp::Scalar {
            offset: 0,
            size: 8,
            align: 8,
        },
        MemOp::Scalar {
            offset: 8,
            size: 4,
            align: 4,
        },
        MemOp::Scalar {
            offset: 12,
            size: 2,
            align: 2,
        },
        MemOp::Scalar {
            offset: 14,
            size: 1,
            align: 1,
        },
        MemOp::Scalar {
            offset: 16,
            size: 8,
            align: 8,
        },
        MemOp::Scalar {
            offset: 24,
            size: 4,
            align: 4,
        },
        MemOp::Scalar {
            offset: 28,
            size: 4,
            align: 4,
        },
        MemOp::Scalar {
            offset: 32,
            size: 8,
            align: 8,
        },
    ];
    let memsize = 40;

    // Build wire bytes by encoding a filled buffer through the threaded encoder.
    let src: Vec<u8> = (0..memsize as u8).collect();
    let wire = {
        let enc = phon_jit::compile_encode(&program);
        unsafe { enc.run(src.as_ptr()) }
    };
    #[cfg(phon_jit_tailcall)]
    let stencil_mode = "tail-call (nightly become)";
    #[cfg(not(phon_jit_tailcall))]
    let stencil_mode = "call (stable)";

    let fused = fuse(program.clone());
    println!(
        "program: {} scalar ops -> {} after fusion, {} wire bytes",
        program.len(),
        fused.len(),
        wire.len(),
    );
    println!("jit stencils: {stencil_mode}\n");

    let mut out = vec![0u8; memsize];
    println!("                  unfused      fused");
    let tu = time_threaded(&program, &wire, &mut out);
    let tf = time_threaded(&fused, &wire, &mut out);
    println!("threaded:   {tu:>8.2}   {tf:>8.2}  ns/op");

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let ju = time_jit(&program, &wire, &mut out);
        let jf = time_jit(&fused, &wire, &mut out);
        println!("jit:        {ju:>8.2}   {jf:>8.2}  ns/op");
        println!(
            "\nbest (jit+fused) vs baseline (threaded, unfused): {:.2}x",
            tu / jf,
        );
    }

    std::hint::black_box(&out);
}
