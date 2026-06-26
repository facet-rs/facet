use core::hash::Hasher;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use facet::Facet;
use facet_hash::HashPlan;
#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
use facet_hash::NativeHashPlan;
use weavy::ir::IntrinsicDescriptor;

#[derive(Clone, Debug, Facet)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, Facet)]
struct Person {
    name: String,
    age: u32,
    email: Option<String>,
    scores: Vec<i32>,
}

#[derive(Clone, Debug, Facet)]
struct Floaty {
    x: f64,
    y: f32,
}

#[derive(Clone, Debug, Facet)]
struct TextScalars {
    owned: String,
    borrowed: &'static str,
    cow: Cow<'static, str>,
}

#[derive(Clone, Debug, Facet)]
struct PointArray {
    points: [Point; 2],
    tail: i16,
}

#[derive(Clone, Debug, Facet)]
struct Scores {
    values: Vec<i32>,
}

#[derive(Clone, Debug, Facet)]
struct Collections {
    set: BTreeSet<u32>,
    map: BTreeMap<String, u16>,
}

#[derive(Clone, Debug, Facet, PartialEq, Eq, PartialOrd, Ord)]
struct MapKey {
    id: u32,
}

#[derive(Clone, Debug, Facet)]
struct ProgramKeyMap {
    map: BTreeMap<MapKey, u16>,
}

#[derive(Clone, Debug, Facet)]
struct MixedScalarRuns {
    a: u32,
    point: Point,
    b: u32,
    c: u32,
}

#[derive(Clone, Debug, Facet)]
struct Nested {
    people: Vec<Person>,
    points: [Point; 2],
    label: Box<str>,
    maybe: Result<Option<u16>, String>,
}

#[derive(Clone, Debug, Facet)]
struct Node {
    value: u32,
    next: Option<Box<Node>>,
}

#[derive(Default)]
struct RecordingHasher {
    bytes: Vec<u8>,
}

