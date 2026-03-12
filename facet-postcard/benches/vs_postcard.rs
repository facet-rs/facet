//! Benchmark comparing facet-postcard Tier-2 JIT vs reference postcard crate.
//!
//! This benchmark compares:
//! - Reference postcard crate (serde-based)
//! - facet-postcard with Tier-2 JIT convenience API (TLS-cached)
//! - facet-postcard with Tier-2 JIT compiled handle (no cache lookup)
//!
//! The Tier-2 JIT generates native code that parses postcard binary format directly,
//! without going through the event abstraction layer.

use divan::{Bencher, black_box};
use std::sync::LazyLock;

#[cfg(feature = "jit")]
use facet_format::jit::{self, CompiledFormatDeserializer};
#[cfg(feature = "jit")]
use facet_postcard::PostcardParser;

fn main() {
    divan::main();
}

// =============================================================================
// Vec<bool> - Simple case, no varint for elements
// =============================================================================

mod vec_bool {
    use super::*;

    fn make_data() -> Vec<bool> {
        (0..1000).map(|i| i % 3 != 0).collect()
    }

    static DATA: LazyLock<Vec<bool>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard::to_allocvec(&*DATA).unwrap());

    #[divan::bench]
    fn postcard_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(postcard::from_bytes::<Vec<bool>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<bool>>(black_box(data)).unwrap()));
    }
}

// =============================================================================
// Vec<u8> - Raw bytes: comparing bulk copy approaches at various sizes
// =============================================================================
//
// Wire format is identical for all paths (length-prefixed bytes).
// The difference is which serde API is used:
// - postcard_serde_bytes: deserialize_bytes() → bulk slice + memcpy
// - facet JIT:            emit_seq_bulk_copy_u8 hook → bulk memcpy
//
// Testing multiple sizes to understand fixed vs per-element overhead.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct BytesVec(#[serde(with = "serde_bytes")] Vec<u8>);

fn make_bytes(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 256) as u8).collect()
}

fn encode_bytes(n: usize) -> Vec<u8> {
    postcard::to_allocvec(&BytesVec(make_bytes(n))).unwrap()
}

mod vec_u8_empty {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(0));

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(postcard::from_bytes(black_box(data)).unwrap());
            black_box(w.0)
        });
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u8>>(black_box(data)).unwrap()));
    }

    /// Compiled handle benchmark - measures pure wrapper overhead
    #[cfg(feature = "jit")]
    #[divan::bench]
    fn facet_tier2_handle(bencher: Bencher) {
        let data = &*ENCODED;
        let handle: CompiledFormatDeserializer<Vec<u8>, PostcardParser> =
            jit::get_format_deserializer().expect("Vec<u8> should be Tier-2 compatible");

        bencher.bench(|| {
            let mut parser = PostcardParser::new(black_box(data));
            black_box(handle.deserialize(&mut parser).unwrap())
        });
    }
}

mod vec_u8_16 {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(16));

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(postcard::from_bytes(black_box(data)).unwrap());
            black_box(w.0)
        });
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u8>>(black_box(data)).unwrap()));
    }
}

mod vec_u8_256 {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(256));

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(postcard::from_bytes(black_box(data)).unwrap());
            black_box(w.0)
        });
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u8>>(black_box(data)).unwrap()));
    }
}

mod vec_u8_1k {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(1000));

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(postcard::from_bytes(black_box(data)).unwrap());
            black_box(w.0)
        });
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u8>>(black_box(data)).unwrap()));
    }
}

mod vec_u8_64k {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(65536));

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(postcard::from_bytes(black_box(data)).unwrap());
            black_box(w.0)
        });
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u8>>(black_box(data)).unwrap()));
    }
}

mod vec_u8_4m {
    use super::*;
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| encode_bytes(4 * 1024 * 1024));

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| {
            let w: BytesVec = black_box(postcard::from_bytes(black_box(data)).unwrap());
            black_box(w.0)
        });
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u8>>(black_box(data)).unwrap()));
    }
}

// =============================================================================
// Vec<u8> Serialization - Bulk copy vs element-by-element
// =============================================================================

mod vec_u8_serialize_256 {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct ByteData {
        data: Vec<u8>,
    }

    static DATA: LazyLock<ByteData> = LazyLock::new(|| ByteData {
        data: make_bytes(256),
    });

    #[derive(Serialize)]
    struct ByteDataSerde {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    }

