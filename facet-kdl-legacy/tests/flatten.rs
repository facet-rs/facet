use facet::Facet;
use facet_kdl_legacy as kdl;
use indoc::indoc;

/// Test that #[facet(flatten)] inlines nested struct fields.
#[test]
fn flatten_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(flatten)]
        connection: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ConnectionSettings {
        #[facet(kdl::property)]
        port: u16,
        #[facet(kdl::property)]
        timeout: u32,
    }

    // With flatten, port and timeout should be properties on server directly
    let kdl = indoc! {r#"
        server "localhost" port=8080 timeout=30
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.connection.port, 8080);
    assert_eq!(config.server.connection.timeout, 30);
}

/// Test that #[facet(flatten)] works when properties are interleaved
/// (some from parent, some from flattened struct, in mixed order).
///
/// NOTE: Currently this is a known limitation - flattened properties must be
/// grouped together. This test documents the current behavior.
#[test]
fn flatten_struct_interleaved() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(kdl::property)]
        enabled: bool,
        #[facet(flatten)]
        connection: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ConnectionSettings {
        #[facet(kdl::property)]
        port: u16,
        #[facet(kdl::property)]
        timeout: u32,
    }

    // Flattened properties grouped together - this works
    let kdl = indoc! {r#"
        server "localhost" enabled=#true port=8080 timeout=30
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert!(config.server.enabled);
    assert_eq!(config.server.connection.port, 8080);
    assert_eq!(config.server.connection.timeout, 30);

    // Interleaved properties - now works with solver-based deserialization
    let kdl_interleaved = indoc! {r#"
        server "localhost" port=8080 enabled=#true timeout=30
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_interleaved).unwrap();
    assert_eq!(config.server.host, "localhost");
    assert!(config.server.enabled);
    assert_eq!(config.server.connection.port, 8080);
    assert_eq!(config.server.connection.timeout, 30);
}

