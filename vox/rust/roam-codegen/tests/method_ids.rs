use facet::Facet;
use roam::schema::{ArgDetail, MethodDetail};
use roam::service;
use roam::session::{Rx, Tx};
use roam_codegen::targets;

/// Testbed service for conformance testing
#[allow(async_fn_in_trait)]
#[service]
pub trait Testbed {
    /// Echoes the message back
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed
    async fn reverse(&self, message: String) -> String;

    /// Client pushes numbers, server returns their sum
    async fn sum(&self, numbers: Tx<i32>) -> i64;

    /// Server streams numbers back to client
    async fn generate(&self, count: u32, output: Rx<i32>);

    /// Bidirectional streaming
    async fn transform(&self, input: Tx<String>, output: Rx<String>);
}

fn fixture_methods() -> Vec<MethodDetail> {
    vec![
        MethodDetail {
            service_name: "TemplateHost".into(),
            method_name: "load_template".into(),
            args: vec![
                ArgDetail {
                    name: "context_id".into(),
                    ty: <u64 as Facet>::SHAPE,
                },
                ArgDetail {
                    name: "name".into(),
                    ty: <String as Facet>::SHAPE,
                },
            ],
            return_type: <Vec<u8> as Facet>::SHAPE,
            doc: None,
        },
        MethodDetail {
            service_name: "TemplateHost".into(),
            method_name: "loadTemplate".into(), // should normalize to same kebab as load_template
            args: vec![ArgDetail {
                name: "id".into(),
                ty: <u64 as Facet>::SHAPE,
            }],
            return_type: <() as Facet>::SHAPE,
            doc: None,
        },
    ]
}

#[test]
fn typescript_contains_hex_bigint_ids() {
    let methods = fixture_methods();
    let out = targets::typescript::generate_method_ids(&methods);
    assert!(out.contains("export const METHOD_ID"));
    // ensure we emit hex + bigint literal
    assert!(out.contains("0x"));
    assert!(out.contains("n,"));
}

#[test]
fn swift_contains_uint64_literals() {
    let methods = fixture_methods();
    let out = targets::swift::generate_method_ids(&methods);
    assert!(out.contains("UInt64"));
    assert!(out.contains("0x"));
}

#[test]
fn typescript_service_generation() {
    let service = testbed_service_detail();
    let out = targets::typescript::generate_service(&service);

    // Print for inspection
    println!("{}", out);

    // Should contain method IDs
    assert!(out.contains("export const METHOD_ID"));
    assert!(out.contains("echo:"));
    assert!(out.contains("reverse:"));

    // Should contain type definitions
    assert!(out.contains("EchoRequest"));
    assert!(out.contains("EchoResponse"));
    assert!(out.contains("ReverseRequest"));
    assert!(out.contains("ReverseResponse"));

    // Should contain caller interface
    assert!(out.contains("interface TestbedCaller"));
    assert!(out.contains("echo(message: string): Promise<string>"));

    // Should contain handler interface
    assert!(out.contains("interface TestbedHandler"));

    // Should contain method handlers Map (for use with RpcDispatcher)
    assert!(out.contains("testbed_methodHandlers"));
    assert!(out.contains("Map<bigint, MethodHandler<TestbedHandler>>"));
}

#[test]
fn swift_service_generation() {
    let service = testbed_service_detail();
    let out = targets::swift::generate_service(&service);

    // Should contain method IDs
    assert!(out.contains("TestbedMethodId"));
    assert!(out.contains("echo:"));
    assert!(out.contains("reverse:"));

    // Should contain caller protocol
    assert!(out.contains("protocol TestbedCaller"));
    assert!(out.contains("func echo(message: String) async throws -> String"));

    // Should contain handler
    assert!(out.contains("protocol TestbedHandler"));
    // Should contain dispatcher class
    assert!(out.contains("class TestbedDispatcher"));

    // Print for inspection
    println!("{}", out);
}
