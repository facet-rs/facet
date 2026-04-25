#[cfg(feature = "protobuf")]
pub mod pb {
    tonic::include_proto!("adder");
}

use facet::Facet;
use spec_proto::{GnarlyAttr, GnarlyEntry, GnarlyKind, GnarlyPayload};

pub fn jit_encode<T>(value: &T) -> Vec<u8>
where
    T: Facet<'static>,
{
    let ptr = facet::PtrConst::new((value as *const T).cast::<u8>());
    vox_jit::global_runtime()
        .try_encode_ptr(ptr, T::SHAPE)
        .expect("JIT encode unsupported")
        .expect("JIT encode failed")
}

pub fn jit_decode<T>(
    bytes: &[u8],
    plan: &vox_postcard::plan::TranslationPlan,
    registry: &vox_types::SchemaRegistry,
) -> T
where
    T: Facet<'static>,
{
    vox_jit::global_runtime()
        .try_decode_owned::<T>(bytes, 0, plan, registry)
        .expect("JIT decode unsupported")
        .expect("JIT decode failed")
}

pub fn make_gnarly_payload(entry_count: usize, seq: usize) -> GnarlyPayload {
    let entries = (0..entry_count)
        .map(|i| {
            let attrs = vec![
                GnarlyAttr {
                    key: "owner".to_string(),
                    value: format!("user-{seq}-{i}"),
                },
                GnarlyAttr {
                    key: "class".to_string(),
                    value: format!("hot-path-{}", (seq + i) % 17),
                },
                GnarlyAttr {
                    key: "etag".to_string(),
                    value: format!("etag-{seq:08x}-{i:08x}"),
                },
            ];
            let chunks = (0..3)
                .map(|j| {
                    let len = 32 * (j + 1);
                    vec![((seq + i + j) & 0xff) as u8; len]
                })
                .collect();
            let kind = match i % 3 {
                0 => GnarlyKind::File {
                    mime: "application/octet-stream".to_string(),
                    tags: vec![
                        "warm".to_string(),
                        "cacheable".to_string(),
                        format!("tag-{seq}-{i}"),
                    ],
                },
                1 => GnarlyKind::Directory {
                    child_count: i as u32 + 3,
                    children: vec![
                        format!("child-{seq}-{i}-0"),
                        format!("child-{seq}-{i}-1"),
                        format!("child-{seq}-{i}-2"),
                    ],
                },
                _ => GnarlyKind::Symlink {
                    target: format!("/target/{seq}/{i}/nested/item"),
                    hops: vec![1, 2, 3, i as u32],
                },
            };
            GnarlyEntry {
                id: seq as u64 * 1_000_000 + i as u64,
                parent: if i == 0 {
                    None
                } else {
                    Some(seq as u64 * 1_000_000 + i as u64 - 1)
                },
                name: format!("entry-{seq}-{i}"),
                path: format!("/mount/very/deep/path/with/component/{seq}/{i}/file.bin"),
                attrs,
                chunks,
                kind,
            }
        })
        .collect();

    GnarlyPayload {
        revision: seq as u64,
        mount: format!("/mnt/bench-fast-path-{seq:08x}"),
        entries,
        footer: Some(format!("benchmark footer {seq}")),
        digest: vec![(seq & 0xff) as u8; 64],
    }
}

/// Borrowed mirror of `spec_proto::GnarlyPayload` used by codec benches to
/// measure zero-copy decode (where every leaf string and byte slice points
/// directly into the input buffer instead of being heap-allocated). The wire
/// format matches the owned variant byte-for-byte (postcard encodes
/// `String`/`&str` and `Vec<u8>`/`&[u8]` identically), so the same encoded
/// bytes feed both decoders.
///
/// We **duplicate** the types here rather than parameterizing
/// `spec_proto::GnarlyPayload` over a lifetime — the spec types are the
/// authoritative shape and shouldn't carry bench-only lifetime baggage.
pub mod borrowed {
    use facet::Facet;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    pub struct GnarlyAttr<'a> {
        #[serde(borrow)]
        pub key: &'a str,
        #[serde(borrow)]
        pub value: &'a str,
    }

    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    pub enum GnarlyKind<'a> {
        File {
            #[serde(borrow)]
            mime: &'a str,
            #[serde(borrow)]
            tags: Vec<&'a str>,
        } = 0,
        Directory {
            child_count: u32,
            #[serde(borrow)]
            children: Vec<&'a str>,
        } = 1,
        Symlink {
            #[serde(borrow)]
            target: &'a str,
            hops: Vec<u32>,
        } = 2,
    }

    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    pub struct GnarlyEntry<'a> {
        pub id: u64,
        pub parent: Option<u64>,
        #[serde(borrow)]
        pub name: &'a str,
        #[serde(borrow)]
        pub path: &'a str,
        #[serde(borrow)]
        pub attrs: Vec<GnarlyAttr<'a>>,
        #[serde(borrow)]
        pub chunks: Vec<&'a [u8]>,
        #[serde(borrow)]
        pub kind: GnarlyKind<'a>,
    }

    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    pub struct GnarlyPayload<'a> {
        pub revision: u64,
        #[serde(borrow)]
        pub mount: &'a str,
        #[serde(borrow)]
        pub entries: Vec<GnarlyEntry<'a>>,
        #[serde(borrow)]
        pub footer: Option<&'a str>,
        #[serde(borrow)]
        pub digest: &'a [u8],
    }
}

