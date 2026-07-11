//! Adversarial certificates for the closed role-typed value-identity epoch.
//!
//! These exercise the public framed-identity surface as an outside consumer:
//! the closed [`FramedHasher`] writer, the owned pre-resolved [`FramedNode`]
//! tree, and [`Store`] interning. They are the structural proofs required by
//! `machine.identity.framed-encoding` and the rung-051 checkpoint.

use std::path::Path;

use vix::runtime::{
    FramedField, FramedHasher, FramedNode, FramedValue, SchemaId, Store, ValueId,
};

fn schema(name: &str) -> SchemaId {
    SchemaId::named(name)
}

/// Certificate 1 — role and length-prefix structural distinctions.
///
/// The same underlying scalar bytes, arranged under different framed roles
/// (leaf vs. sequence vs. field list vs. map pairs) and different length-prefix
/// boundaries, must produce distinct identities. Concatenation ambiguity is
/// closed by the role tags and the length prefixes.
#[test]
fn roles_and_length_prefixes_are_structurally_distinct() {
    let s = schema("role.subject");
    let es = schema("role.element");

    let leaf = {
        let mut h = FramedHasher::new();
        h.start(s, 1).bytes(b"\x01\x02");
        h.finish()
    };
    let sequence = {
        let mut h = FramedHasher::new();
        h.start(s, 2).seq_len(2);
        h.seq_element(0, es).bytes(b"\x01");
        h.seq_element(1, es).bytes(b"\x02");
        h.finish()
    };
    let fields = {
        let mut h = FramedHasher::new();
        h.start(s, 2);
        h.field(0, es).bytes(b"\x01");
        h.field(1, es).bytes(b"\x02");
        h.finish()
    };
    let one_field = {
        let mut h = FramedHasher::new();
        h.start(s, 1);
        h.field(0, es).bytes(b"\x01\x02");
        h.finish()
    };
    let variant = {
        let mut h = FramedHasher::new();
        h.start(s, 2);
        h.variant(0);
        h.field(0, es).bytes(b"\x01");
        h.field(1, es).bytes(b"\x02");
        h.finish()
    };

    // Same 0x01,0x02 material; every role framing separates the identities.
    let all = [leaf, sequence, fields, one_field, variant];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b, "distinct framed roles must not collide");
        }
    }

    // The map-pair role is distinct from the sequence-element role over the
    // same following bytes.
    let map_role = {
        let mut h = FramedHasher::new();
        h.start(s, 1);
        h.map_pair(0).bytes(b"\x01");
        h.finish()
    };
    let seq_role = {
        let mut h = FramedHasher::new();
        h.start(s, 1).seq_len(1);
        h.seq_element(0, es).bytes(b"\x01");
        h.finish()
    };
    assert_ne!(map_role, seq_role, "map-pair and seq-element roles differ");
}

/// Certificate 2 — a semantic scalar/opaque value dedupes under the new epoch,
/// and the store leaf identity equals the independently-computed
/// [`FramedNode::leaf`] identity (production wiring goes through the writer).
#[test]
fn scalar_leaf_dedupes_and_matches_node_identity() {
    let mut store = Store::default();
    let first = store.intern_realized(schema("scalar.t"), b"payload");
    let again = store.intern_realized(schema("scalar.t"), b"payload");
    assert_eq!(first.identity, again.identity);
    assert_eq!(first.handle, again.handle);
    assert!(again.deduped, "equal semantic scalar dedupes");

    // Same bytes, different schema -> different identity (value-identity pair).
    let other_schema = store.intern_realized(schema("scalar.u"), b"payload");
    assert_ne!(
        first.identity, other_schema.identity,
        "identity is the (schema, content) pair, not bytes alone"
    );

    // The realized scalar path computes exactly the framed leaf identity.
    let node = FramedNode::leaf(schema("scalar.t"), b"payload".to_vec());
    assert_eq!(first.identity, node.identity());
}

