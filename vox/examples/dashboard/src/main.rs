//! Rapace Dashboard Server
//!
//! Provides an HTTP API and web UI for exploring and calling rapace services.
//!
//! ## Endpoints
//!
//! - `GET /api/services` - List all registered services
//! - `GET /api/services/{id}` - Get details for a specific service
//! - `POST /api/call` - Call a method on a service
//! - `GET /api/stream` - Stream a method's results via SSE
//! - `GET /` - Serve the dashboard UI

use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use rapace_registry::ServiceRegistry;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

// ============================================================================
// API response types
// ============================================================================

/// Summary of a service for the list view.
#[derive(Serialize)]
struct ServiceSummary {
    id: u32,
    name: String,
    doc: String,
    method_count: usize,
}

/// Full details of a service including methods.
#[derive(Serialize)]
struct ServiceDetail {
    id: u32,
    name: String,
    doc: String,
    methods: Vec<MethodDetail>,
}

/// Details of an argument to a method.
#[derive(Serialize)]
struct ArgDetail {
    name: String,
    type_name: String,
}

/// Details of a method.
#[derive(Serialize)]
struct MethodDetail {
    id: u32,
    name: String,
    full_name: String,
    doc: String,
    args: Vec<ArgDetail>,
    is_streaming: bool,
    encodings: Vec<String>,
    request_type: String,
    response_type: String,
}

/// Error response.
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Request to call a method.
#[derive(Deserialize)]
struct CallRequest {
    service: String,
    method: String,
    #[serde(default)]
    args: serde_json::Value,
    /// Binary arguments as base64-encoded strings, keyed by field name.
    #[serde(default)]
    binary_args: HashMap<String, String>,
}

/// Response from calling a method.
#[derive(Serialize)]
struct CallResponse {
    result: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// ============================================================================
// Application state
// ============================================================================

struct AppState {
    registry: ServiceRegistry,
    calculator: CalculatorImpl,
    greeter: GreeterImpl,
    counter: CounterImpl,
}

// ============================================================================
// API handlers
// ============================================================================

/// GET /api/services - List all services.
async fn list_services(State(state): State<Arc<AppState>>) -> Json<Vec<ServiceSummary>> {
    let services: Vec<ServiceSummary> = state
        .registry
        .services()
        .map(|service| ServiceSummary {
            id: service.id.0,
            name: service.name.to_string(),
            doc: service.doc.clone(),
            method_count: service.methods.len(),
        })
        .collect();

    Json(services)
}

/// GET /api/services/{id} - Get service details.
async fn get_service(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
) -> Result<Json<ServiceDetail>, (StatusCode, Json<ErrorResponse>)> {
    let service = state
        .registry
        .service_by_id(rapace_registry::ServiceId(id))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Service with id {} not found", id),
                }),
            )
        })?;

    let methods: Vec<MethodDetail> = service
        .methods
        .values()
        .map(|method| MethodDetail {
            id: method.id.0,
            name: method.name.to_string(),
            full_name: method.full_name.clone(),
            doc: method.doc.clone(),
            args: method
                .args
                .iter()
                .map(|arg| ArgDetail {
                    name: arg.name.to_string(),
                    type_name: arg.type_name.to_string(),
                })
                .collect(),
            is_streaming: method.is_streaming,
            encodings: method
                .supported_encodings
                .iter()
                .map(|e| format!("{:?}", e))
                .collect(),
            request_type: format!("{}", method.request_shape),
            response_type: format!("{}", method.response_shape),
        })
        .collect();

    Ok(Json(ServiceDetail {
        id: service.id.0,
        name: service.name.to_string(),
        doc: service.doc.clone(),
        methods,
    }))
}

/// POST /api/call - Call a method on a service.
async fn call_method(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CallRequest>,
) -> Result<Json<CallResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if method is streaming
    let method = state
        .registry
        .lookup_method(&req.service, &req.method)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Method {}.{} not found", req.service, req.method),
                }),
            )
        })?;

    if method.is_streaming {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Streaming methods are not yet supported. Use unary methods only.".into(),
            }),
        ));
    }

    // Dispatch to the appropriate service
    let result = match req.service.as_str() {
        "Calculator" => dispatch_calculator(&state.calculator, &req.method, &req.args).await,
        "Greeter" => dispatch_greeter(&state.greeter, &req.method, &req.args, &req.binary_args).await,
        _ => Err(format!("Service {} is not callable", req.service)),
    };

    match result {
        Ok(value) => Ok(Json(CallResponse {
            result: value,
            error: None,
        })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: e }),
        )),
    }
}

