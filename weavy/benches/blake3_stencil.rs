use std::hint::black_box;
use std::time::{Duration, Instant};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "../stencils/blake3_neon_core.rs"]
mod blake3_neon_core;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod native {
    use super::*;
    use crate::blake3_neon_core;
    use blake3::hazmat::{Mode, merge_subtrees_non_root};
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
    struct SmallCase {
        label: &'static str,
        len: usize,
        iterations: usize,
    }

    #[derive(Clone, Copy)]
    struct BatchCase {
        label: &'static str,
        len: usize,
        count: usize,
        iterations: usize,
    }

    struct StencilChain {
        native: NativeProgram,
    }

    impl StencilChain {
        fn new(stencil: &[u8], conts: &[usize], prog_words: &[u64]) -> Self {
            let mut layout = StencilLayout::new();
            let root = layout.start_chain();
            for &word in prog_words {
                layout.push_prog_word(root.prog_index, word);
            }
            let op = layout.emit_stencil(stencil);
            let done = layout.emit_stencil(stencils::DONE);
            for &rel in conts {
                layout.patch_continuation(op + rel, done);
            }
            Self {
                native: NativeProgram::new(layout, root),
            }
        }

        fn run<T>(&self, frame: &mut T) {
            let mut resume = 0;
            let mut await_index = 0;
            let mut exit = 0;
            let mut ctx = Ctx {
                prog: self.native.entry_prog(),
                frame: core::ptr::from_mut(frame).cast(),
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
            Self {
                native: NativeProgram::new(layout, root),
            }
        }

        fn run(&self) {
            let mut frame = [0u64; 1];
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

    unsafe extern "C" fn host_hash_small(cx: *mut (), _info: *const ()) -> bool {
        let frame = unsafe { &mut *cx.cast::<[u64; 3]>() };
        let input =
            unsafe { core::slice::from_raw_parts(frame[0] as *const u8, frame[1] as usize) };
        let out = frame[2] as *mut u8;
        let hash = blake3::hash(input);
        unsafe { core::ptr::copy_nonoverlapping(hash.as_bytes().as_ptr(), out, 32) };
        true
    }

    unsafe extern "C" fn host_fold_parent(cx: *mut (), _info: *const ()) -> bool {
        let frame = unsafe { &mut *cx.cast::<[u64; 3]>() };
        let left = unsafe { &*(frame[0] as *const [u8; 32]) };
        let right = unsafe { &*(frame[1] as *const [u8; 32]) };
        let out = frame[2] as *mut u8;
        let cv = fold_native(left, right);
        unsafe { core::ptr::copy_nonoverlapping(cv.as_ptr(), out, 32) };
        true
    }

    unsafe extern "C" fn host_empty(_cx: *mut (), _info: *const ()) -> bool {
        true
    }

    fn fold_native(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        merge_subtrees_non_root(left, right, Mode::Hash)
    }

    fn native_hash_into(input: &[u8], out: &mut [u8; 32]) {
        let hash = blake3::hash(input);
        out.copy_from_slice(hash.as_bytes());
    }

    fn inline_hash_into(input: &[u8], out: &mut [u8; 32]) {
        unsafe {
            blake3_neon_core::hash_small_neon(input.as_ptr(), input.len(), out.as_mut_ptr());
        }
    }

    fn inline_fold_into(left: &[u8; 32], right: &[u8; 32], out: &mut [u8; 32]) {
        unsafe {
            blake3_neon_core::fold_parent_neon(left.as_ptr(), right.as_ptr(), out.as_mut_ptr());
        }
    }

    fn inline_batch_into(input: &[u8], len: usize, count: usize, out: &mut [u8]) {
        unsafe {
            blake3_neon_core::hash_batch_neon(input.as_ptr(), len, count, len, out.as_mut_ptr());
        }
    }

    fn input(len: usize, salt: usize) -> Vec<u8> {
        (0..len)
            .map(|i| {
                (i as u8)
                    .wrapping_mul(31)
                    .wrapping_add((i >> 8) as u8)
                    .wrapping_add(salt as u8)
            })
            .collect()
    }

    fn batch_input(len: usize, count: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(len * count);
        for i in 0..count {
            bytes.extend_from_slice(&input(len, i));
        }
        bytes
    }

    fn small_frame(input: &[u8], out: &mut [u8; 32]) -> [u64; 3] {
        [
            input.as_ptr() as u64,
            input.len() as u64,
            out.as_mut_ptr() as u64,
        ]
    }

    fn fold_frame(left: &[u8; 32], right: &[u8; 32], out: &mut [u8; 32]) -> [u64; 3] {
        [
            left.as_ptr() as u64,
            right.as_ptr() as u64,
            out.as_mut_ptr() as u64,
        ]
    }

    fn batch_frame(
        input: &[u8],
        len: usize,
        count: usize,
        stride: usize,
        out: &mut [u8],
    ) -> [u64; 5] {
        [
            input.as_ptr() as u64,
            len as u64,
            count as u64,
            stride as u64,
            out.as_mut_ptr() as u64,
        ]
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

    fn measure4(
        mut a: impl FnMut(),
        mut b: impl FnMut(),
        mut c: impl FnMut(),
        mut d: impl FnMut(),
        iterations: usize,
    ) -> [f64; 4] {
        for _ in 0..256 {
            a();
            b();
            c();
            d();
        }
        let mut as_ = Vec::new();
        let mut bs = Vec::new();
        let mut cs = Vec::new();
        let mut ds = Vec::new();
        for round in 0..12 {
            match round % 4 {
                0 => {
                    as_.push(sample(&mut a, iterations));
                    bs.push(sample(&mut b, iterations));
                    cs.push(sample(&mut c, iterations));
                    ds.push(sample(&mut d, iterations));
                }
                1 => {
                    bs.push(sample(&mut b, iterations));
                    cs.push(sample(&mut c, iterations));
                    ds.push(sample(&mut d, iterations));
                    as_.push(sample(&mut a, iterations));
                }
                2 => {
                    cs.push(sample(&mut c, iterations));
                    ds.push(sample(&mut d, iterations));
                    as_.push(sample(&mut a, iterations));
                    bs.push(sample(&mut b, iterations));
                }
                _ => {
                    ds.push(sample(&mut d, iterations));
                    as_.push(sample(&mut a, iterations));
                    bs.push(sample(&mut b, iterations));
                    cs.push(sample(&mut c, iterations));
                }
            }
        }
        [
            median_ns(&mut as_, iterations),
            median_ns(&mut bs, iterations),
            median_ns(&mut cs, iterations),
            median_ns(&mut ds, iterations),
        ]
    }

    fn ns_per_call(mut f: impl FnMut(), iterations: usize) -> f64 {
        for _ in 0..256 {
            f();
        }
        let mut samples = Vec::new();
        for _ in 0..9 {
            samples.push(sample(&mut f, iterations));
        }
        median_ns(&mut samples, iterations)
    }

    fn gib_per_s(bytes: usize, ns: f64) -> f64 {
        bytes as f64 / ns / 1.073_741_824
    }

    fn assert_small_matches(input: &[u8], stencil: &StencilChain, host: &RawHostCallChain<()>) {
        let expected = blake3::hash(input);
        let mut host_out = [0u8; 32];
        let mut host_frame = small_frame(input, &mut host_out);
        unsafe { host.run(&mut host_frame) };
        assert_eq!(host_out, *expected.as_bytes(), "host small hash drift");

        let mut stencil_out = [0u8; 32];
        let mut stencil_frame = small_frame(input, &mut stencil_out);
        stencil.run(&mut stencil_frame);
        assert_eq!(
            stencil_out,
            *expected.as_bytes(),
            "stencil small hash drift"
        );

        let mut inline_out = [0u8; 32];
        inline_hash_into(input, &mut inline_out);
        assert_eq!(inline_out, *expected.as_bytes(), "inline small hash drift");
    }

    fn assert_fold_matches(
        left: &[u8; 32],
        right: &[u8; 32],
        stencil: &StencilChain,
        host: &RawHostCallChain<()>,
    ) {
        let expected = fold_native(left, right);
        let mut host_out = [0u8; 32];
        let mut host_frame = fold_frame(left, right, &mut host_out);
        unsafe { host.run(&mut host_frame) };
        assert_eq!(host_out, expected, "host fold drift");

        let mut stencil_out = [0u8; 32];
        let mut stencil_frame = fold_frame(left, right, &mut stencil_out);
        stencil.run(&mut stencil_frame);
        assert_eq!(stencil_out, expected, "stencil fold drift");

        let mut inline_out = [0u8; 32];
        inline_fold_into(left, right, &mut inline_out);
        assert_eq!(inline_out, expected, "inline fold drift");
    }

    fn assert_batch_matches(input: &[u8], case: BatchCase, stencil: &StencilChain) {
        let mut expected = vec![0u8; case.count * 32];
        for i in 0..case.count {
            let hash = blake3::hash(&input[i * case.len..(i + 1) * case.len]);
            expected[i * 32..(i + 1) * 32].copy_from_slice(hash.as_bytes());
        }

        let mut stencil_out = vec![0u8; case.count * 32];
        let mut stencil_frame =
            batch_frame(input, case.len, case.count, case.len, &mut stencil_out);
        stencil.run(&mut stencil_frame);
        assert_eq!(stencil_out, expected, "stencil batch drift");

        let mut inline_out = vec![0u8; case.count * 32];
        inline_batch_into(input, case.len, case.count, &mut inline_out);
        assert_eq!(inline_out, expected, "inline batch drift");
    }

    fn measure_small(
        case: SmallCase,
        stencil: &StencilChain,
        host: &RawHostCallChain<()>,
    ) -> [f64; 4] {
        let input = input(case.len, 7);
        assert_small_matches(&input, stencil, host);

        let mut native_out = [0u8; 32];
        let mut host_out = [0u8; 32];
        let mut host_frame = small_frame(&input, &mut host_out);
        let mut stencil_out = [0u8; 32];
        let mut stencil_frame = small_frame(&input, &mut stencil_out);
        let mut inline_out = [0u8; 32];

        measure4(
            || {
                unsafe { host.run(black_box(&mut host_frame)) };
                black_box(host_out[0]);
            },
            || {
                stencil.run(black_box(&mut stencil_frame));
                black_box(stencil_out[0]);
            },
            || {
                native_hash_into(black_box(input.as_slice()), black_box(&mut native_out));
                black_box(native_out[0]);
            },
            || {
                inline_hash_into(black_box(input.as_slice()), black_box(&mut inline_out));
                black_box(inline_out[0]);
            },
            case.iterations,
        )
    }

    fn measure_fold(stencil: &StencilChain, host: &RawHostCallChain<()>) -> [f64; 4] {
        let left = *blake3::hash(&input(64, 1)).as_bytes();
        let right = *blake3::hash(&input(64, 2)).as_bytes();
        assert_fold_matches(&left, &right, stencil, host);

        let mut host_out = [0u8; 32];
        let mut host_frame = fold_frame(&left, &right, &mut host_out);
        let mut stencil_out = [0u8; 32];
        let mut stencil_frame = fold_frame(&left, &right, &mut stencil_out);
        let mut native_out = [0u8; 32];
        let mut inline_out = [0u8; 32];

        measure4(
            || {
                unsafe { host.run(black_box(&mut host_frame)) };
                black_box(host_out[0]);
            },
            || {
                stencil.run(black_box(&mut stencil_frame));
                black_box(stencil_out[0]);
            },
            || {
                native_out = fold_native(black_box(&left), black_box(&right));
                black_box(native_out[0]);
            },
            || {
                inline_fold_into(
                    black_box(&left),
                    black_box(&right),
                    black_box(&mut inline_out),
                );
                black_box(inline_out[0]);
            },
            500_000,
        )
    }

    fn measure_batch(
        case: BatchCase,
        small_host: &RawHostCallChain<()>,
        stencil: &StencilChain,
    ) -> [f64; 4] {
        let input = batch_input(case.len, case.count);
        assert_batch_matches(&input, case, stencil);

        let mut host_out = vec![[0u8; 32]; case.count];
        let mut host_frames: Vec<[u64; 3]> = (0..case.count)
            .map(|i| small_frame(&input[i * case.len..(i + 1) * case.len], &mut host_out[i]))
            .collect();
        let mut stencil_out = vec![0u8; case.count * 32];
        let mut stencil_frame =
            batch_frame(&input, case.len, case.count, case.len, &mut stencil_out);
        let mut native_out = vec![[0u8; 32]; case.count];
        let mut inline_out = vec![0u8; case.count * 32];

        let per_batch = measure4(
            || {
                for frame in &mut host_frames {
                    unsafe { small_host.run(black_box(frame)) };
                }
                black_box(host_out[0][0]);
            },
            || {
                stencil.run(black_box(&mut stencil_frame));
                black_box(stencil_out[0]);
            },
            || {
                for i in 0..case.count {
                    native_hash_into(
                        black_box(&input[i * case.len..(i + 1) * case.len]),
                        black_box(&mut native_out[i]),
                    );
                }
                black_box(native_out[0][0]);
            },
            || {
                inline_batch_into(
                    black_box(input.as_slice()),
                    case.len,
                    case.count,
                    black_box(&mut inline_out),
                );
                black_box(inline_out[0]);
            },
            case.iterations,
        );
        [
            per_batch[0] / case.count as f64,
            per_batch[1] / case.count as f64,
            per_batch[2] / case.count as f64,
            per_batch[3] / case.count as f64,
        ]
    }

    pub fn main() {
        let small_stencil = StencilChain::new(
            blake3_stencils::HASH_SMALL,
            blake3_stencils::HASH_SMALL_CONT,
            &[0, 8, 16],
        );
        let fold_stencil = StencilChain::new(
            blake3_stencils::FOLD_PARENT,
            blake3_stencils::FOLD_PARENT_CONT,
            &[0, 8, 16],
        );
        let batch_stencil = StencilChain::new(
            blake3_stencils::HASH_BATCH,
            blake3_stencils::HASH_BATCH_CONT,
            &[0, 8, 16, 24, 32],
        );
        let host_small = RawHostCallChain::new(vec![()], host_hash_small);
        let host_fold = RawHostCallChain::new(vec![()], host_fold_parent);
        let host_empty = RawHostCallChain::new(vec![()], host_empty);
        let empty_native = EmptyNativeChain::new();

        let empty_iterations = 2_000_000;
        let empty_native_ns = ns_per_call(|| empty_native.run(), empty_iterations);
        let empty_host_ns = ns_per_call(
            || unsafe {
                let mut frame = [0u64; 1];
                host_empty.run(black_box(&mut frame));
            },
            empty_iterations,
        );

        let small_cases = [
            SmallCase {
                label: "32 B",
                len: 32,
                iterations: 250_000,
            },
            SmallCase {
                label: "128 B",
                len: 128,
                iterations: 200_000,
            },
            SmallCase {
                label: "256 B",
                len: 256,
                iterations: 150_000,
            },
            SmallCase {
                label: "1 KiB",
                len: 1024,
                iterations: 50_000,
            },
        ];

        let batch_cases = [
            BatchCase {
                label: "32 B x256",
                len: 32,
                count: 256,
                iterations: 5_000,
            },
            BatchCase {
                label: "128 B x256",
                len: 128,
                count: 256,
                iterations: 3_000,
            },
            BatchCase {
                label: "256 B x256",
                len: 256,
                count: 256,
                iterations: 2_000,
            },
            BatchCase {
                label: "1 KiB x256",
                len: 1024,
                count: 256,
                iterations: 500,
            },
        ];

        println!("# BLAKE3 NEON stencil spike");
        println!();
        println!(
            "All stencil and inline outputs are validated against the `blake3` crate before timing."
        );
        println!();
        println!("## Single intern value, one hasher init per value");
        println!();
        println!(
            "| size | host-call crate incl init | stencil NEON | native crate ceiling | inline NEON no boundary | stencil/host | inline/host |"
        );
        println!("|---:|---:|---:|---:|---:|---:|---:|");
        let mut small_rows = Vec::new();
        for case in small_cases {
            let row = measure_small(case, &small_stencil, &host_small);
            small_rows.push((case, row));
            println!(
                "| {} | {:.1} ns ({:.2} GiB/s) | {:.1} ns ({:.2} GiB/s) | {:.1} ns ({:.2} GiB/s) | {:.1} ns ({:.2} GiB/s) | {:.2}x | {:.2}x |",
                case.label,
                row[0],
                gib_per_s(case.len, row[0]),
                row[1],
                gib_per_s(case.len, row[1]),
                row[2],
                gib_per_s(case.len, row[2]),
                row[3],
                gib_per_s(case.len, row[3]),
                row[1] / row[0],
                row[3] / row[0],
            );
        }

        println!();
        println!("## Carried fold, one parent compression");
        println!();
        let fold = measure_fold(&fold_stencil, &host_fold);
        println!(
            "| op | host-call crate | stencil NEON | native crate ceiling | inline NEON no boundary | stencil/host | inline/host |"
        );
        println!("|---|---:|---:|---:|---:|---:|---:|");
        println!(
            "| parent fold | {:.1} ns | {:.1} ns | {:.1} ns | {:.1} ns | {:.2}x | {:.2}x |",
            fold[0],
            fold[1],
            fold[2],
            fold[3],
            fold[1] / fold[0],
            fold[3] / fold[0],
        );

        println!();
        println!("## Batched cache-resident values");
        println!();
        println!(
            "| batch | host-call crate per value | stencil NEON per value | native crate per value | inline NEON per value | stencil/host | inline/host |"
        );
        println!("|---:|---:|---:|---:|---:|---:|---:|");
        let mut batch_rows = Vec::new();
        for case in batch_cases {
            let row = measure_batch(case, &host_small, &batch_stencil);
            batch_rows.push((case, row));
            println!(
                "| {} | {:.1} ns | {:.1} ns | {:.1} ns | {:.1} ns | {:.2}x | {:.2}x |",
                case.label,
                row[0],
                row[1],
                row[2],
                row[3],
                row[1] / row[0],
                row[3] / row[0],
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
        for (case, row) in small_rows {
            println!(
                "| {} host-call minus native crate | {:.1} |",
                case.label,
                row[0] - row[2]
            );
            println!(
                "| {} stencil entry minus inline NEON | {:.1} |",
                case.label,
                row[1] - row[3]
            );
        }
        println!(
            "| fold host-call minus native crate | {:.1} |",
            fold[0] - fold[2]
        );
        println!(
            "| fold stencil entry minus inline NEON | {:.1} |",
            fold[1] - fold[3]
        );

        println!();
        println!("## Gaps");
        println!();
        println!(
            "- `blake3` 1.8.5 does not ship a single-compression NEON implementation; its C NEON file has `TODO: compress_neon` and falls back to portable for `hash_one_neon`. The stencil implements the missing one-block NEON compression directly."
        );
        println!(
            "- The stencil compiler still rejects non-continuation relocations, so the message schedule is inline literal calls and the helper is `#[inline(always)]`-friendly."
        );
        println!(
            "- NEON rotate support is expressible with shifts/or in `core::arch::aarch64`; no rotate IR blocker appeared."
        );
        println!(
            "- The task vocabulary still has no normal pointer-bearing hash op ABI; this bench enters copied code with a task-shaped `Ctx` and raw pointer frame words."
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() {
    native::main();
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    println!("BLAKE3 NEON stencil spike requires macOS/aarch64 in this harness");
}