    static DATA_SERDE: LazyLock<ByteDataSerde> = LazyLock::new(|| ByteDataSerde {
        data: make_bytes(256),
    });

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*DATA_SERDE;
        bencher.bench(|| black_box(postcard::to_allocvec(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_postcard(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_postcard::to_vec(black_box(data)).unwrap()));
    }
}

mod vec_u8_serialize_1k {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct ByteData {
        data: Vec<u8>,
    }

    static DATA: LazyLock<ByteData> = LazyLock::new(|| ByteData {
        data: make_bytes(1000),
    });

    #[derive(Serialize)]
    struct ByteDataSerde {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    }

    static DATA_SERDE: LazyLock<ByteDataSerde> = LazyLock::new(|| ByteDataSerde {
        data: make_bytes(1000),
    });

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*DATA_SERDE;
        bencher.bench(|| black_box(postcard::to_allocvec(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_postcard(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_postcard::to_vec(black_box(data)).unwrap()));
    }
}

mod vec_u8_serialize_64k {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct ByteData {
        data: Vec<u8>,
    }

    static DATA: LazyLock<ByteData> = LazyLock::new(|| ByteData {
        data: make_bytes(65536),
    });

    #[derive(Serialize)]
    struct ByteDataSerde {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    }

    static DATA_SERDE: LazyLock<ByteDataSerde> = LazyLock::new(|| ByteDataSerde {
        data: make_bytes(65536),
    });

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*DATA_SERDE;
        bencher.bench(|| black_box(postcard::to_allocvec(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_postcard(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_postcard::to_vec(black_box(data)).unwrap()));
    }
}

mod vec_u8_serialize_256k {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct ByteData {
        data: Vec<u8>,
    }

    static DATA: LazyLock<ByteData> = LazyLock::new(|| ByteData {
        data: make_bytes(256 * 1024),
    });

    #[derive(Serialize)]
    struct ByteDataSerde {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    }

    static DATA_SERDE: LazyLock<ByteDataSerde> = LazyLock::new(|| ByteDataSerde {
        data: make_bytes(256 * 1024),
    });

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*DATA_SERDE;
        bencher.bench(|| black_box(postcard::to_allocvec(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_postcard(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_postcard::to_vec(black_box(data)).unwrap()));
    }
}

mod vec_u8_serialize_1m {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct ByteData {
        data: Vec<u8>,
    }

    static DATA: LazyLock<ByteData> = LazyLock::new(|| ByteData {
        data: make_bytes(1024 * 1024),
    });

    #[derive(Serialize)]
    struct ByteDataSerde {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    }

    static DATA_SERDE: LazyLock<ByteDataSerde> = LazyLock::new(|| ByteDataSerde {
        data: make_bytes(1024 * 1024),
    });

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*DATA_SERDE;
        bencher.bench(|| black_box(postcard::to_allocvec(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_postcard(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_postcard::to_vec(black_box(data)).unwrap()));
    }
}

mod vec_u8_serialize_4m {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct ByteData {
        data: Vec<u8>,
    }

    static DATA: LazyLock<ByteData> = LazyLock::new(|| ByteData {
        data: make_bytes(4 * 1024 * 1024),
    });

    #[derive(Serialize)]
    struct ByteDataSerde {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    }

    static DATA_SERDE: LazyLock<ByteDataSerde> = LazyLock::new(|| ByteDataSerde {
        data: make_bytes(4 * 1024 * 1024),
    });

    #[divan::bench]
    fn postcard_serde_bytes(bencher: Bencher) {
        let data = &*DATA_SERDE;
        bencher.bench(|| black_box(postcard::to_allocvec(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_postcard(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_postcard::to_vec(black_box(data)).unwrap()));
    }
}

// =============================================================================
// Vec<u32> - Varint encoding
// =============================================================================

mod vec_u32 {
    use super::*;

    fn make_data() -> Vec<u32> {
        // Mix of small values (1-byte varint) and larger values (multi-byte varint)
        (0..1000)
            .map(|i| match i % 4 {
                0 => i as u32,             // small (1 byte)
                1 => (i as u32) * 100,     // medium (2 bytes)
                2 => (i as u32) * 10000,   // large (3 bytes)
                _ => (i as u32) * 1000000, // very large (4-5 bytes)
            })
            .collect()
    }

    static DATA: LazyLock<Vec<u32>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard::to_allocvec(&*DATA).unwrap());

    #[divan::bench]
    fn postcard_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(postcard::from_bytes::<Vec<u32>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u32>>(black_box(data)).unwrap()));
    }
}

// =============================================================================
// Vec<u64> - Varint encoding with potentially longer varints
// =============================================================================

mod vec_u64 {
    use super::*;