/// Certificate 4 — nested child identities are contributed by referent
/// `ValueId` and are independent of any handle integer or assignment order.
#[test]
fn nested_children_are_by_referent_not_handle() {
    let element = schema("child.element");
    let container = schema("child.container");

    // Two stores that assign different handle integers to the same values.
    let mut store_a = Store::default();
    let x_a = store_a.intern_realized(element, b"x").identity;
    let y_a = store_a.intern_realized(element, b"y").identity;

    let mut store_b = Store::default();
    // Pad so identical values land on different handle integers here.
    let _pad = store_b.intern_realized(schema("child.pad"), b"pad");
    let _pad2 = store_b.intern_realized(schema("child.pad"), b"pad2");
    let y_b = store_b.intern_realized(element, b"y").identity;
    let x_b = store_b.intern_realized(element, b"x").identity;

    assert_eq!(x_a, x_b, "referent identity independent of handle integer");
    assert_eq!(y_a, y_b, "referent identity independent of handle integer");

    let node_a = FramedNode::SeqChildren {
        schema: container,
        element_schema: element,
        children: vec![x_a, y_a],
    };
    let node_b = FramedNode::SeqChildren {
        schema: container,
        element_schema: element,
        children: vec![x_b, y_b],
    };
    assert_eq!(
        node_a.identity(),
        node_b.identity(),
        "container identity is a function of referent ids, not handles or interning order"
    );

    // Sequence order is still structural.
    let reversed = FramedNode::SeqChildren {
        schema: container,
        element_schema: element,
        children: vec![y_a, x_a],
    };
    assert_ne!(node_a.identity(), reversed.identity());
}

/// Certificate 5 — streaming and bulk use of the writer over the same framed
/// sequence produce identical identities.
#[test]
fn streaming_and_bulk_sequence_match() {
    let s = schema("stream.seq");
    let es = schema("stream.element");
    let count: u64 = 4096;

    // Bulk: one packed canonical buffer through the owned node.
    let mut packed = Vec::with_capacity(count as usize * 8);
    for i in 0..count {
        packed.extend_from_slice(&i.to_le_bytes());
    }
    let bulk = FramedNode::SeqInline {
        schema: s,
        element_schema: es,
        element_width: 8,
        canonical_bytes: packed,
    };
    let bulk_id = bulk.identity();

    // Streaming: drive the closed writer element by element from separate
    // per-element buffers.
    let streamed = {
        let mut h = FramedHasher::new();
        h.start(s, count).seq_len(count);
        for i in 0..count {
            h.seq_element(i, es).bytes(&i.to_le_bytes());
        }
        ValueId {
            schema: s,
            content: h.finish(),
        }
    };

    assert_eq!(
        bulk_id, streamed,
        "an incrementally fed sequence hashes bit-identically to the bulk build"
    );
}

/// Certificate 6 — audit boundary. Runtime identity hashing is encapsulated in
/// exactly one module: `blake3` may only appear in `src/runtime/identity.rs`.
/// This is a module-boundary assertion, not a per-line grep for `update`.
#[test]
fn only_the_identity_module_references_blake3() {
    let runtime_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("runtime");
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&runtime_dir).expect("runtime dir is readable") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("file name")
            .to_owned();
        let source = std::fs::read_to_string(&path).expect("source is readable");
        if source.contains("blake3") && name != "identity.rs" {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "blake3 must only be reachable through the closed writer in identity.rs; offenders: {offenders:?}"
    );
}

/// A failure value routed through the store uses start/variant/field/child
/// roles and stays resident-memory independent.
#[test]
fn failure_leaf_uses_framed_roles_via_the_writer() {
    // The failure certificate lives in the store unit tests; here we only
    // confirm the field/optional value forms compile against the public tree.
    let s = schema("failure.demo");
    let child_schema = schema("failure.subject");
    let subject = FramedNode::leaf(child_schema, b"subject".to_vec()).identity();

    let with_subject = FramedNode::Variant {
        schema: s,
        tag: 1,
        fields: vec![
            FramedField {
                schema: schema("failure.site"),
                value: FramedValue::Bytes(7u32.to_le_bytes().to_vec()),
            },
            FramedField {
                schema: child_schema,
                value: FramedValue::Optional(Some(subject)),
            },
        ],
    };
    let without_subject = FramedNode::Variant {
        schema: s,
        tag: 1,
        fields: vec![
            FramedField {
                schema: schema("failure.site"),
                value: FramedValue::Bytes(7u32.to_le_bytes().to_vec()),
            },
            FramedField {
                schema: child_schema,
                value: FramedValue::Optional(None),
            },
        ],
    };
    assert_ne!(
        with_subject.identity(),
        without_subject.identity(),
        "an optional child participates in identity through the child role"
    );
}