impl Hasher for RecordingHasher {
    fn finish(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn write(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn write_u8(&mut self, i: u8) {
        self.bytes.push(i);
    }

    fn write_u16(&mut self, i: u16) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_u32(&mut self, i: u32) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_u64(&mut self, i: u64) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_u128(&mut self, i: u128) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_usize(&mut self, i: usize) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_i8(&mut self, i: i8) {
        self.bytes.push(i as u8);
    }

    fn write_i16(&mut self, i: i16) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_i32(&mut self, i: i32) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_i64(&mut self, i: i64) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_i128(&mut self, i: i128) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }

    fn write_isize(&mut self, i: isize) {
        self.bytes.extend_from_slice(&i.to_ne_bytes());
    }
}

#[test]
fn repeated_hashing_reuses_the_same_plan() {
    let plan = HashPlan::<Point>::build().unwrap();
    let value = Point { x: 10, y: -4 };

    let first = plan.hash64(&value).unwrap();
    let second = plan.hash64(&value).unwrap();

    assert_eq!(first, second);
}

#[test]
fn scalar_struct_fields_hash_without_child_frames() {
    let plan = HashPlan::<Point>::build().unwrap();
    let value = Point { x: 10, y: -4 };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    let stats = plan.hash_with_stats(&value, &mut hasher).unwrap();

    assert_eq!(stats.step_count, 1);
    assert_eq!(stats.inline_call_count, 0);
    assert_eq!(stats.continuation_resume_count, 0);
    assert_eq!(stats.max_frame_depth, 1);
}

#[test]
fn scalar_list_elements_hash_without_element_frames() {
    let plan = HashPlan::<Scores>::build().unwrap();
    let value = Scores {
        values: vec![1, 1, 2, 3, 5, 8, 13],
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    let stats = plan.hash_with_stats(&value, &mut hasher).unwrap();

    assert_eq!(stats.step_count, 2);
    assert_eq!(stats.inline_call_count, 1);
    assert_eq!(stats.continuation_resume_count, 1);
    assert_eq!(stats.max_frame_depth, 2);
}

#[test]
fn byte_vec_value_plan_hashes_bulk_bytes() {
    let plan = HashPlan::<Vec<u8>>::build().unwrap();
    let value = vec![0, 1, 2, 3, 5, 8, 13, 255];
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    facet_hash::hash_bytes_into(&value, &mut expected);
    plan.hash(&value, &mut actual).unwrap();

    assert_eq!(actual.bytes, expected.bytes);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let stats = plan.hash_with_stats(&value, &mut hasher).unwrap();
    let analysis = plan.analysis();

    assert_eq!(stats.step_count, 1);
    assert_eq!(stats.inline_call_count, 0);
    assert_eq!(stats.continuation_resume_count, 0);
    assert_eq!(stats.max_frame_depth, 1);
    assert_eq!(
        analysis
            .intrinsic_counts
            .get(&IntrinsicDescriptor {
                dialect: "facet-hash",
                name: "bytes",
            })
            .copied(),
        Some(1)
    );
    assert_eq!(
        analysis
            .intrinsic_counts
            .get(&IntrinsicDescriptor {
                dialect: "facet-hash",
                name: "list",
            })
            .copied()
            .unwrap_or(0),
        0
    );
}

#[test]
fn byte_array_value_plan_hashes_bulk_bytes() {
    let plan = HashPlan::<[u8; 4]>::build().unwrap();
    let value = [10, 20, 30, 40];
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    facet_hash::hash_bytes_into(&value, &mut expected);
    plan.hash(&value, &mut actual).unwrap();

    assert_eq!(actual.bytes, expected.bytes);
    assert_eq!(
        plan.analysis().intrinsic_counts[&IntrinsicDescriptor {
            dialect: "facet-hash",
            name: "bytes",
        }],
        1
    );
}

#[test]
fn structural_byte_vec_keeps_sequence_shape() {
    let plan = HashPlan::<Vec<u8>>::build_structural().unwrap();
    let analysis = plan.analysis();

    assert_eq!(
        analysis
            .intrinsic_counts
            .get(&IntrinsicDescriptor {
                dialect: "facet-hash",
                name: "bytes",
            })
            .copied()
            .unwrap_or(0),
        0
    );
    assert_eq!(
        analysis.intrinsic_counts[&IntrinsicDescriptor {
            dialect: "facet-hash",
            name: "list",
        }],
        1
    );
}

#[test]
fn hashes_scalar_sets_and_maps() {
    let plan = HashPlan::<Collections>::build().unwrap();
    let mut set = BTreeSet::new();
    set.insert(1);
    set.insert(2);
    let mut map = BTreeMap::new();
    map.insert("ada".to_owned(), 36);
    map.insert("grace".to_owned(), 37);
    let left = Collections { set, map };
    let mut right = left.clone();
    right.map.insert("grace".to_owned(), 38);

    assert_ne!(plan.hash64(&left).unwrap(), plan.hash64(&right).unwrap());
}

#[test]
fn maps_continue_after_program_key_and_scalar_value() {
    let plan = HashPlan::<ProgramKeyMap>::build().unwrap();
    let mut left_map = BTreeMap::new();
    left_map.insert(MapKey { id: 1 }, 10);
    left_map.insert(MapKey { id: 2 }, 20);
    let mut right_map = left_map.clone();
    right_map.insert(MapKey { id: 2 }, 21);

    assert_ne!(
        plan.hash64(&ProgramKeyMap { map: left_map }).unwrap(),
        plan.hash64(&ProgramKeyMap { map: right_map }).unwrap()
    );
}

#[test]
fn struct_scalar_runs_resume_around_program_fields() {
    let plan = HashPlan::<MixedScalarRuns>::build().unwrap();
    let left = MixedScalarRuns {
        a: 1,
        point: Point { x: 2, y: 3 },
        b: 4,
        c: 5,
    };
    let right = MixedScalarRuns {
        c: 6,
        ..left.clone()
    };

    assert_ne!(plan.hash64(&left).unwrap(), plan.hash64(&right).unwrap());
}

#[test]
fn structural_mode_adds_shape_discrimination() {
    let value_plan = HashPlan::<Point>::build().unwrap();
    let structural_plan = HashPlan::<Point>::build_structural().unwrap();
    let value = Point { x: 10, y: -4 };

    assert_ne!(
        value_plan.hash64(&value).unwrap(),
        structural_plan.hash64(&value).unwrap()
    );
}

#[test]
fn hashes_nested_supported_shapes() {
    let plan = HashPlan::<Nested>::build().unwrap();
    let left = Nested {
        people: vec![Person {
            name: "Ada".to_owned(),
            age: 36,
            email: Some("ada@example.test".to_owned()),
            scores: vec![1, 1, 2, 3, 5],
        }],
        points: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }],
        label: "math".into(),
        maybe: Ok(Some(42)),
    };
    let right = Nested {
        maybe: Ok(Some(43)),
        ..left.clone()
    };