/// Dispatch Calculator method calls.
async fn dispatch_calculator(
    calc: &CalculatorImpl,
    method: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "add" => {
            let a = args
                .get("a")
                .or_else(|| args.get(0))
                .and_then(|v| v.as_i64())
                .ok_or("Missing or invalid argument 'a' (expected i32)")?
                as i32;
            let b = args
                .get("b")
                .or_else(|| args.get(1))
                .and_then(|v| v.as_i64())
                .ok_or("Missing or invalid argument 'b' (expected i32)")?
                as i32;
            let result = calc.add(a, b).await;
            Ok(serde_json::json!(result))
        }
        "multiply" => {
            let a = args
                .get("a")
                .or_else(|| args.get(0))
                .and_then(|v| v.as_i64())
                .ok_or("Missing or invalid argument 'a' (expected i32)")?
                as i32;
            let b = args
                .get("b")
                .or_else(|| args.get(1))
                .and_then(|v| v.as_i64())
                .ok_or("Missing or invalid argument 'b' (expected i32)")?
                as i32;
            let result = calc.multiply(a, b).await;
            Ok(serde_json::json!(result))
        }
        "factorial" => {
            let n = args
                .get("n")
                .or_else(|| args.get(0))
                .and_then(|v| v.as_u64())
                .ok_or("Missing or invalid argument 'n' (expected u32)")?
                as u32;
            let result = calc.factorial(n).await;
            Ok(serde_json::json!(result))
        }
        _ => Err(format!("Unknown Calculator method: {}", method)),
    }
}

/// Dispatch Greeter method calls.
async fn dispatch_greeter(
    greeter: &GreeterImpl,
    method: &str,
    args: &serde_json::Value,
    binary_args: &HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    // Helper to get string arg, checking binary_args first for base64 data
    let get_string_arg = |name: &str, index: usize| -> Result<String, String> {
        // Check binary_args first (for file uploads)
        if let Some(base64_data) = binary_args.get(name) {
            let bytes = base64_decode(base64_data)
                .map_err(|e| format!("Invalid base64 for '{}': {}", name, e))?;
            return String::from_utf8(bytes)
                .map_err(|e| format!("Invalid UTF-8 in '{}': {}", name, e));
        }

        // Fall back to JSON args
        args.get(name)
            .or_else(|| args.get(index))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("Missing or invalid argument '{}' (expected string)", name))
    };

    match method {
        "greet" => {
            let name = get_string_arg("name", 0)?;
            let result = greeter.greet(name).await;
            Ok(serde_json::json!(result))
        }
        "greet_formal" => {
            let title = get_string_arg("title", 0)?;
            let name = get_string_arg("name", 1)?;
            let result = greeter.greet_formal(title, name).await;
            Ok(serde_json::json!(result))
        }
        _ => Err(format!("Unknown Greeter method: {}", method)),
    }
}

/// Decode base64 string to bytes.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    // Simple base64 decoder
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn char_to_val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => None, // padding
            _ => None,
        }
    }

    let input: Vec<u8> = s.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ').collect();
    if !input.len().is_multiple_of(4) {
        return Err("Invalid base64 length".into());
    }

    let mut output = Vec::with_capacity(input.len() * 3 / 4);

    for chunk in input.chunks(4) {
        let a = char_to_val(chunk[0]).ok_or("Invalid base64 character")?;
        let b = char_to_val(chunk[1]).ok_or("Invalid base64 character")?;
        let c_opt = char_to_val(chunk[2]);
        let d_opt = char_to_val(chunk[3]);

        output.push((a << 2) | (b >> 4));
        if let Some(c) = c_opt {
            output.push((b << 4) | (c >> 2));
            if let Some(d) = d_opt {
                output.push((c << 6) | d);
            }
        }
    }

    let _ = ALPHABET; // suppress warning
    Ok(output)
}

/// Query parameters for streaming endpoint.
#[derive(Deserialize)]
struct StreamQuery {
    service: String,
    method: String,
    #[serde(default)]
    n: Option<u32>,
}

/// GET /api/stream - Stream a method's results via SSE.
async fn stream_method(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Check if method exists and is streaming
    let method = state
        .registry
        .lookup_method(&query.service, &query.method)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Method {}.{} not found", query.service, query.method),
                }),
            )
        })?;

    if !method.is_streaming {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "This endpoint is only for streaming methods. Use /api/call for unary methods.".into(),
            }),
        ));
    }

    // Only Counter service has streaming methods
    if query.service != "Counter" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Service {} does not have streaming support", query.service),
            }),
        ));
    }

    let n = query.n.unwrap_or(10);

    type PinnedEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

    let stream: PinnedEventStream = match query.method.as_str() {
        "count_to" => {
            let inner_stream = state.counter.count_to(n).await;
            Box::pin(futures::stream::StreamExt::map(inner_stream, |result| {
                match result {
                    Ok(value) => Ok(Event::default().data(value.to_string())),
                    Err(e) => Ok(Event::default().event("error").data(format!("{:?}", e))),
                }
            }))
        }
        "fibonacci" => {
            let inner_stream = state.counter.fibonacci(n).await;
            Box::pin(futures::stream::StreamExt::map(inner_stream, |result| {
                match result {
                    Ok(value) => Ok(Event::default().data(value.to_string())),
                    Err(e) => Ok(Event::default().event("error").data(format!("{:?}", e))),
                }
            }))
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Unknown Counter method: {}", query.method),
                }),
            ));
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// GET / - Serve the dashboard UI.
async fn dashboard_ui() -> impl IntoResponse {
    Html(include_str!("../static/index.html"))
}