/// Shape catalogue for diverse codec benches: each type stresses a different
/// axis of decode/encode that GnarlyPayload doesn't (which is dominated by
/// "many small allocations + lots of strings"). Each derives both `Facet` and
/// `serde::{Serialize, Deserialize}` from a single definition since vox-bench
/// owns the types — postcard's wire format is the same for both codecs.
pub mod shapes {
    use facet::Facet;
    use serde::{Deserialize, Serialize};

    /// 64 primitive fields. Tests per-field decode dispatch and the cost of
    /// emitting a long straight-line decoder. Zero allocations: pure varint /
    /// fixed-bytes reads. JIT and serde should both be very fast here; the
    /// gap (if any) reflects raw codegen quality on a flat-struct workload.
    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    pub struct WideStruct {
        // 16 × u32 (varint, 1-5 bytes each)
        pub a00: u32, pub a01: u32, pub a02: u32, pub a03: u32,
        pub a04: u32, pub a05: u32, pub a06: u32, pub a07: u32,
        pub a08: u32, pub a09: u32, pub a10: u32, pub a11: u32,
        pub a12: u32, pub a13: u32, pub a14: u32, pub a15: u32,
        // 16 × i64 (zigzag varint)
        pub b00: i64, pub b01: i64, pub b02: i64, pub b03: i64,
        pub b04: i64, pub b05: i64, pub b06: i64, pub b07: i64,
        pub b08: i64, pub b09: i64, pub b10: i64, pub b11: i64,
        pub b12: i64, pub b13: i64, pub b14: i64, pub b15: i64,
        // 16 × bool (1 byte each, validated)
        pub c00: bool, pub c01: bool, pub c02: bool, pub c03: bool,
        pub c04: bool, pub c05: bool, pub c06: bool, pub c07: bool,
        pub c08: bool, pub c09: bool, pub c10: bool, pub c11: bool,
        pub c12: bool, pub c13: bool, pub c14: bool, pub c15: bool,
        // 16 × u8 (1 byte raw)
        pub d00: u8, pub d01: u8, pub d02: u8, pub d03: u8,
        pub d04: u8, pub d05: u8, pub d06: u8, pub d07: u8,
        pub d08: u8, pub d09: u8, pub d10: u8, pub d11: u8,
        pub d12: u8, pub d13: u8, pub d14: u8, pub d15: u8,
    }

    pub fn make_wide(seq: u32) -> WideStruct {
        let s = seq as i64;
        WideStruct {
            a00: seq.wrapping_mul(1), a01: seq.wrapping_mul(3), a02: seq.wrapping_mul(7), a03: seq.wrapping_mul(11),
            a04: seq.wrapping_mul(13), a05: seq.wrapping_mul(17), a06: seq.wrapping_mul(19), a07: seq.wrapping_mul(23),
            a08: seq.wrapping_mul(29), a09: seq.wrapping_mul(31), a10: seq.wrapping_mul(37), a11: seq.wrapping_mul(41),
            a12: seq.wrapping_mul(43), a13: seq.wrapping_mul(47), a14: seq.wrapping_mul(53), a15: seq.wrapping_mul(59),
            b00: s, b01: -s, b02: s.wrapping_mul(2), b03: -s.wrapping_mul(3),
            b04: s.wrapping_mul(5), b05: -s.wrapping_mul(7), b06: s.wrapping_mul(11), b07: -s.wrapping_mul(13),
            b08: s.wrapping_mul(17), b09: -s.wrapping_mul(19), b10: s.wrapping_mul(23), b11: -s.wrapping_mul(29),
            b12: s.wrapping_mul(31), b13: -s.wrapping_mul(37), b14: s.wrapping_mul(41), b15: -s.wrapping_mul(43),
            c00: seq & 1 == 0, c01: seq & 2 != 0, c02: seq & 4 != 0, c03: seq & 8 != 0,
            c04: seq & 16 != 0, c05: seq & 32 != 0, c06: seq & 64 != 0, c07: seq & 128 != 0,
            c08: true, c09: false, c10: true, c11: false,
            c12: true, c13: true, c14: false, c15: false,
            d00: seq as u8, d01: (seq >> 8) as u8, d02: (seq >> 16) as u8, d03: (seq >> 24) as u8,
            d04: 0, d05: 0xFF, d06: 0x42, d07: 0xA5,
            d08: seq as u8, d09: (seq >> 8) as u8, d10: (seq >> 16) as u8, d11: (seq >> 24) as u8,
            d12: 1, d13: 2, d14: 3, d15: 4,
        }
    }