    assert_ne!(plan.hash64(&left).unwrap(), plan.hash64(&right).unwrap());
}

#[test]
fn hashes_floats_by_bits() {
    let plan = HashPlan::<Floaty>::build().unwrap();
    let negative_zero = Floaty { x: -0.0, y: -0.0 };
    let positive_zero = Floaty { x: 0.0, y: 0.0 };

    assert_ne!(
        plan.hash64(&negative_zero).unwrap(),
        plan.hash64(&positive_zero).unwrap()
    );
}

#[test]
fn metadata_fields_are_not_hashed() {
    #[derive(Debug, Facet)]
    struct WithMetadata {
        value: u32,
        #[facet(metadata = "span")]
        span: u32,
    }

    let plan = HashPlan::<WithMetadata>::build().unwrap();

    assert_eq!(
        plan.hash64(&WithMetadata { value: 7, span: 0 }).unwrap(),
        plan.hash64(&WithMetadata {
            value: 7,
            span: 999
        })
        .unwrap()
    );
}

#[test]
fn plan_writes_through_supplied_hasher() {
    let plan = HashPlan::<Person>::build().unwrap();
    let value = Person {
        name: "Grace".to_owned(),
        age: 37,
        email: None,
        scores: vec![10, 20],
    };
    let mut hasher = RecordingHasher::default();

    plan.hash(&value, &mut hasher).unwrap();

    assert!(!hasher.bytes.is_empty());
}

#[test]
fn plan_reports_canonical_effect_stats() {
    let plan = HashPlan::<Nested>::build().unwrap();

    let analysis = plan.analysis();
    let stats = analysis.effect_stats;

    assert_eq!(plan.effect_stats(), stats);
    assert_eq!(analysis.program_stats.block_count, 0);
    assert_eq!(stats.block_count, 0);
    assert!(
        analysis.intrinsic_counts[&IntrinsicDescriptor {
            dialect: "facet-hash",
            name: "struct",
        }] >= 2
    );
    assert_eq!(
        analysis.intrinsic_counts[&IntrinsicDescriptor {
            dialect: "facet-hash",
            name: "list",
        }],
        2
    );
    assert_eq!(
        analysis.intrinsic_counts[&IntrinsicDescriptor {
            dialect: "facet-hash",
            name: "array",
        }],
        1
    );
    assert_eq!(
        analysis.intrinsic_counts[&IntrinsicDescriptor {
            dialect: "facet-hash",
            name: "pointer",
        }],
        1
    );
    assert!(stats.total.intrinsic_op_count > 0);
    assert!(stats.total.sink_write_count > 0);
    assert!(stats.total.typed_memory_read_count > 0);
    assert!(stats.total.barrier_count > 0);
    assert_eq!(stats.total.opaque_count, 0);
}

#[test]
fn recursive_shapes_still_lower_blocks() {
    let plan = HashPlan::<Node>::build().unwrap();
    let short = Node {
        value: 1,
        next: None,
    };
    let long = Node {
        value: 1,
        next: Some(Box::new(Node {
            value: 2,
            next: None,
        })),
    };

    assert!(plan.analysis().program_stats.block_count > 0);
    assert_ne!(plan.hash64(&short).unwrap(), plan.hash64(&long).unwrap());
}

