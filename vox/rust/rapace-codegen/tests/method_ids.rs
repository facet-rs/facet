use rapace_codegen::targets;
use rapace_schema::{ArgDetail, MethodDetail, TypeDetail};
use rapace_service_macros::service;

/// Simple echo service for conformance testing
#[allow(async_fn_in_trait)]
#[service]
pub trait Echo {
    /// Echoes the message back
    async fn echo(&self, message: String) -> String;

    /// Returns the message reversed
    async fn reverse(&self, message: String) -> String;
}

fn fixture_methods() -> Vec<MethodDetail> {
    vec![
        MethodDetail {
            service_name: "TemplateHost".into(),
            method_name: "load_template".into(),
            args: vec![
                ArgDetail {
                    name: "context_id".into(),
                    type_info: TypeDetail::U64,
                },
                ArgDetail {
                    name: "name".into(),
                    type_info: TypeDetail::String,
                },
            ],
            return_type: TypeDetail::Bytes,
            doc: None,
        },
        MethodDetail {
            service_name: "TemplateHost".into(),
            method_name: "loadTemplate".into(), // should normalize to same kebab as load_template
            args: vec![ArgDetail {
                name: "id".into(),
                type_info: TypeDetail::U64,
            }],
            return_type: TypeDetail::Unit,
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
fn go_contains_uint64_map() {
    let methods = fixture_methods();
    let out = targets::go::generate_method_ids(&methods);
    assert!(out.contains("map[string]uint64"));
    assert!(out.contains("0x"));
}

#[test]
fn java_contains_map_entries() {
    let methods = fixture_methods();
    let out = targets::java::generate_method_ids(&methods);
    assert!(out.contains("Map.entry("));
    assert!(out.contains("0x"));
    assert!(out.contains("L)"));
}

#[test]
fn typescript_service_generation() {
    let service = echo_service_detail();
    let out = targets::typescript::generate_service(&service);

    // Should contain method IDs
    assert!(out.contains("export const METHOD_ID"));
    assert!(out.contains("echo:"));
    assert!(out.contains("reverse:"));

    // Should contain type definitions
    assert!(out.contains("EchoRequest"));
    assert!(out.contains("EchoResponse"));
    assert!(out.contains("ReverseRequest"));
    assert!(out.contains("ReverseResponse"));

    // Should contain client interface
    assert!(out.contains("interface EchoClient"));
    assert!(out.contains("echo(message: string): Promise<string>"));

    // Should contain server handler interface
    assert!(out.contains("interface EchoHandler"));
    assert!(out.contains("createEchoDispatcher"));

    // Print for inspection
    println!("{}", out);
}

#[test]
fn python_method_ids() {
    let methods = fixture_methods();
    let out = targets::python::generate_method_ids(&methods);
    assert!(out.contains("METHOD_ID: dict[str, int]"));
    assert!(out.contains("0x"));
}

#[test]
fn python_service_generation() {
    let service = echo_service_detail();
    let out = targets::python::generate_service(&service);

    // Should contain method IDs
    assert!(out.contains("METHOD_ID"));
    assert!(out.contains("\"echo\":"));
    assert!(out.contains("\"reverse\":"));

    // Should contain client protocol
    assert!(out.contains("class EchoClient(Protocol)"));
    assert!(out.contains("def echo(self, message: str) -> str"));

    // Should contain server handler
    assert!(out.contains("class EchoHandler(ABC)"));
    assert!(out.contains("@abstractmethod"));
    assert!(out.contains("create_echo_dispatcher"));

    // Print for inspection
    println!("{}", out);
}

#[test]
fn swift_service_generation() {
    let service = echo_service_detail();
    let out = targets::swift::generate_service(&service);

    // Should contain method IDs
    assert!(out.contains("EchoMethodId"));
    assert!(out.contains("echo:"));
    assert!(out.contains("reverse:"));

    // Should contain client protocol
    assert!(out.contains("protocol EchoClient"));
    assert!(out.contains("func echo(message: String) async throws -> String"));

    // Should contain server handler
    assert!(out.contains("protocol EchoHandler"));
    assert!(out.contains("createEchoDispatcher"));

    // Print for inspection
    println!("{}", out);
}

#[test]
fn go_service_generation() {
    let service = echo_service_detail();
    let out = targets::go::generate_service(&service);

    // Should contain method ID constants
    assert!(out.contains("EchoMethodEcho"));
    assert!(out.contains("EchoMethodReverse"));

    // Should contain client interface
    assert!(out.contains("type EchoClient interface"));
    assert!(out.contains("Echo(ctx context.Context, message string) (string, error)"));

    // Should contain server handler
    assert!(out.contains("type EchoHandler interface"));
    assert!(out.contains("NewEchoDispatcher"));

    // Print for inspection
    println!("{}", out);
}

#[test]
fn java_service_generation() {
    let service = echo_service_detail();
    let out = targets::java::generate_service(&service);

    // Should contain method ID constants
    assert!(out.contains("EchoMethodId"));
    assert!(out.contains("ECHO"));
    assert!(out.contains("REVERSE"));

    // Should contain client interface
    assert!(out.contains("interface EchoClient"));
    assert!(out.contains("CompletableFuture<String> echo(String message)"));

    // Should contain server handler
    assert!(out.contains("interface EchoHandler"));
    assert!(out.contains("class EchoDispatcher"));

    // Print for inspection
    println!("{}", out);
}