/// Test that #[facet(flatten)] works with enums - the solver should disambiguate
/// based on which properties are present.
#[test]
fn flatten_enum_simple() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        storage: Storage,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Storage {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        backend: StorageBackend,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum StorageBackend {
        File(FileStorage),
        Database(DatabaseStorage),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FileStorage {
        #[facet(kdl::property)]
        path: String,
        #[facet(kdl::property)]
        readonly: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DatabaseStorage {
        #[facet(kdl::property)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    // Test File variant - has path and readonly properties
    let kdl_file = indoc! {r#"
        storage "my-files" path="/var/data" readonly=#true
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_file).expect("should parse File variant");
    assert_eq!(config.storage.name, "my-files");
    match &config.storage.backend {
        StorageBackend::File(f) => {
            assert_eq!(f.path, "/var/data");
            assert!(f.readonly);
        }
        _ => panic!("expected File variant"),
    }

    // Test Database variant - has host and port properties
    let kdl_db = indoc! {r#"
        storage "my-db" host="localhost" port=5432
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_db).expect("should parse Database variant");
    assert_eq!(config.storage.name, "my-db");
    match &config.storage.backend {
        StorageBackend::Database(db) => {
            assert_eq!(db.host, "localhost");
            assert_eq!(db.port, 5432);
        }
        _ => panic!("expected Database variant"),
    }
}

/// Test nested flatten: flatten inside flatten
/// struct A { flatten B { flatten C(enum) } }
#[test]
fn flatten_nested() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        settings: ServerSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ServerSettings {
        #[facet(kdl::property)]
        enabled: bool,
        #[facet(flatten)]
        backend: Backend,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Backend {
        Http(HttpBackend),
        Grpc(GrpcBackend),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct HttpBackend {
        #[facet(kdl::property)]
        url: String,
        #[facet(kdl::property)]
        timeout: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct GrpcBackend {
        #[facet(kdl::property)]
        address: String,
        #[facet(kdl::property)]
        tls: bool,
    }

    // HTTP backend
    let kdl = indoc! {r#"
        server "api" enabled=#true url="http://localhost:8080" timeout=30
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse nested flatten (HTTP)");
    assert_eq!(config.server.name, "api");
    assert!(config.server.settings.enabled);
    match &config.server.settings.backend {
        Backend::Http(http) => {
            assert_eq!(http.url, "http://localhost:8080");
            assert_eq!(http.timeout, 30);
        }
        _ => panic!("expected Http variant"),
    }

    // gRPC backend
    let kdl = indoc! {r#"
        server "rpc" enabled=#false address="localhost:9090" tls=#true
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse nested flatten (gRPC)");
    assert_eq!(config.server.name, "rpc");
    assert!(!config.server.settings.enabled);
    match &config.server.settings.backend {
        Backend::Grpc(grpc) => {
            assert_eq!(grpc.address, "localhost:9090");
            assert!(grpc.tls);
        }
        _ => panic!("expected Grpc variant"),
    }
}

/// Test multiple flattened enums in the same struct
#[test]
fn flatten_multiple_enums() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        connection: Connection,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Connection {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        auth: Auth,
        #[facet(flatten)]
        transport: Transport,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Auth {
        Token(TokenAuth),
        Basic(BasicAuth),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TokenAuth {
        #[facet(kdl::property)]
        token: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct BasicAuth {
        #[facet(kdl::property)]
        username: String,
        #[facet(kdl::property)]
        password: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Transport {
        Tcp(TcpTransport),
        Unix(UnixTransport),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TcpTransport {
        #[facet(kdl::property)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct UnixTransport {
        #[facet(kdl::property)]
        socket_path: String,
    }

    // Token auth + TCP transport (4 configurations total: 2 auth × 2 transport)
    let kdl = indoc! {r#"
        connection "prod" token="secret123" host="api.example.com" port=443
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse Token + TCP");
    assert_eq!(config.connection.name, "prod");
    match (&config.connection.auth, &config.connection.transport) {
        (Auth::Token(t), Transport::Tcp(tcp)) => {
            assert_eq!(t.token, "secret123");
            assert_eq!(tcp.host, "api.example.com");
            assert_eq!(tcp.port, 443);
        }
        _ => panic!("expected Token + Tcp"),
    }

    // Basic auth + Unix socket
    let kdl = indoc! {r#"
        connection "local" username="admin" password="hunter2" socket_path="/var/run/app.sock"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse Basic + Unix");
    match (&config.connection.auth, &config.connection.transport) {
        (Auth::Basic(b), Transport::Unix(u)) => {
            assert_eq!(b.username, "admin");
            assert_eq!(b.password, "hunter2");
            assert_eq!(u.socket_path, "/var/run/app.sock");
        }
        _ => panic!("expected Basic + Unix"),
    }
}

/// Test enum variants with overlapping fields - disambiguation must use unique fields
#[test]
fn flatten_enum_overlapping_fields() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        source: Source,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Source {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        kind: SourceKind,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SourceKind {
        // Both variants have 'url', but Git also has 'branch'
        Http(HttpSource),
        Git(GitSource),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct HttpSource {
        #[facet(kdl::property)]
        url: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct GitSource {
        #[facet(kdl::property)]
        url: String,
        #[facet(kdl::property)]
        branch: String,
    }

    // Git source - 'branch' disambiguates
    let kdl = indoc! {r#"
        source "repo" url="https://github.com/example/repo" branch="main"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse Git source");
    match &config.source.kind {
        SourceKind::Git(git) => {
            assert_eq!(git.url, "https://github.com/example/repo");
            assert_eq!(git.branch, "main");
        }
        _ => panic!("expected Git variant"),
    }

    // HTTP source - only 'url', no 'branch'
    // The solver now correctly resolves this to Http because Git is missing required 'branch'.
    // Previously this was ambiguous, but the solver now filters out configs that are
    // missing required fields before reporting ambiguity.
    let kdl = indoc! {r#"
        source "download" url="https://example.com/file.zip"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should resolve to Http (Git missing required 'branch')");
    match &config.source.kind {
        SourceKind::Http(http) => {
            assert_eq!(http.url, "https://example.com/file.zip");
        }
        _ => panic!("expected Http variant"),
    }
}

/// Test flattened enum with a unit variant (no fields)
#[test]
fn flatten_enum_unit_variant() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        log: LogConfig,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LogConfig {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        output: LogOutput,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum LogOutput {
        // Unit variant - no properties needed
        Stdout,
        File(FileOutput),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FileOutput {
        #[facet(kdl::property)]
        path: String,
    }

    // File output - has 'path' property
    let kdl = indoc! {r#"
        log "app" path="/var/log/app.log"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse File output");
    match &config.log.output {
        LogOutput::File(f) => {
            assert_eq!(f.path, "/var/log/app.log");
        }
        _ => panic!("expected File variant"),
    }

    // Stdout - no properties, the solver picks Stdout since File requires 'path' which isn't provided
    let kdl = indoc! {r#"
        log "console"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse Stdout output");
    match &config.log.output {
        LogOutput::Stdout => { /* expected */ }
        _ => panic!("expected Stdout variant"),
    }
}

/// Flattened struct that mixes properties and children, with unrelated children
/// interleaved to force the "open flattened field" bookkeeping to close/reopen.
///
/// TODO: This test triggers an abort in facet-solver, needs investigation
#[test]
fn flatten_struct_child_boundaries() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        service: Service,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Service {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        details: Details,
        #[facet(kdl::child)]
        owner: Owner,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Details {
        #[facet(kdl::property)]
        secure: bool,
        #[facet(kdl::property)]
        port: u16,
        #[facet(kdl::child)]
        #[facet(default)]
        tls: Option<Tls>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Tls {
        #[facet(kdl::argument)]
        cert: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Owner {
        #[facet(kdl::argument)]
        team: String,
    }

    // Properties for the flattened `details` struct are inside `service` node,
    // and the child nodes `owner` and `tls` are in the service's block.
    let kdl = indoc! {r#"
        service "api" secure=#true port=443 {
            owner "platform"
            tls "certs/api.pem"
        }
    "#};

    let cfg: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse flattened struct with children");
    assert_eq!(cfg.service.name, "api");
    assert!(cfg.service.details.secure);
    assert_eq!(cfg.service.details.port, 443);
    assert_eq!(cfg.service.owner.team, "platform");
    assert_eq!(
        cfg.service.details.tls,
        Some(Tls {
            cert: "certs/api.pem".to_string()
        })
    );
}

/// Untagged-style enum that must use *property presence* to disambiguate.
/// Both variants share `level`, only `Tuned` requires `gain` and a `tuning` child.
///
/// This requires facet-solver to support enum variant disambiguation, where each
/// variant's struct fields are exposed as configuration options. Currently the solver
/// only handles this for `#[facet(flatten)]` enums, not regular tuple variants.
#[test]
fn flatten_enum_child_presence_disambiguation() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        mode: Mode,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Mode {
        Simple(Simple),
        Tuned(Tuned),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        #[facet(kdl::property)]
        level: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Tuned {
        #[facet(kdl::property)]
        level: u8,
        #[facet(kdl::child)]
        tuning: Tuning,
        #[facet(kdl::property)]
        gain: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Tuning {
        #[facet(kdl::argument)]
        knob: u8,
    }

    // With a tuning child present, solver must pick Tuned.
    let tuned_kdl = indoc! {r#"
        mode level=3 gain=7 {
            tuning 11
        }
    "#};

    let cfg: Config = facet_kdl_legacy::from_str(tuned_kdl).expect("should select Tuned variant");
    assert_eq!(
        cfg.mode,
        Mode::Tuned(Tuned {
            level: 3,
            gain: 7,
            tuning: Tuning { knob: 11 },
        })
    );

    // Without the tuning child, the same level should map to Simple.
    let simple_kdl = indoc! {r#"
        mode level=3
    "#};

    let cfg: Config = facet_kdl_legacy::from_str(simple_kdl).expect("should select Simple variant");
    assert_eq!(cfg.mode, Mode::Simple(Simple { level: 3 }));
}

/// When fields from *different* flattened variants are mixed together,
/// the solver should fail instead of silently choosing one.
#[test]
fn flatten_enum_mixed_fields_should_error() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        mode: Mode,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Mode {
        Simple(Simple),
        Tuned(Tuned),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        #[facet(kdl::property)]
        level: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Tuned {
        #[facet(kdl::property)]
        level: u8,
        #[facet(kdl::child)]
        tuning: Tuning,
        #[facet(kdl::property)]
        gain: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Tuning {
        #[facet(kdl::argument)]
        knob: u8,
    }

    // Contains the Tuned-only property `gain` but *no* tuning child.
    // Simple cannot accept `gain`, Tuned is missing required `tuning` → error.
    let invalid_kdl = indoc! {r#"
        mode level=3 gain=7
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(invalid_kdl);
    assert!(
        result.is_err(),
        "mixing fields from different flattened variants must error"
    );
}

/// Test Option<T> fields inside flattened structs
#[test]
fn flatten_with_optional_fields() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        cache: CacheConfig,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct CacheConfig {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        settings: CacheSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct CacheSettings {
        #[facet(kdl::property)]
        ttl: u32,
        #[facet(kdl::property)]
        max_size: Option<u64>,
    }

    // With optional field present
    let kdl = indoc! {r#"
        cache "redis" ttl=3600 max_size=1000000
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with optional");
    assert_eq!(config.cache.name, "redis");
    assert_eq!(config.cache.settings.ttl, 3600);
    assert_eq!(config.cache.settings.max_size, Some(1000000));

    // Without optional field - should now work!
    // The solver's missing_optional_fields() API allows us to automatically
    // initialize missing Option<T> fields to None.
    let kdl = indoc! {r#"
        cache "memory" ttl=60
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with missing optional field");
    assert_eq!(config.cache.name, "memory");
    assert_eq!(config.cache.settings.ttl, 60);
    assert_eq!(config.cache.settings.max_size, None);
}

/// Test truly ambiguous configurations - when variants have identical fields
/// with the same types, the solver should error (no way to disambiguate).
#[test]
fn flatten_enum_identical_fields_ambiguous_error() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        resource: Resource,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Resource {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        kind: ResourceKind,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum ResourceKind {
        // Both variants have identical fields - truly ambiguous!
        TypeA(CommonFields),
        TypeB(CommonFields),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct CommonFields {
        #[facet(kdl::property)]
        value: String,
    }

    // When configurations are truly identical (same fields, same types),
    // the solver cannot disambiguate and should error.
    let kdl = indoc! {r#"
        resource "test" value="hello"
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(
        result.is_err(),
        "should error on truly ambiguous configuration"
    );
}

// ============================================================================
// Value-based type disambiguation tests
// ============================================================================
//
// These tests exercise the Solver's ability to disambiguate based on VALUE types,
// not just key presence. This is critical for cases where variants have the same
// field names but different value types (u8 vs u16, etc.).

/// Test (u8, u16) integer range disambiguation.
/// Value 1000 doesn't fit in u8 (max 255) but fits in u16 → should resolve to Large.
#[test]
fn flatten_type_disambiguation_u8_u16_large_value() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        data: DataContainer,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DataContainer {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        payload: IntPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum IntPayload {
        Small(SmallInt),
        Large(LargeInt),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallInt {
        #[facet(kdl::property)]
        value: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargeInt {
        #[facet(kdl::property)]
        value: u16,
    }

    // Value 1000 > 255 (u8::MAX), only u16 can hold it
    let kdl = indoc! {r#"
        data "test" value=1000
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should disambiguate to Large variant (u16 can hold 1000)");

    assert_eq!(config.data.name, "test");
    match &config.data.payload {
        IntPayload::Large(large) => {
            assert_eq!(large.value, 1000);
        }
        IntPayload::Small(_) => panic!("expected Large variant, got Small"),
    }
}

/// Test (u8, u16) with small value that fits both.
/// Value 42 fits in both u8 and u16 → should resolve to first variant (Small).
#[test]
fn flatten_type_disambiguation_u8_u16_small_value() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        data: DataContainer,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DataContainer {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        payload: IntPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum IntPayload {
        Small(SmallInt),
        Large(LargeInt),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallInt {
        #[facet(kdl::property)]
        value: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargeInt {
        #[facet(kdl::property)]
        value: u16,
    }

    // Value 42 fits in both u8 and u16 - first variant (Small) wins
    let kdl = indoc! {r#"
        data "test" value=42
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl)
        .expect("should disambiguate to Small variant (first, value fits both)");

    assert_eq!(config.data.name, "test");
    match &config.data.payload {
        IntPayload::Small(small) => {
            assert_eq!(small.value, 42);
        }
        IntPayload::Large(_) => panic!("expected Small variant (first), got Large"),
    }
}

/// Test (u8, u16) at boundary value 255 (u8::MAX).
/// Value 255 fits in both → should resolve to first variant (Small).
#[test]
fn flatten_type_disambiguation_u8_u16_boundary_255() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        data: DataContainer,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DataContainer {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        payload: IntPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum IntPayload {
        Small(SmallInt),
        Large(LargeInt),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallInt {
        #[facet(kdl::property)]
        value: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargeInt {
        #[facet(kdl::property)]
        value: u16,
    }

    // Value 255 = u8::MAX, fits in both
    let kdl = indoc! {r#"
        data "test" value=255
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should disambiguate to Small variant (255 fits u8)");

    match &config.data.payload {
        IntPayload::Small(small) => {
            assert_eq!(small.value, 255);
        }
        IntPayload::Large(_) => panic!("expected Small variant, got Large"),
    }
}

/// Test (u8, u16) at boundary value 256 (u8::MAX + 1).
/// Value 256 doesn't fit in u8 → should resolve to Large.
#[test]
fn flatten_type_disambiguation_u8_u16_boundary_256() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        data: DataContainer,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DataContainer {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        payload: IntPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum IntPayload {
        Small(SmallInt),
        Large(LargeInt),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallInt {
        #[facet(kdl::property)]
        value: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargeInt {
        #[facet(kdl::property)]
        value: u16,
    }

    // Value 256 > u8::MAX, only u16 can hold it
    let kdl = indoc! {r#"
        data "test" value=256
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should disambiguate to Large variant (256 > u8::MAX)");

    match &config.data.payload {
        IntPayload::Large(large) => {
            assert_eq!(large.value, 256);
        }
        IntPayload::Small(_) => panic!("expected Large variant, got Small"),
    }
}

/// Test (i8, u8) signed/unsigned disambiguation.
/// Negative value -10 only fits in i8 → should resolve to Signed.
#[test]
fn flatten_type_disambiguation_i8_u8_negative() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        data: DataContainer,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DataContainer {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        payload: SignedPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SignedPayload {
        Signed(SignedInt),
        Unsigned(UnsignedInt),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SignedInt {
        #[facet(kdl::property)]
        num: i8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct UnsignedInt {
        #[facet(kdl::property)]
        num: u8,
    }

    // Negative value -10 only fits in i8
    let kdl = indoc! {r#"
        data "test" num=-10
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should disambiguate to Signed variant (negative value)");

    match &config.data.payload {
        SignedPayload::Signed(s) => {
            assert_eq!(s.num, -10);
        }
        SignedPayload::Unsigned(_) => panic!("expected Signed variant, got Unsigned"),
    }
}

/// Test (i8, u8) with value 200.
/// Value 200 fits in u8 (0-255) but not i8 (-128 to 127) → should resolve to Unsigned.
#[test]
fn flatten_type_disambiguation_i8_u8_large_positive() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        data: DataContainer,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DataContainer {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        payload: SignedPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SignedPayload {
        Signed(SignedInt),
        Unsigned(UnsignedInt),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SignedInt {
        #[facet(kdl::property)]
        num: i8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct UnsignedInt {
        #[facet(kdl::property)]
        num: u8,
    }

    // Value 200 > i8::MAX (127), only u8 can hold it
    let kdl = indoc! {r#"
        data "test" num=200
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should disambiguate to Unsigned variant (200 > i8::MAX)");

    match &config.data.payload {
        SignedPayload::Unsigned(u) => {
            assert_eq!(u.num, 200);
        }
        SignedPayload::Signed(_) => panic!("expected Unsigned variant, got Signed"),
    }
}

/// Test the "super annoying" case: same nested path, different types.
/// Both variants have `payload.value` but with different types (u8 vs u16).
///
/// NOTE: This test uses solver integration with child node tracking.
/// The solver now processes child nodes to enable nested value-based disambiguation.
#[test]
fn flatten_super_annoying_same_path_different_types() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        container: Container,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        inner: SuperAnnoyingEnum,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SuperAnnoyingEnum {
        Small(SmallPayloadWrapper),
        Large(LargePayloadWrapper),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallPayloadWrapper {
        #[facet(kdl::child)]
        payload: SmallPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallPayload {
        #[facet(kdl::property)]
        value: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargePayloadWrapper {
        #[facet(kdl::child)]
        payload: LargePayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargePayload {
        #[facet(kdl::property)]
        value: u16,
    }

    // Both variants have "payload" child with "value" property
    // But Small.payload.value is u8, Large.payload.value is u16
    // Value 1000 > 255 → must be Large
    let kdl = indoc! {r#"
        container "test" {
            payload value=1000
        }
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl)
        .expect("should disambiguate by nested value type (1000 > u8::MAX)");

    assert_eq!(config.container.name, "test");
    match &config.container.inner {
        SuperAnnoyingEnum::Large(large) => {
            assert_eq!(large.payload.value, 1000);
        }
        SuperAnnoyingEnum::Small(_) => panic!("expected Large variant, got Small"),
    }
}

/// Test the "super annoying" case with a value that fits both types.
/// Both variants have `payload.value` - value 42 fits both → first variant wins.
///
/// NOTE: This test uses solver integration with child node tracking.
/// The solver now processes child nodes to enable nested value-based disambiguation.
#[test]
fn flatten_super_annoying_same_path_small_value() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        container: Container,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        inner: SuperAnnoyingEnum,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SuperAnnoyingEnum {
        Small(SmallPayloadWrapper),
        Large(LargePayloadWrapper),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallPayloadWrapper {
        #[facet(kdl::child)]
        payload: SmallPayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct SmallPayload {
        #[facet(kdl::property)]
        value: u8,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargePayloadWrapper {
        #[facet(kdl::child)]
        payload: LargePayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LargePayload {
        #[facet(kdl::property)]
        value: u16,
    }

    // Value 42 fits both u8 and u16 → first variant (Small) wins
    let kdl = indoc! {r#"
        container "test" {
            payload value=42
        }
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl)
        .expect("should disambiguate to Small variant (first, value fits both)");

    assert_eq!(config.container.name, "test");
    match &config.container.inner {
        SuperAnnoyingEnum::Small(small) => {
            assert_eq!(small.payload.value, 42);
        }
        SuperAnnoyingEnum::Large(_) => panic!("expected Small variant (first), got Large"),
    }
}

/// Test (i64, f64, String) multi-type discrimination.
/// Integer without decimal → i64, Float with decimal → f64, Quoted string → String.
#[test]
fn flatten_type_disambiguation_int_float_string() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        data: DataContainer,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DataContainer {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        payload: MultiTypePayload,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum MultiTypePayload {
        Int(IntData),
        Float(FloatData),
        Text(TextData),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct IntData {
        #[facet(kdl::property)]
        data: i64,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct FloatData {
        #[facet(kdl::property)]
        data: f64,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct TextData {
        #[facet(kdl::property)]
        data: String,
    }

    // Integer value → Int variant
    let kdl_int = indoc! {r#"
        data "test" data=42
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl_int).expect("should disambiguate to Int variant");
    match &config.data.payload {
        MultiTypePayload::Int(i) => assert_eq!(i.data, 42),
        _ => panic!("expected Int variant"),
    }

    // Float value → Float variant
    let kdl_float = indoc! {r#"
        data "test" data=2.5
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl_float).expect("should disambiguate to Float variant");
    match &config.data.payload {
        MultiTypePayload::Float(f) => assert!((f.data - 2.5).abs() < 0.001),
        _ => panic!("expected Float variant"),
    }

    // String value → Text variant
    let kdl_string = indoc! {r#"
        data "test" data="hello world"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl_string).expect("should disambiguate to Text variant");
    match &config.data.payload {
        MultiTypePayload::Text(t) => assert_eq!(t.data, "hello world"),
        _ => panic!("expected Text variant"),
    }
}

/// Test Option<Flattened> - when a flattened struct is wrapped in Option<T>,
/// it should be None if none of its fields are present, Some if any fields are present.
#[test]
fn option_flattened_struct_absent() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(flatten)]
        advanced: Option<AdvancedSettings>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct AdvancedSettings {
        #[facet(kdl::property)]
        buffer_size: u32,
        #[facet(kdl::property)]
        max_connections: u32,
    }

    // No advanced settings present - should be None
    let kdl = indoc! {r#"
        server "localhost"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with absent optional flattened struct");
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.advanced, None);
}

/// Test Option<Flattened> - when all fields of the flattened struct are present.
#[test]
fn option_flattened_struct_present() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(flatten)]
        advanced: Option<AdvancedSettings>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct AdvancedSettings {
        #[facet(kdl::property)]
        buffer_size: u32,
        #[facet(kdl::property)]
        max_connections: u32,
    }

    // Advanced settings present - should be Some
    let kdl = indoc! {r#"
        server "localhost" buffer_size=4096 max_connections=100
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with present optional flattened struct");
    assert_eq!(config.server.host, "localhost");
    assert_eq!(
        config.server.advanced,
        Some(AdvancedSettings {
            buffer_size: 4096,
            max_connections: 100,
        })
    );
}

/// Test Option<Flattened> with partial fields - when only some fields of the flattened
/// struct are present, the Option is filled and missing fields get defaults.
///
/// Note: When Option<Flattened> is used, the solver marks inner fields as optional.
/// This means if ANY field is present, the Option is filled (Some), and missing fields
/// get their default values.
#[test]
fn option_flattened_struct_partial_fills_defaults() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        #[facet(flatten)]
        advanced: Option<AdvancedSettings>,
    }

    #[derive(Facet, Debug, PartialEq, Default)]
    struct AdvancedSettings {
        #[facet(kdl::property, default)]
        buffer_size: u32,
        #[facet(kdl::property, default)]
        max_connections: u32,
    }

    // Only one field present - Option is filled, missing field gets default
    let kdl = indoc! {r#"
        server "localhost" buffer_size=4096
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl)
        .expect("should parse with partial flattened struct (using defaults)");
    assert_eq!(config.server.host, "localhost");
    assert_eq!(
        config.server.advanced,
        Some(AdvancedSettings {
            buffer_size: 4096,
            max_connections: 0, // default value
        })
    );
}

/// Test Option<Flattened> with nested optional fields.
#[test]
fn option_flattened_struct_with_inner_options() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        cache: CacheConfig,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct CacheConfig {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        tuning: Option<CacheTuning>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct CacheTuning {
        #[facet(kdl::property)]
        ttl: u32,
        #[facet(kdl::property)]
        eviction_policy: Option<String>,
    }

    // No tuning at all
    let kdl = indoc! {r#"
        cache "redis"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with absent optional flattened struct");
    assert_eq!(config.cache.name, "redis");
    assert_eq!(config.cache.tuning, None);

    // Tuning with only required field (inner optional is None)
    let kdl = indoc! {r#"
        cache "redis" ttl=3600
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with partial inner options");
    assert_eq!(config.cache.name, "redis");
    assert_eq!(
        config.cache.tuning,
        Some(CacheTuning {
            ttl: 3600,
            eviction_policy: None,
        })
    );

    // Tuning with all fields
    let kdl = indoc! {r#"
        cache "redis" ttl=3600 eviction_policy="lru"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with all tuning fields");
    assert_eq!(config.cache.name, "redis");
    assert_eq!(
        config.cache.tuning,
        Some(CacheTuning {
            ttl: 3600,
            eviction_policy: Some("lru".to_string()),
        })
    );
}

/// Test that duplicate field names from different sources produce an error.
/// This can happen when a parent struct and a flattened struct both define
/// a field with the same name.
#[test]
fn duplicate_field_detection() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        host: String,
        // This defines a 'port' property on Server
        #[facet(kdl::property)]
        port: u16,
        // This flattened struct ALSO defines a 'port' property
        #[facet(flatten)]
        connection: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ConnectionSettings {
        #[facet(kdl::property)]
        port: u16, // Duplicate! Same name as Server.port
        #[facet(kdl::property)]
        timeout: u32,
    }

    // The schema should detect the duplicate field name
    let kdl = indoc! {r#"
        server "localhost" port=8080 timeout=30
    "#};

    let result: Result<Config, _> = facet_kdl_legacy::from_str(kdl);
    assert!(result.is_err(), "should error on duplicate field");
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    eprintln!("Error message: {err_msg}");
    // The error message should mention the duplicate field
    assert!(
        err_msg.contains("port") || err_msg.contains("Duplicate"),
        "error should mention duplicate field: {err_msg}"
    );
}

// ============================================================================
// Custom deserialization with proxy attribute
// ============================================================================

/// Test custom deserialization for property fields using proxy
#[test]
fn proxy_property() {
    use std::num::IntErrorKind;

    // Opaque type that doesn't implement Facet for its inner value directly
    #[derive(Debug, PartialEq)]
    struct HexValue(u64);

    // Proxy type for deserializing HexValue from String
    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct HexValueProxy(String);

    impl TryFrom<HexValueProxy> for HexValue {
        type Error = &'static str;
        fn try_from(proxy: HexValueProxy) -> Result<Self, Self::Error> {
            let s = &proxy.0;
            if let Some(hex) = s.strip_prefix("0x") {
                u64::from_str_radix(hex, 16)
            } else {
                s.parse()
            }
            .map(HexValue)
            .map_err(|e| match e.kind() {
                IntErrorKind::Empty => "cannot parse integer from empty string",
                IntErrorKind::InvalidDigit => "invalid digit found in string",
                IntErrorKind::PosOverflow => "number too large to fit in target type",
                IntErrorKind::NegOverflow => "number too small to fit in target type",
                IntErrorKind::Zero => "number would be zero for non-zero type",
                _ => "unknown error",
            })
        }
    }

    impl From<&HexValue> for HexValueProxy {
        fn from(v: &HexValue) -> Self {
            HexValueProxy(format!("0x{:x}", v.0))
        }
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        item: Item,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::property, opaque, proxy = HexValueProxy)]
        value: HexValue,
    }

    let kdl = indoc! {r#"
        item "test" value="0xff"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with proxy");
    assert_eq!(config.item.name, "test");
    assert_eq!(config.item.value, HexValue(255));
}

/// Test custom deserialization for argument fields using proxy
#[test]
fn proxy_argument() {
    use std::num::IntErrorKind;

    #[derive(Debug, PartialEq)]
    struct HexValue(u64);

    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct HexValueProxy(String);

    impl TryFrom<HexValueProxy> for HexValue {
        type Error = &'static str;
        fn try_from(proxy: HexValueProxy) -> Result<Self, Self::Error> {
            let s = &proxy.0;
            if let Some(hex) = s.strip_prefix("0x") {
                u64::from_str_radix(hex, 16)
            } else {
                s.parse()
            }
            .map(HexValue)
            .map_err(|e| match e.kind() {
                IntErrorKind::Empty => "cannot parse integer from empty string",
                IntErrorKind::InvalidDigit => "invalid digit found in string",
                _ => "unknown error",
            })
        }
    }

    impl From<&HexValue> for HexValueProxy {
        fn from(v: &HexValue) -> Self {
            HexValueProxy(format!("0x{:x}", v.0))
        }
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        item: Item,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        #[facet(kdl::argument, opaque, proxy = HexValueProxy)]
        code: HexValue,
    }

    let kdl = indoc! {r#"
        item "0xabc"
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with proxy on argument");
    assert_eq!(config.item.code, HexValue(0xabc));
}

/// Test custom deserialization with flattened struct (solver path) using proxy
#[test]
fn proxy_flattened() {
    use std::num::IntErrorKind;

    #[derive(Debug, PartialEq)]
    struct HexValue(u64);

    #[derive(Facet, Clone)]
    #[facet(transparent)]
    struct HexValueProxy(String);

    impl TryFrom<HexValueProxy> for HexValue {
        type Error = &'static str;
        fn try_from(proxy: HexValueProxy) -> Result<Self, Self::Error> {
            let s = &proxy.0;
            if let Some(hex) = s.strip_prefix("0x") {
                u64::from_str_radix(hex, 16)
            } else {
                s.parse()
            }
            .map(HexValue)
            .map_err(|e| match e.kind() {
                IntErrorKind::Empty => "cannot parse integer from empty string",
                IntErrorKind::InvalidDigit => "invalid digit found in string",
                _ => "unknown error",
            })
        }
    }

    impl From<&HexValue> for HexValueProxy {
        fn from(v: &HexValue) -> Self {
            HexValueProxy(format!("0x{:x}", v.0))
        }
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        item: Item,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        settings: Settings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Settings {
        #[facet(kdl::property, opaque, proxy = HexValueProxy)]
        code: HexValue,
        #[facet(kdl::property, default)]
        extra: Option<String>,
    }

    let kdl = indoc! {r#"
        item "test" code="0xdead"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with proxy in flattened struct");
    assert_eq!(config.item.name, "test");
    assert_eq!(config.item.settings.code, HexValue(0xdead));
    assert_eq!(config.item.settings.extra, None);
}

// ============================================================================
// Flattened field with #[facet(default)]
// ============================================================================

/// Test that a flattened struct with #[facet(default)] uses the Default impl
/// when none of its fields are present.
#[test]
fn flatten_with_default_absent() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten, default)]
        settings: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq, Default)]
    struct ConnectionSettings {
        #[facet(kdl::property, default)]
        port: u16,
        #[facet(kdl::property, default)]
        timeout: u32,
    }

    // No settings properties present - should use Default
    let kdl = indoc! {r#"
        server "main"
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with absent default flattened struct");
    assert_eq!(config.server.name, "main");
    assert_eq!(config.server.settings, ConnectionSettings::default());
}

/// Test that a flattened struct with #[facet(default)] still works when fields are present.
#[test]
fn flatten_with_default_present() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten, default)]
        settings: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq, Default)]
    struct ConnectionSettings {
        #[facet(kdl::property, default)]
        port: u16,
        #[facet(kdl::property, default)]
        timeout: u32,
    }

    // Settings properties present - should parse normally
    let kdl = indoc! {r#"
        server "main" port=8080 timeout=30
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse with present default flattened struct");
    assert_eq!(config.server.name, "main");
    assert_eq!(
        config.server.settings,
        ConnectionSettings {
            port: 8080,
            timeout: 30,
        }
    );
}

// ============================================================================
// Round-trip tests for flatten + serialization
// ============================================================================

/// Test that serializing a flattened struct and deserializing it back produces the same value.
#[test]
fn flatten_round_trip_simple() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        connection: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ConnectionSettings {
        #[facet(kdl::property)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    let original = Config {
        server: Server {
            name: "main".to_string(),
            connection: ConnectionSettings {
                host: "localhost".to_string(),
                port: 8080,
            },
        },
    };

    // Serialize
    let kdl = facet_kdl_legacy::to_string(&original).expect("should serialize");
    eprintln!("Serialized:\n{kdl}");

    // Deserialize back
    let parsed: Config = facet_kdl_legacy::from_str(&kdl).expect("should deserialize");
    assert_eq!(parsed, original, "round-trip should preserve value");
}

/// Test round-trip with interleaved flattened properties (properties from different
/// flattened structs may be interleaved in KDL).
#[test]
fn flatten_round_trip_interleaved() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::property)]
        enabled: bool,
        #[facet(flatten)]
        connection: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ConnectionSettings {
        #[facet(kdl::property)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    // Parse interleaved properties (enabled is between connection fields)
    let kdl = indoc! {r#"
        server "main" host="localhost" enabled=#true port=8080
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse interleaved properties");
    assert_eq!(config.server.name, "main");
    assert!(config.server.enabled);
    assert_eq!(config.server.connection.host, "localhost");
    assert_eq!(config.server.connection.port, 8080);

    // Round-trip
    let serialized = facet_kdl_legacy::to_string(&config).expect("should serialize");
    eprintln!("Serialized:\n{serialized}");
    let reparsed: Config = facet_kdl_legacy::from_str(&serialized).expect("should deserialize");
    assert_eq!(
        reparsed, config,
        "round-trip should preserve interleaved value"
    );
}

/// Test round-trip with flattened enum (solver-based disambiguation).
#[test]
fn flatten_round_trip_enum() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        source: Source,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Source {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        kind: SourceKind,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SourceKind {
        Local(LocalSource),
        Remote(RemoteSource),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LocalSource {
        #[facet(kdl::property)]
        path: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct RemoteSource {
        #[facet(kdl::property)]
        url: String,
        #[facet(kdl::property, default)]
        timeout: Option<u32>,
    }

    // Test Local variant
    let local_config = Config {
        source: Source {
            name: "local-files".to_string(),
            kind: SourceKind::Local(LocalSource {
                path: "/data/files".to_string(),
            }),
        },
    };

    let kdl = facet_kdl_legacy::to_string(&local_config).expect("should serialize Local");
    eprintln!("Local serialized:\n{kdl}");
    let reparsed: Config = facet_kdl_legacy::from_str(&kdl).expect("should deserialize Local");
    assert_eq!(reparsed, local_config);

    // Test Remote variant
    let remote_config = Config {
        source: Source {
            name: "api".to_string(),
            kind: SourceKind::Remote(RemoteSource {
                url: "https://api.example.com".to_string(),
                timeout: Some(30),
            }),
        },
    };

    let kdl = facet_kdl_legacy::to_string(&remote_config).expect("should serialize Remote");
    eprintln!("Remote serialized:\n{kdl}");
    let reparsed: Config = facet_kdl_legacy::from_str(&kdl).expect("should deserialize Remote");
    assert_eq!(reparsed, remote_config);
}

// ============================================================================
// Mixed children + flatten ordering tests
// ============================================================================

/// Test Option<Child> without flatten (baseline test)
#[test]
#[allow(dead_code)]
fn option_child_without_flatten() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        #[facet(kdl::property)]
        host: String,
        #[facet(kdl::child, default)]
        logging: Option<Logging>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Logging {
        #[facet(kdl::property)]
        level: String,
    }

    let kdl = indoc! {r#"
        server "main" host="localhost" {
            logging level="debug"
        }
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with optional child");
    assert_eq!(config.server.name, "main");
    assert_eq!(config.server.host, "localhost");
    assert_eq!(
        config.server.logging,
        Some(Logging {
            level: "debug".to_string(),
        })
    );
}

/// Test that flattened struct with sibling children on parent work together.
/// Children on the parent alongside flattened properties.
#[test]
#[allow(dead_code)]
fn flatten_with_sibling_children() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        server: Server,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        #[facet(kdl::argument)]
        name: String,
        // Properties from flattened struct
        #[facet(flatten)]
        connection: ConnectionSettings,
        // Child on the parent, not inside the flattened struct
        #[facet(kdl::child, default)]
        logging: Option<Logging>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct ConnectionSettings {
        #[facet(kdl::property)]
        host: String,
        #[facet(kdl::property)]
        port: u16,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Logging {
        #[facet(kdl::property)]
        level: String,
    }

    // Test with flattened properties and parent child
    let kdl = indoc! {r#"
        server "main" host="localhost" port=8080 {
            logging level="debug"
        }
    "#};

    let config: Config = facet_kdl_legacy::from_str(kdl).expect("should parse with sibling children");
    assert_eq!(config.server.name, "main");
    assert_eq!(config.server.connection.host, "localhost");
    assert_eq!(config.server.connection.port, 8080);
    assert_eq!(
        config.server.logging,
        Some(Logging {
            level: "debug".to_string(),
        })
    );

    // Round-trip test
    let serialized = facet_kdl_legacy::to_string(&config).expect("should serialize");
    eprintln!("Sibling children serialized:\n{serialized}");
    let reparsed: Config = facet_kdl_legacy::from_str(&serialized).expect("should deserialize");
    assert_eq!(reparsed, config);
}

/// Test flattened enum with children that disambiguates based on child presence.
#[test]
#[allow(dead_code)]
fn flatten_enum_child_disambiguation() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(kdl::child)]
        storage: Storage,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Storage {
        #[facet(kdl::argument)]
        name: String,
        #[facet(flatten)]
        backend: StorageBackend,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum StorageBackend {
        // Local has a 'cache' child
        Local(LocalBackend),
        // S3 has a 'credentials' child
        S3(S3Backend),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct LocalBackend {
        #[facet(kdl::property)]
        path: String,
        #[facet(kdl::child, default)]
        cache: Option<CacheConfig>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct S3Backend {
        #[facet(kdl::property)]
        bucket: String,
        #[facet(kdl::child)]
        credentials: Credentials,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct CacheConfig {
        #[facet(kdl::property)]
        size_mb: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Credentials {
        #[facet(kdl::property)]
        key_id: String,
        #[facet(kdl::property)]
        secret: String,
    }

    // Local backend with cache child
    let kdl = indoc! {r#"
        storage "local-store" path="/data" {
            cache size_mb=1024
        }
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse Local backend with cache child");
    match &config.storage.backend {
        StorageBackend::Local(local) => {
            assert_eq!(local.path, "/data");
            assert_eq!(local.cache, Some(CacheConfig { size_mb: 1024 }));
        }
        _ => panic!("expected Local backend"),
    }

    // S3 backend with credentials child
    let kdl = indoc! {r#"
        storage "s3-store" bucket="my-bucket" {
            credentials key_id="AKIA..." secret="secret123"
        }
    "#};

    let config: Config =
        facet_kdl_legacy::from_str(kdl).expect("should parse S3 backend with credentials child");
    match &config.storage.backend {
        StorageBackend::S3(s3) => {
            assert_eq!(s3.bucket, "my-bucket");
            assert_eq!(s3.credentials.key_id, "AKIA...");
            assert_eq!(s3.credentials.secret, "secret123");
        }
        _ => panic!("expected S3 backend"),
    }
}