// ============================================================================
// Example services for demonstration
// ============================================================================

/// A calculator service for demonstration.
///
/// Provides basic arithmetic operations like addition and multiplication.
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
pub trait Calculator {
    /// Add two numbers together.
    ///
    /// Returns the sum of `a` and `b`.
    async fn add(&self, a: i32, b: i32) -> i32;

    /// Multiply two numbers.
    ///
    /// Returns the product of `a` and `b`.
    async fn multiply(&self, a: i32, b: i32) -> i32;

    /// Compute the factorial of a number.
    ///
    /// Returns n! (n factorial). For n=0, returns 1.
    async fn factorial(&self, n: u32) -> u64;
}

/// Calculator implementation.
struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn multiply(&self, a: i32, b: i32) -> i32 {
        a * b
    }

    async fn factorial(&self, n: u32) -> u64 {
        (1..=n as u64).product()
    }
}

/// A greeting service for demonstration.
///
/// Provides friendly greeting messages in various formats.
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
pub trait Greeter {
    /// Generate a simple greeting.
    ///
    /// Returns "Hello, {name}!" for the given name.
    async fn greet(&self, name: String) -> String;

    /// Generate a formal greeting.
    ///
    /// Returns a more formal greeting with title and name.
    async fn greet_formal(&self, title: String, name: String) -> String;
}

/// Greeter implementation.
struct GreeterImpl;

impl Greeter for GreeterImpl {
    async fn greet(&self, name: String) -> String {
        format!("Hello, {}!", name)
    }

    async fn greet_formal(&self, title: String, name: String) -> String {
        format!("Good day, {} {}. How may I assist you?", title, name)
    }
}

/// Counter implementation.
struct CounterImpl;

impl Counter for CounterImpl {
    async fn count_to(&self, n: u32) -> rapace_core::Streaming<u32> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            for i in 0..n {
                // Add a small delay to make streaming visible
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if tx.send(Ok(i)).await.is_err() {
                    break;
                }
            }
        });
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }

    async fn fibonacci(&self, n: u32) -> rapace_core::Streaming<u64> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            let mut a: u64 = 0;
            let mut b: u64 = 1;
            for _ in 0..n {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if tx.send(Ok(a)).await.is_err() {
                    break;
                }
                let next = a.saturating_add(b);
                a = b;
                b = next;
            }
        });
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

/// A counter service that demonstrates streaming.
///
/// Provides methods for counting and streaming sequences.
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
pub trait Counter {
    /// Count from 0 to n-1.
    ///
    /// Returns a stream of numbers from 0 up to (but not including) n.
    async fn count_to(&self, n: u32) -> rapace_core::Streaming<u32>;

    /// Generate Fibonacci numbers.
    ///
    /// Returns a stream of the first n Fibonacci numbers.
    async fn fibonacci(&self, n: u32) -> rapace_core::Streaming<u64>;
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    // Create and populate the registry
    let mut registry = ServiceRegistry::new();

    // Register our demo services
    calculator_methods::register(&mut registry);
    greeter_methods::register(&mut registry);
    counter_methods::register(&mut registry);

    println!("Registered {} services:", registry.service_count());
    for service in registry.services() {
        println!(
            "  - {} ({} methods)",
            service.name,
            service.methods.len()
        );
    }

    let state = Arc::new(AppState {
        registry,
        calculator: CalculatorImpl,
        greeter: GreeterImpl,
        counter: CounterImpl,
    });

    // Build the router
    let app = Router::new()
        .route("/", get(dashboard_ui))
        .route("/api/services", get(list_services))
        .route("/api/services/{id}", get(get_service))
        .route("/api/stream", get(stream_method))
        .route("/api/call", post(call_method))
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .with_state(state);

    let addr = "127.0.0.1:3000";
    println!("\nDashboard running at http://{}", addr);
    println!("API endpoints:");
    println!("  GET  /api/services");
    println!("  GET  /api/services/{{id}}");
    println!("  POST /api/call");
    println!("  GET  /api/stream (SSE)");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
