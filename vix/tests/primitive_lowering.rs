//! Phase 04 — the `Op::EffectRequest` a registered primitive lowers to is
//! partitioned into a request value island plus one generic effect edge, and the
//! response binds as a realized value input on the artifact. Structural only:
//! phase 04 never resolves, memoizes, dispatches, or executes a primitive.

use vix::compiler::{Compilation, Compiler, PrimitiveManifest, PrimitiveSignature};
use vix::diagnostic::Diagnostics;
use vix::lowering::LoweringCache;
use vix::vir::{EffectId, Op, RecordField, RecordType, Type};

const PRIMITIVE_EFFECT_ID: EffectId = EffectId([9u8; 32]);

/// One primitive: `probe_version where { text: String } -> String`. A `String`
/// response has a single-`Handle` frame shape, so its value input binds as a
/// `RealizedHandle` store handle (the realized channel the effect response rides).
fn probe_manifest() -> PrimitiveManifest {
    let mut manifest = PrimitiveManifest::new();
    manifest.insert(
        "probe_version",
        PrimitiveSignature {
            effect: PRIMITIVE_EFFECT_ID,
            request: Type::Record(RecordType {
                name: "ProbeRequest@0000000000000009".into(),
                fields: vec![RecordField {
                    name: "text".into(),
                    ty: Type::String,
                }],
            }),
            response: Type::String,
        },
    );
    manifest
}

fn compile(source: &str) -> Result<Compilation, Diagnostics> {
    Compiler::new()
        .with_primitives(probe_manifest())
        .compile(source)
}

const EFFECT_SOURCE: &str = "#[test]\nfn t() -> Stream<Check> {\n    let v = probe_version where { text: \"1.2.3\" };\n    yield expect_eq(v, \"9\");\n}\n";

#[test]
fn the_request_subgraph_becomes_its_own_value_island() {
    let compilation = compile(EFFECT_SOURCE).expect("effect source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    assert_eq!(
        partitioned.effect_islands.len(),
        1,
        "one request island per distinct request record"
    );
    let request_island = &partitioned.effect_islands[0];
    let output = request_island
        .island
        .nodes
        .iter()
        .find(|node| node.id == request_island.island.output)
        .expect("request island has its output node");
    assert!(
        matches!(output.op, Op::Record),
        "the request island outputs the request record, got {:?}",
        output.op
    );
    // The request subgraph never contains the effect node that consumes it.
    assert!(
        !request_island
            .island
            .nodes
            .iter()
            .any(|node| matches!(node.op, Op::EffectRequest { .. })),
        "the request island is pure — the effect node is downstream of it"
    );
}

#[test]
fn the_consumer_records_a_generic_effect_edge() {
    let compilation = compile(EFFECT_SOURCE).expect("effect source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    let request_id = partitioned.effect_islands[0].id;
    let edges: Vec<_> = partitioned
        .islands
        .iter()
        .flat_map(|island| island.effect_inputs.iter())
        .collect();
    assert_eq!(edges.len(), 1, "one effect edge on the consuming check island");
    assert_eq!(
        edges[0].primitive, PRIMITIVE_EFFECT_ID,
        "the edge carries the registered EffectId, not a per-primitive variant"
    );
    assert_eq!(
        edges[0].request, request_id,
        "the edge names the request value island"
    );
}

#[test]
fn the_effect_node_is_rewritten_to_a_bound_parameter() {
    let compilation = compile(EFFECT_SOURCE).expect("effect source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    let leftover = partitioned
        .islands
        .iter()
        .chain(partitioned.effect_islands.iter().map(|value| &value.island))
        .flat_map(|island| island.nodes.iter())
        .any(|node| matches!(node.op, Op::EffectRequest { .. }));
    assert!(
        !leftover,
        "partitioning rewrote every consumed Op::EffectRequest to a value-input parameter"
    );
    // The consuming island holds exactly one effect parameter and no leftover
    // value inputs it did not ask for.
    let consumer = &partitioned.islands[0];
    assert_eq!(consumer.effect_inputs.len(), 1);
    assert_eq!(
        consumer.parameters.len(),
        consumer.value_inputs.len() + consumer.effect_inputs.len(),
        "effect params occupy parameters[value_inputs.len()..]"
    );
}

#[test]
fn an_effect_free_test_has_no_effect_machinery() {
    let source =
        "#[test]\nfn t() -> Stream<Check> {\n    yield expect_eq(1 + 1, 2);\n}\n";
    let compilation = Compiler::new().compile(source).expect("pure source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    assert!(partitioned.effect_islands.is_empty());
    assert!(
        partitioned
            .islands
            .iter()
            .all(|island| island.effect_inputs.is_empty())
    );
}

/// The effect response binds as a realized value input on the artifact, and the
/// request island lowers (phase 05 finds it warm). No primitive is executed.
#[test]
fn the_response_binds_as_a_realized_value_input() {
    let compilation = compile(EFFECT_SOURCE).expect("effect source compiles");
    let partitioned = compilation
        .module
        .partition_test(&compilation.module.tests[0]);
    let mut cache = LoweringCache::default();

    let request = cache
        .get_or_lower(&partitioned.effect_islands[0].island)
        .expect("request island lowers");
    assert!(
        request.effect_inputs.is_empty(),
        "a request island is a pure value producer"
    );

    let consumer = cache
        .get_or_lower(&partitioned.islands[0])
        .expect("consumer island lowers");
    assert_eq!(
        consumer.effect_inputs.len(),
        1,
        "the artifact carries the effect edge as a precomputed fact"
    );
    let binding = &consumer.effect_inputs[0];
    assert_eq!(binding.primitive, PRIMITIVE_EFFECT_ID);
    assert_eq!(binding.request, partitioned.effect_islands[0].id);
    assert_eq!(
        binding.entry,
        consumer.value_inputs.len(),
        "the effect param entry follows the value-input entries"
    );
    assert!(
        binding.schema.is_some(),
        "a String response binds as a RealizedHandle store-handle entry"
    );
}
