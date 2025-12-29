//! Benchmark comparing facet-msgpack Tier-2 JIT vs reference rmp-serde crate.
//!
//! This benchmark compares:
//! - Reference rmp-serde crate (serde-based MsgPack)
//! - facet-msgpack with Tier-2 JIT convenience API (TLS-cached)
//!
//! The Tier-2 JIT generates native code that parses MsgPack binary format directly,
//! without going through the event abstraction layer.

use divan::{Bencher, black_box};
use std::sync::LazyLock;

fn main() {
    divan::main();
}

// =============================================================================
// Vec<bool> - Simple case
// =============================================================================

mod vec_bool {
    use super::*;

    fn make_data() -> Vec<bool> {
        (0..1000).map(|i| i % 3 != 0).collect()
    }

    static DATA: LazyLock<Vec<bool>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| rmp_serde::to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn rmp_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(rmp_serde::from_slice::<Vec<bool>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            black_box(facet_msgpack::from_slice::<Vec<bool>>(black_box(data)).unwrap())
        });
    }
}

// =============================================================================
// Vec<u8> - Binary data: comparing bulk copy vs element-wise
// =============================================================================

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct BytesVec(#[serde(with = "serde_bytes")] Vec<u8>);

fn make_bytes(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 256) as u8).collect()
}

fn encode_bytes(n: usize) -> Vec<u8> {
    rmp_serde::to_vec(&BytesVec(make_bytes(n))).unwrap()
}

mod vec_u8_256 {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(256));

    #[divan::bench]
    fn rmp_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(rmp_serde::from_slice(black_box(data)).unwrap());
            black_box(w.0)
        });
    }

    // Note: facet Vec<u8> as bin requires special handling - not yet implemented
}

mod vec_u8_64k {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(65536));

    #[divan::bench]
    fn rmp_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(rmp_serde::from_slice(black_box(data)).unwrap());
            black_box(w.0)
        });
    }
}

// =============================================================================
// Vec<u64> - Unsigned integers
// =============================================================================

mod vec_u64 {
    use super::*;

    fn make_data() -> Vec<u64> {
        (0..1000)
            .map(|i| match i % 5 {
                0 => i as u64,                  // small (fixint)
                1 => (i as u64) * 1000,         // medium (u16)
                2 => (i as u64) * 1000000,      // large (u32)
                3 => (i as u64) * 1000000000,   // very large (u64)
                _ => u64::MAX / (i as u64 + 1), // huge
            })
            .collect()
    }

    static DATA: LazyLock<Vec<u64>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| rmp_serde::to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn rmp_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(rmp_serde::from_slice::<Vec<u64>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            black_box(facet_msgpack::from_slice::<Vec<u64>>(black_box(data)).unwrap())
        });
    }
}

// =============================================================================
// Vec<i64> - Signed integers
// =============================================================================

mod vec_i64 {
    use super::*;

    fn make_data() -> Vec<i64> {
        (0..1000)
            .map(|i| {
                let base = (i as i64) * 1000000;
                if i % 2 == 0 { base } else { -base }
            })
            .collect()
    }

    static DATA: LazyLock<Vec<i64>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| rmp_serde::to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn rmp_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(rmp_serde::from_slice::<Vec<i64>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            black_box(facet_msgpack::from_slice::<Vec<i64>>(black_box(data)).unwrap())
        });
    }
}

// =============================================================================
// Vec<u64> large - Throughput test
// =============================================================================

mod vec_u64_large {
    use super::*;

    fn make_data() -> Vec<u64> {
        (0..10000).map(|i| i * 12345).collect()
    }

    static DATA: LazyLock<Vec<u64>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| rmp_serde::to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn rmp_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(rmp_serde::from_slice::<Vec<u64>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            black_box(facet_msgpack::from_slice::<Vec<u64>>(black_box(data)).unwrap())
        });
    }
}