    fn make_data() -> Vec<u64> {
        (0..1000)
            .map(|i| match i % 5 {
                0 => i as u64,                  // small
                1 => (i as u64) * 1000,         // medium
                2 => (i as u64) * 1000000,      // large
                3 => (i as u64) * 1000000000,   // very large
                _ => u64::MAX / (i as u64 + 1), // huge
            })
            .collect()
    }

    static DATA: LazyLock<Vec<u64>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard::to_allocvec(&*DATA).unwrap());

    #[divan::bench]
    fn postcard_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(postcard::from_bytes::<Vec<u64>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u64>>(black_box(data)).unwrap()));
    }
}

// =============================================================================
// Vec<i32> - ZigZag + Varint encoding
// =============================================================================

mod vec_i32 {
    use super::*;

    fn make_data() -> Vec<i32> {
        // Mix of positive and negative values to exercise ZigZag encoding
        (0..1000)
            .map(|i| {
                let base = i * 100;
                if i % 2 == 0 { base } else { -base }
            })
            .collect()
    }

    static DATA: LazyLock<Vec<i32>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard::to_allocvec(&*DATA).unwrap());

    #[divan::bench]
    fn postcard_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(postcard::from_bytes::<Vec<i32>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<i32>>(black_box(data)).unwrap()));
    }
}

// =============================================================================
// Vec<i64> - ZigZag + Varint for large signed integers
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
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard::to_allocvec(&*DATA).unwrap());

    #[divan::bench]
    fn postcard_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(postcard::from_bytes::<Vec<i64>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<i64>>(black_box(data)).unwrap()));
    }
}

// =============================================================================
// Small Vec (10 elements) - measures overhead vs element processing
// =============================================================================

mod vec_u64_small {
    use super::*;

    fn make_data() -> Vec<u64> {
        (0..10).map(|i| i * 12345).collect()
    }

    static DATA: LazyLock<Vec<u64>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard::to_allocvec(&*DATA).unwrap());

    #[divan::bench]
    fn postcard_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(postcard::from_bytes::<Vec<u64>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u64>>(black_box(data)).unwrap()));
    }

    /// Compiled handle benchmark - no cache lookup at all
    #[cfg(feature = "jit")]
    #[divan::bench]
    fn facet_tier2_handle(bencher: Bencher) {
        let data = &*ENCODED;
        // Get handle once (outside the benchmark loop)
        let handle: CompiledFormatDeserializer<Vec<u64>, PostcardParser> =
            jit::get_format_deserializer().expect("Vec<u64> should be Tier-2 compatible");

        bencher.bench(|| {
            let mut parser = PostcardParser::new(black_box(data));
            black_box(handle.deserialize(&mut parser).unwrap())
        });
    }
}

// =============================================================================
// Large Vec (10000 elements) - measures throughput
// =============================================================================

mod vec_u64_large {
    use super::*;

    fn make_data() -> Vec<u64> {
        (0..10000).map(|i| i * 12345).collect()
    }

    static DATA: LazyLock<Vec<u64>> = LazyLock::new(make_data);
    static ENCODED: LazyLock<Vec<u8>> = LazyLock::new(|| postcard::to_allocvec(&*DATA).unwrap());

    #[divan::bench]
    fn postcard_serde(bencher: Bencher) {
        let data = &*ENCODED;
        bencher.bench(|| black_box(postcard::from_bytes::<Vec<u64>>(black_box(data)).unwrap()));
    }

    #[divan::bench]
    fn facet_tier2_jit(bencher: Bencher) {
        let data = &*ENCODED;
        bencher
            .bench(|| black_box(facet_postcard::from_slice::<Vec<u64>>(black_box(data)).unwrap()));
    }

    /// Compiled handle benchmark - measures throughput without cache overhead
    #[cfg(feature = "jit")]
    #[divan::bench]
    fn facet_tier2_handle(bencher: Bencher) {
        let data = &*ENCODED;
        let handle: CompiledFormatDeserializer<Vec<u64>, PostcardParser> =
            jit::get_format_deserializer().expect("Vec<u64> should be Tier-2 compatible");

        bencher.bench(|| {
            let mut parser = PostcardParser::new(black_box(data));
            black_box(handle.deserialize(&mut parser).unwrap())
        });
    }
}
