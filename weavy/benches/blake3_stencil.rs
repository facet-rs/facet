use std::hint::black_box;
use std::time::{Duration, Instant};

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod native {
    use super::*;
    use weavy::jit::{NativeProgram, RawHostCallChain, StencilLayout, blake3_stencils, stencils};

    #[repr(C)]
    struct Ctx {
        prog: *const u64,
        frame: *mut u8,
        ready: *mut i64,
        awaited: *const i64,
        resume: *mut u64,
        await_index: *mut u64,
        exit: *mut i64,
    }

    #[derive(Clone, Copy)]
    struct Case {
        label: &'static str,
        len: usize,
        iterations: usize,
    }

    #[derive(Clone, Copy)]
    struct Row {
        case: Case,
        native_ns: f64,
        host_ns: f64,
        stencil_ns: f64,
    }

    struct StencilChain {
        native: NativeProgram,
    }

    impl StencilChain {
        fn new() -> Self {
            let mut layout = StencilLayout::new();
            let root = layout.start_chain();
            for word in [0, 8, 16, 24] {
                layout.push_prog_word(root.prog_index, word);
            }
            let hash = layout.emit_stencil(blake3_stencils::HASH);
            let done = layout.emit_stencil(stencils::DONE);
            for &rel in blake3_stencils::HASH_CONT {
                layout.patch_continuation(hash + rel, done);
            }
            StencilChain {
                native: NativeProgram::new(layout, root),
            }
        }

        fn run(&self, frame: &mut [u64; 4]) {
            let mut resume = 0;
            let mut await_index = 0;
            let mut exit = 0;
            let mut ctx = Ctx {
                prog: self.native.entry_prog(),
                frame: frame.as_mut_ptr().cast(),
                ready: core::ptr::null_mut(),
                awaited: core::ptr::null(),
                resume: &mut resume,
                await_index: &mut await_index,
                exit: &mut exit,
            };
            let entry = unsafe { self.native.entry_fn::<Ctx>() };
            unsafe { entry(&mut ctx) };
        }
    }

    struct EmptyNativeChain {
        native: NativeProgram,
    }

    impl EmptyNativeChain {
        fn new() -> Self {
            let mut layout = StencilLayout::new();
            let root = layout.start_chain();
            layout.emit_stencil(stencils::DONE);
            EmptyNativeChain {
                native: NativeProgram::new(layout, root),
            }
        }

        fn run(&self) {
            let mut frame = [0u64; 4];
            let mut resume = 0;
            let mut await_index = 0;
            let mut exit = 0;
            let mut ctx = Ctx {
                prog: self.native.entry_prog(),
                frame: frame.as_mut_ptr().cast(),
                ready: core::ptr::null_mut(),
                awaited: core::ptr::null(),
                resume: &mut resume,
                await_index: &mut await_index,
                exit: &mut exit,
            };
            let entry = unsafe { self.native.entry_fn::<Ctx>() };
            unsafe { entry(&mut ctx) };
        }
    }

    unsafe extern "C" fn host_blake3(cx: *mut (), _info: *const ()) -> bool {
        let frame = unsafe { &mut *cx.cast::<[u64; 4]>() };
        let input =
            unsafe { core::slice::from_raw_parts(frame[0] as *const u8, frame[1] as usize) };
        let out = frame[3] as *mut u8;
        let hash = blake3::hash(input);
        unsafe {
            core::ptr::copy_nonoverlapping(hash.as_bytes().as_ptr(), out, 32);
        }
        true
    }

    unsafe extern "C" fn host_empty(_cx: *mut (), _info: *const ()) -> bool {
        true
    }

    fn ns_per_call(mut f: impl FnMut(), iterations: usize) -> f64 {
        for _ in 0..128 {
            f();
        }
        let mut samples = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            for _ in 0..iterations {
                f();
            }
            samples.push(start.elapsed());
        }
        samples.sort_unstable();
        nanos(samples[samples.len() / 2]) / iterations as f64
    }

    fn sample(mut f: impl FnMut(), iterations: usize) -> Duration {
        let start = Instant::now();
        for _ in 0..iterations {
            f();
        }
        start.elapsed()
    }

    fn median_ns(samples: &mut [Duration], iterations: usize) -> f64 {
        samples.sort_unstable();
        nanos(samples[samples.len() / 2]) / iterations as f64
    }

    fn nanos(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1_000_000_000.0
    }

    fn input(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add((i >> 8) as u8))
            .collect()
    }

    fn native_hash_into(input: &[u8], out: &mut [u8; 32]) {
        let hash = blake3::hash(input);
        out.copy_from_slice(hash.as_bytes());
    }

    fn frame(input: &[u8], scratch: &mut [u8], out: &mut [u8; 32]) -> [u64; 4] {
        [
            input.as_ptr() as u64,
            input.len() as u64,
            scratch.as_mut_ptr() as u64,
            out.as_mut_ptr() as u64,
        ]
    }

    fn assert_paths_match(input: &[u8], stencil: &StencilChain, host: &RawHostCallChain<()>) {
        let expected = blake3::hash(input);

        let mut host_out = [0u8; 32];
        let mut host_scratch = vec![0u8; (input.len() / 1024).max(1) * 32];
        let mut host_frame = frame(input, &mut host_scratch, &mut host_out);
        unsafe { host.run(&mut host_frame) };
        assert_eq!(host_out, *expected.as_bytes(), "host-call path drift");

        let mut stencil_out = [0u8; 32];
        let mut stencil_scratch = vec![0u8; (input.len() / 1024).max(1) * 32];
        let mut stencil_frame = frame(input, &mut stencil_scratch, &mut stencil_out);
        stencil.run(&mut stencil_frame);
        assert_eq!(stencil_out, *expected.as_bytes(), "stencil path drift");
    }

    fn measure_case(case: Case, stencil: &StencilChain, host: &RawHostCallChain<()>) -> Row {
        let input = input(case.len);
        assert_paths_match(&input, stencil, host);

        let mut native_out = [0u8; 32];
        let mut host_out = [0u8; 32];
        let mut host_scratch = vec![0u8; (case.len / 1024).max(1) * 32];
        let mut host_frame = frame(&input, &mut host_scratch, &mut host_out);
        let mut stencil_out = [0u8; 32];
        let mut stencil_scratch = vec![0u8; (case.len / 1024).max(1) * 32];
        let mut stencil_frame = frame(&input, &mut stencil_scratch, &mut stencil_out);

        for _ in 0..128 {
            native_hash_into(black_box(input.as_slice()), black_box(&mut native_out));
            unsafe { host.run(black_box(&mut host_frame)) };
            stencil.run(black_box(&mut stencil_frame));
        }

        let mut native_samples = Vec::new();
        let mut host_samples = Vec::new();
        let mut stencil_samples = Vec::new();
        for round in 0..9 {
            match round % 3 {
                0 => {
                    native_samples.push(sample(
                        || {
                            native_hash_into(
                                black_box(input.as_slice()),
                                black_box(&mut native_out),
                            );
                            black_box(native_out[0]);
                        },
                        case.iterations,
                    ));
                    host_samples.push(sample(
                        || {
                            unsafe { host.run(black_box(&mut host_frame)) };
                            black_box(host_out[0]);
                        },
                        case.iterations,
                    ));
                    stencil_samples.push(sample(
                        || {
                            stencil.run(black_box(&mut stencil_frame));
                            black_box(stencil_out[0]);
                        },
                        case.iterations,
                    ));
                }
                1 => {
                    host_samples.push(sample(
                        || {
                            unsafe { host.run(black_box(&mut host_frame)) };
                            black_box(host_out[0]);
                        },
                        case.iterations,
                    ));
                    stencil_samples.push(sample(
                        || {
                            stencil.run(black_box(&mut stencil_frame));
                            black_box(stencil_out[0]);
                        },
                        case.iterations,
                    ));
                    native_samples.push(sample(
                        || {
                            native_hash_into(
                                black_box(input.as_slice()),
                                black_box(&mut native_out),
                            );
                            black_box(native_out[0]);
                        },
                        case.iterations,
                    ));
                }
                _ => {
                    stencil_samples.push(sample(
                        || {
                            stencil.run(black_box(&mut stencil_frame));
                            black_box(stencil_out[0]);
                        },
                        case.iterations,
                    ));
                    native_samples.push(sample(
                        || {
                            native_hash_into(
                                black_box(input.as_slice()),
                                black_box(&mut native_out),
                            );
                            black_box(native_out[0]);
                        },
                        case.iterations,
                    ));
                    host_samples.push(sample(
                        || {
                            unsafe { host.run(black_box(&mut host_frame)) };
                            black_box(host_out[0]);
                        },
                        case.iterations,
                    ));
                }
            }
        }

        let native_ns = median_ns(&mut native_samples, case.iterations);
        let host_ns = median_ns(&mut host_samples, case.iterations);
        let stencil_ns = median_ns(&mut stencil_samples, case.iterations);

        Row {
            case,
            native_ns,
            host_ns,
            stencil_ns,
        }
    }

    fn gib_per_s(bytes: usize, ns: f64) -> f64 {
        bytes as f64 / ns / 1.073_741_824
    }

    pub fn main() {
        let stencil = StencilChain::new();
        let host = RawHostCallChain::new(vec![()], host_blake3);
        let empty_host = RawHostCallChain::new(vec![()], host_empty);
        let empty_native = EmptyNativeChain::new();

        let empty_iterations = 2_000_000;
        let empty_native_ns = ns_per_call(|| empty_native.run(), empty_iterations);
        let empty_host_ns = ns_per_call(
            || unsafe {
                let mut frame = [0u64; 4];
                empty_host.run(black_box(&mut frame));
            },
            empty_iterations,
        );

        let cases = [
            Case {
                label: "1 KiB",
                len: 1024,
                iterations: 50_000,
            },
            Case {
                label: "64 KiB",
                len: 64 * 1024,
                iterations: 5_000,
            },
            Case {
                label: "1 MiB",
                len: 1024 * 1024,
                iterations: 500,
            },
        ];
        let rows: Vec<_> = cases
            .into_iter()
            .map(|case| measure_case(case, &stencil, &host))
            .collect();

        println!("# BLAKE3 stencil spike");
        println!();
        println!("All paths validated against `blake3::hash` before timing.");
        println!();
        println!(
            "| size | native blake3 | host-call blake3 | stencil portable | host/native | stencil/host |"
        );
        println!("|---:|---:|---:|---:|---:|---:|");
        for row in &rows {
            println!(
                "| {} | {:.1} ns ({:.2} GiB/s) | {:.1} ns ({:.2} GiB/s) | {:.1} ns ({:.2} GiB/s) | {:.2}x | {:.2}x |",
                row.case.label,
                row.native_ns,
                gib_per_s(row.case.len, row.native_ns),
                row.host_ns,
                gib_per_s(row.case.len, row.host_ns),
                row.stencil_ns,
                gib_per_s(row.case.len, row.stencil_ns),
                row.host_ns / row.native_ns,
                row.stencil_ns / row.host_ns,
            );
        }
        println!();
        println!("## Per-call overhead");
        println!();
        println!("| component | ns/call |");
        println!("|---|---:|");
        println!("| empty copied-code entry | {:.1} |", empty_native_ns);
        println!("| empty host-call chain | {:.1} |", empty_host_ns);
        println!(
            "| host-call boundary delta | {:.1} |",
            empty_host_ns - empty_native_ns
        );
        for row in &rows {
            println!(
                "| {} host-call minus native | {:.1} |",
                row.case.label,
                row.host_ns - row.native_ns
            );
            println!(
                "| {} stencil minus native | {:.1} |",
                row.case.label,
                row.stencil_ns - row.native_ns
            );
        }
        println!();
        println!("## Gaps");
        println!();
        println!(
            "- The task JIT has no consumer extension point for custom native ops; this bench assembles a task-shaped `Ctx` chain directly instead of adding `Blake3Hash` to `weavy::task::Op`."
        );
        println!(
            "- The stencil can express BLAKE3 rotates directly via `u32::rotate_right`; no rotate IR gap appeared at the stencil/LLVM level."
        );
        println!(
            "- Static schedule tables would create non-continuation relocations; the stencil expands the seven message schedules into literal `g(...)` calls."
        );
        println!(
            "- Pointer-bearing frame slots work in the harness, but the typed task vocabulary has no pointer argument/capability story for making that a normal machine op."
        );
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn main() {
    native::main();
}

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
fn main() {
    println!("native copy-and-patch is unavailable on this target");
}