    /// 16 variants with diverse primitive payloads. Tests enum dispatch (the
    /// varint variant-index read + branch-on-variant pattern) at scale. Each
    /// variant has a different shape so the per-variant decode body is also
    /// non-trivial. Constructor cycles through variants by `seq % 16`.
    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    pub enum ManyVariants {
        V00 = 0,
        V01(u8) = 1,
        V02(u32) = 2,
        V03(u64) = 3,
        V04(i32) = 4,
        V05(bool) = 5,
        V06(f32) = 6,
        V07(f64) = 7,
        V08 { a: u32, b: u32 } = 8,
        V09 { x: u64, y: u64, z: u64 } = 9,
        V10(u8, u8, u8, u8) = 10,
        V11(u32, u32, u32, u32) = 11,
        V12(u64, u64) = 12,
        V13(i64) = 13,
        V14(u16, u16, u16, u16) = 14,
        V15(u8) = 15,
    }

    pub fn make_many_variants(seq: u32) -> ManyVariants {
        match seq % 16 {
            0 => ManyVariants::V00,
            1 => ManyVariants::V01(seq as u8),
            2 => ManyVariants::V02(seq.wrapping_mul(7)),
            3 => ManyVariants::V03(seq as u64 * 1_000_003),
            4 => ManyVariants::V04(-(seq as i32)),
            5 => ManyVariants::V05(seq & 1 == 0),
            6 => ManyVariants::V06(seq as f32 * 1.5),
            7 => ManyVariants::V07(seq as f64 / 7.0),
            8 => ManyVariants::V08 { a: seq, b: seq.wrapping_add(1) },
            9 => ManyVariants::V09 { x: seq as u64, y: seq as u64 + 1, z: seq as u64 + 2 },
            10 => ManyVariants::V10(seq as u8, (seq >> 8) as u8, (seq >> 16) as u8, (seq >> 24) as u8),
            11 => ManyVariants::V11(seq, seq.wrapping_mul(2), seq.wrapping_mul(3), seq.wrapping_mul(4)),
            12 => ManyVariants::V12(seq as u64, seq as u64 * 2),
            13 => ManyVariants::V13(-(seq as i64) * 7),
            14 => ManyVariants::V14(seq as u16, (seq >> 16) as u16, (seq + 1) as u16, (seq + 2) as u16),
            _ => ManyVariants::V15(0xAA),
        }
    }

    /// Recursive binary tree. Tests JIT recursion + heavy `Box<T>` allocation
    /// patterns. A balanced tree of depth d has 2^d leaves and 2^d - 1 internal
    /// nodes, requiring 2*(2^d - 1) Box allocations to decode. Good stress for
    /// the allocator and for the JIT's nested-decode call discipline.
    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    #[repr(u8)]
    pub enum Tree {
        Leaf(u64) = 0,
        Node(Box<Tree>, Box<Tree>) = 1,
    }

    pub fn make_tree(depth: u32, seq: u64) -> Tree {
        if depth == 0 {
            Tree::Leaf(seq)
        } else {
            Tree::Node(
                Box::new(make_tree(depth - 1, seq.wrapping_mul(2))),
                Box::new(make_tree(depth - 1, seq.wrapping_mul(2).wrapping_add(1))),
            )
        }
    }

    /// Numerical buffer: large `Vec<f32>` + `Vec<f64>` payloads, no strings.
    /// Audio-sample-shaped workload. f32/f64 are postcard-encoded as fixed
    /// 4/8 little-endian bytes (NOT varint), so on LE the wire layout matches
    /// the in-memory `[f32]` / `[f64]` slice byte-for-byte. The JIT can
    /// memcpy these directly (alignment guaranteed by the allocator); current
    /// code falls back to a per-element loop, leaving wins on the table.
    #[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
    pub struct NumericBuffer {
        pub sample_rate: u32,
        pub channels: u32,
        pub mono_f32: Vec<f32>,
        pub mono_f64: Vec<f64>,
        pub flags: Vec<bool>,
    }

    pub fn make_numeric_buffer(n: usize, seq: u32) -> NumericBuffer {
        let mono_f32 = (0..n)
            .map(|i| (seq.wrapping_add(i as u32) as f32 / 1024.0).sin())
            .collect();
        let mono_f64 = (0..n)
            .map(|i| (seq.wrapping_add(i as u32) as f64 / 4096.0).cos())
            .collect();
        let flags = (0..n).map(|i| (seq.wrapping_add(i as u32)) & 1 == 0).collect();
        NumericBuffer {
            sample_rate: 48_000,
            channels: 2,
            mono_f32,
            mono_f64,
            flags,
        }
    }
}
