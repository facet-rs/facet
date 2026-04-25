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
