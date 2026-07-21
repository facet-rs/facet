//! End-to-end certificate for a type-safe second-order primitive.
//!
//! Vix supplies `fn(HttpRequest) -> HttpResponse`; Rust owns request delivery
//! and invokes the retained callback. No raw primitive descriptor, request
//! parser, `PrimitiveValue`, callback registry, or frame ABI appears here.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use facet::Facet;
use vix::compiler::Compiler;
use vix::lowering::{LoweringCache, attribution_for};
use vix::runtime::{
    ArgRoleDecl, Callback, CallbackError, ChaosPolicy, Evaluation, EventLog, FromRef, IslandInputs,
    Location, PrimitiveDecl, PrimitiveDispatcher, PrimitiveMachineError, PrimitiveMemoPolicy,
    PrimitiveRegistry, Runtime,
};
use vix::vir::ValueIslandId;
use vixen_primitives::{CallbackExt, EffectTicket, Primitive, TypedAdapter};

#[derive(Facet, Clone, Debug, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    path: String,
}

#[derive(Facet, Clone, Debug, PartialEq, Eq)]
struct HttpResponse {
    status: i64,
    body: String,
}

#[derive(Facet, Clone)]
struct ServeHttpRequest {
    on_request: Callback<HttpRequest, HttpResponse>,
}

/// A deliberately tiny one-request HTTP server. Socket IO, HTTP parsing, and
/// response writing stay in Rust; only the request handler is supplied by Vix.
struct RustHttpServer {
    listener: Mutex<Option<TcpListener>>,
    responses: Mutex<Vec<HttpResponse>>,
}

impl RustHttpServer {
    fn bind() -> Self {
        Self {
            listener: Mutex::new(Some(
                TcpListener::bind("127.0.0.1:0").expect("the HTTP test binds localhost"),
            )),
            responses: Mutex::new(Vec::new()),
        }
    }

    fn local_addr(&self) -> SocketAddr {
        self.listener
            .lock()
            .expect("HTTP listener mutex poisoned")
            .as_ref()
            .expect("the HTTP listener has not accepted yet")
            .local_addr()
            .expect("the HTTP test listener has a local address")
    }

    fn serve(
        &self,
        on_request: &Callback<HttpRequest, HttpResponse>,
    ) -> Result<HttpResponse, CallbackError> {
        let listener = self
            .listener
            .lock()
            .expect("HTTP listener mutex poisoned")
            .take()
            .expect("the test server accepts one request");
        let (mut stream, _) = listener.accept().map_err(callback_io_error)?;
        let mut request_line = String::new();
        BufReader::new(&stream)
            .read_line(&mut request_line)
            .map_err(callback_io_error)?;
        let mut words = request_line.split_whitespace();
        let request = HttpRequest {
            method: words
                .next()
                .ok_or_else(|| callback_runtime_error("HTTP request has no method"))?
                .to_owned(),
            path: words
                .next()
                .ok_or_else(|| callback_runtime_error("HTTP request has no path"))?
                .to_owned(),
        };
        let response = on_request.call(request)?;
        write!(
            stream,
            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.status,
            response.body.len(),
            response.body,
        )
        .map_err(callback_io_error)?;
        stream.flush().map_err(callback_io_error)?;
        self.responses
            .lock()
            .expect("HTTP response mutex poisoned")
            .push(response.clone());
        Ok(response)
    }
}

fn callback_runtime_error(detail: impl Into<String>) -> CallbackError {
    CallbackError::Runtime {
        detail: detail.into(),
    }
}

fn callback_io_error(error: std::io::Error) -> CallbackError {
    callback_runtime_error(error.to_string())
}

struct HttpApp {
    server: Arc<RustHttpServer>,
}

#[derive(Clone)]
struct HttpDeps {
    server: Arc<RustHttpServer>,
}

impl FromRef<HttpApp> for HttpDeps {
    fn from_ref(app: &HttpApp) -> Self {
        Self {
            server: Arc::clone(&app.server),
        }
    }
}

struct ServeHttp;

impl Primitive<HttpApp> for ServeHttp {
    type Request = ServeHttpRequest;
    type Response = HttpResponse;
    type Deps = HttpDeps;

    const DECL: PrimitiveDecl = PrimitiveDecl {
        namespace: "vix.test",
        name: "serve_http",
        id_name: "serve-http",
        version: 1,
        memo_policy: PrimitiveMemoPolicy::Volatile,
        protocol_version: 1,
        failure_schema_name: "HttpServerFailure",
        capabilities: &[],
        args: &[ArgRoleDecl::Value],
    };

    fn begin(
        &self,
        request: ServeHttpRequest,
        ctx: vix::runtime::EffectCtx,
        deps: HttpDeps,
    ) -> EffectTicket<HttpResponse> {
        let (ticket, completer) = EffectTicket::pair(&ctx, || {});
        std::thread::spawn(move || {
            let completion = match deps.server.serve(&request.on_request) {
                Ok(response) => completer.complete_value(&ctx, response),
                Err(error) => completer.complete_err(
                    &ctx,
                    PrimitiveMachineError::Unavailable {
                        detail: format!("Vix HTTP handler failed: {error:?}"),
                    },
                ),
            };
            completion.expect("the HTTP primitive completes exactly once");
        });
        ticket
    }
}