#[test]
fn unsupported_enums_fail_while_building() {
    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum Choice {
        #[allow(dead_code)]
        A,
        #[allow(dead_code)]
        B(u8),
    }

    let err = HashPlan::<Choice>::build().unwrap_err();

    assert!(err.to_string().contains("enum"));
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_matches_interpreter_stream_for_scalar_struct() {
    let plan = HashPlan::<Point>::build().unwrap();
    let native = NativeHashPlan::<Point, RecordingHasher>::build().unwrap();
    let value = Point { x: 10, y: -4 };
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    plan.hash(&value, &mut expected).unwrap();
    native.hash(&value, &mut actual).unwrap();

    assert_eq!(expected.bytes, actual.bytes);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_matches_interpreter_stream_for_nested_scalar_struct() {
    let plan = HashPlan::<MixedScalarRuns>::build().unwrap();
    let native = NativeHashPlan::<MixedScalarRuns, RecordingHasher>::build().unwrap();
    let value = MixedScalarRuns {
        a: 1,
        point: Point { x: 2, y: 3 },
        b: 4,
        c: 5,
    };
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    plan.hash(&value, &mut expected).unwrap();
    native.hash(&value, &mut actual).unwrap();

    assert_eq!(expected.bytes, actual.bytes);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_matches_interpreter_stream_for_text_scalars() {
    let plan = HashPlan::<TextScalars>::build().unwrap();
    let native = NativeHashPlan::<TextScalars, RecordingHasher>::build().unwrap();
    let value = TextScalars {
        owned: "Ada".to_owned(),
        borrowed: "Grace",
        cow: Cow::Owned("Katherine".to_owned()),
    };
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    plan.hash(&value, &mut expected).unwrap();
    native.hash(&value, &mut actual).unwrap();

    assert_eq!(expected.bytes, actual.bytes);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_matches_interpreter_stream_for_root_array() {
    let plan = HashPlan::<[u16; 4]>::build().unwrap();
    let native = NativeHashPlan::<[u16; 4], RecordingHasher>::build().unwrap();
    let value = [1, 1, 2, 3];
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    plan.hash(&value, &mut expected).unwrap();
    native.hash(&value, &mut actual).unwrap();

    assert_eq!(expected.bytes, actual.bytes);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_matches_interpreter_stream_for_array_field() {
    let plan = HashPlan::<PointArray>::build().unwrap();
    let native = NativeHashPlan::<PointArray, RecordingHasher>::build().unwrap();
    let value = PointArray {
        points: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }],
        tail: -5,
    };
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    plan.hash(&value, &mut expected).unwrap();
    native.hash(&value, &mut actual).unwrap();

    assert_eq!(expected.bytes, actual.bytes);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_hashes_floats_by_bits() {
    let plan = HashPlan::<Floaty>::build().unwrap();
    let native = NativeHashPlan::<Floaty, RecordingHasher>::build().unwrap();
    let value = Floaty {
        x: -0.0,
        y: f32::NAN,
    };
    let mut expected = RecordingHasher::default();
    let mut actual = RecordingHasher::default();

    plan.hash(&value, &mut expected).unwrap();
    native.hash(&value, &mut actual).unwrap();

    assert_eq!(expected.bytes, actual.bytes);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_reports_code_layout_stats() {
    let native = NativeHashPlan::<Point>::build().unwrap();
    let stats = native.stats();

    assert_eq!(stats.chain_count, 1);
    assert_eq!(stats.scalar_count, 0);
    assert_eq!(stats.scalar_run_count, 1);
    assert_eq!(stats.scalar_run_field_count, 2);
    assert_eq!(stats.const_usize_count, 0);
    assert_eq!(stats.prog_slot_count, 1);
    assert_eq!(stats.stencil_count, 2);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_array_plan_reports_const_layout_stats() {
    let native = NativeHashPlan::<PointArray>::build().unwrap();
    let stats = native.stats();

    assert_eq!(stats.chain_count, 1);
    assert_eq!(stats.scalar_count, 0);
    assert_eq!(stats.scalar_run_count, 1);
    assert_eq!(stats.scalar_run_field_count, 6);
    assert_eq!(stats.const_usize_count, 1);
    assert_eq!(stats.prog_slot_count, 1);
    assert_eq!(stats.stencil_count, 2);
}

#[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_plan_rejects_aggregate_fields_for_now() {
    let Err(err) = NativeHashPlan::<Person>::build() else {
        panic!("Person contains aggregate fields and should not compile natively yet");
    };

    assert!(err.to_string().contains("native aggregate hashing"));
}