const SOURCE: &str = r#"
struct HttpRequest { method: String, path: String }
struct HttpResponse { status: Int, body: String }

fn on_request(request: HttpRequest) -> HttpResponse {
    HttpResponse { status: 200, body: request.path }
}

#[test]
fn rust_http_server_calls_vix() -> Stream<Check> {
    let response = serve_http(on_request);
    yield expect_eq(response.body, "/hello");
}
"#;

const WRONG_HANDLER_SOURCE: &str = r#"
struct HttpRequest { method: String, path: String }
struct HttpResponse { status: Int, body: String }

fn wrong_handler(request: HttpRequest) -> Int { 200 }

#[test]
fn wrong_callback_type() -> Stream<Check> {
    let response = serve_http(wrong_handler);
    yield expect_eq(response.body, "/hello");
}
"#;

fn http_compiler(adapter: &TypedAdapter<ServeHttp>) -> Compiler {
    Compiler::with_config(vixen_runtime::default_config())
        .with_primitive_surfaces([adapter.surface()])
}

#[test]
fn compiler_rejects_a_handler_with_the_wrong_response_type() {
    let adapter = TypedAdapter::new::<HttpApp>(ServeHttp);
    let error = http_compiler(&adapter)
        .compile(WRONG_HANDLER_SOURCE)
        .expect_err("serve_http requires fn(HttpRequest) -> HttpResponse");
    let error = format!("{error:?}");
    assert!(
        error.contains("TypeMismatch") && error.contains("HttpResponse"),
        "the callback result mismatch is diagnosed at compile time: {error}"
    );
}

#[test]
fn primitive_registers_a_typed_vix_callback_with_a_rust_http_server() {
    let adapter = Arc::new(TypedAdapter::new::<HttpApp>(ServeHttp));
    let module = http_compiler(&adapter)
        .compile(SOURCE)
        .expect("the callback parameter and response are checked by the Vix compiler");
    let partitioned = module.partition_test(&module.tests[0]);

    let mut registry = PrimitiveRegistry::<HttpApp>::default();
    registry
        .register(adapter)
        .expect("the typed HTTP primitive registers once");

    let server = Arc::new(RustHttpServer::bind());
    let address = server.local_addr();
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(address).expect("the HTTP client connects");
        stream
            .write_all(b"GET /hello HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("the HTTP client writes its request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("the HTTP client reads its response");
        response
    });
    let app = HttpApp {
        server: Arc::clone(&server),
    };
    let mut runtime = Runtime::with_context(EventLog::default(), app);
    runtime.set_primitive_dispatcher(PrimitiveDispatcher::new(Arc::new(registry)));

    let mut cache = LoweringCache::default();
    let mut published = BTreeMap::<ValueIslandId, Evaluation>::new();
    for value in &partitioned.values {
        let arguments = value
            .island
            .value_inputs
            .iter()
            .map(|input| published[input].clone())
            .collect();
        let evaluation = runtime
            .evaluate(
                value.island.id,
                &Location::for_test_value(&partitioned.name, &value.id.stable_segment()),
                cache
                    .get_or_lower_owned(&value.island)
                    .expect("HTTP value island lowers"),
                &attribution_for(&value.island),
                IslandInputs {
                    arguments,
                    wires: Vec::new(),
                },
                ChaosPolicy::default(),
            )
            .expect("Rust delivers its request and the Vix callback responds");
        published.insert(value.id, evaluation);
    }

    let check = &partitioned.islands[0];
    let evaluation = runtime
        .evaluate(
            check.id,
            &Location::for_test_value(&partitioned.name, "check"),
            cache
                .get_or_lower_owned(check)
                .expect("HTTP response check lowers"),
            &attribution_for(check),
            IslandInputs {
                arguments: check
                    .value_inputs
                    .iter()
                    .map(|input| published[input].clone())
                    .collect(),
                wires: Vec::new(),
            },
            ChaosPolicy::default(),
        )
        .expect("the Vix check runs");
    assert!(
        evaluation.passed,
        "the Vix handler response reaches the test"
    );
    let wire_response = client.join().expect("the HTTP client thread exits");
    assert!(
        wire_response.starts_with("HTTP/1.1 200 OK\r\n"),
        "Rust writes the Vix status as HTTP: {wire_response:?}"
    );
    assert!(
        wire_response.ends_with("\r\n\r\n/hello"),
        "Rust writes the Vix body as HTTP: {wire_response:?}"
    );

    assert_eq!(
        server
            .responses
            .lock()
            .expect("HTTP response mutex poisoned")
            .as_slice(),
        &[HttpResponse {
            status: 200,
            body: "/hello".to_owned(),
        }],
        "Rust received the strongly typed response produced by Vix",
    );
}
