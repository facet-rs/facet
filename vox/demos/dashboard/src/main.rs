//! Rapace Dashboard Server
//!
//! A dashboard that dogfoods rapace: the frontend connects via WebSocket using
//! rapace-wasm-client, and all RPC calls go through the ExplorerService.
//!
//! ## Architecture
//!
//! ```text
//! Browser (wasm) ──WebSocket──> ExplorerService ──> Calculator/Greeter/Counter
//! ```
//!
//! The ExplorerService acts as a proxy/explorer over all registered services,
//! providing:
//! - Service discovery (list_services, get_service)
//! - Dynamic method invocation (call_unary, call_streaming)
//!
//! ## Endpoints
//!
//! - `GET /` - Serve the dashboard UI
//! - `ws://localhost:4269` - WebSocket endpoint for rapace RPC

use std::sync::Arc;

use axum::{response::Html, routing::get, Router};
use owo_colors::OwoColorize;
use rapace::facet_core::{Def, ScalarType, Shape, Type, UserType};
use rapace::registry::{ServiceId, ServiceRegistry};
use rapace::{ErrorCode, RpcError, Streaming, WebSocketTransport};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

// ============================================================================
// ExplorerService types (using Facet for serialization)
// ============================================================================

/// Summary of a service for the list view.
#[derive(Clone, Debug, facet::Facet)]
pub struct ServiceSummary {
    pub id: u32,
    pub name: String,
    pub doc: String,
    pub method_count: u32,
}

/// Serializable representation of a facet shape for form generation.
#[derive(Clone, Debug, facet::Facet)]
#[repr(u8)]
pub enum ShapeInfo {
    /// Scalar types (integers, floats, booleans)
    Scalar {
        type_name: String,
        /// Hint: "integer", "unsigned", "float", "boolean"
        affinity: String,
    },
    /// String type
    String,
    /// Optional type wrapping another shape
    Option {
        #[facet(recursive_type)]
        inner: Box<ShapeInfo>,
    },
    /// List/Vec type
    List {
        #[facet(recursive_type)]
        item: Box<ShapeInfo>,
    },
    /// Struct with named fields
    Struct {
        type_name: String,
        fields: Vec<FieldInfo>,
    },
    /// Enum with variants
    Enum {
        type_name: String,
        variants: Vec<VariantInfo>,
    },
    /// Map type (HashMap, BTreeMap)
    Map {
        #[facet(recursive_type)]
        key: Box<ShapeInfo>,
        #[facet(recursive_type)]
        value: Box<ShapeInfo>,
    },
    /// Fallback for types we can't represent
    Unknown { type_name: String },
}

/// Field in a struct shape
#[derive(Clone, Debug, facet::Facet)]
pub struct FieldInfo {
    pub name: String,
    #[facet(recursive_type)]
    pub shape: ShapeInfo,
}

/// Variant in an enum shape
#[derive(Clone, Debug, facet::Facet)]
pub struct VariantInfo {
    pub name: String,
    /// None for unit variants, Some for tuple/struct variants
    pub fields: Option<Vec<FieldInfo>>,
}

/// Convert a facet Shape into our serializable ShapeInfo
fn shape_to_info(shape: &'static Shape) -> ShapeInfo {
    // Check for scalar types first using ScalarType enum
    if let Some(scalar) = shape.scalar_type() {
        return match scalar {
            ScalarType::String | ScalarType::Str | ScalarType::CowStr => ShapeInfo::String,
            ScalarType::Bool => ShapeInfo::Scalar {
                type_name: shape.type_identifier.to_string(),
                affinity: "boolean".to_string(),
            },
            ScalarType::Char => ShapeInfo::Scalar {
                type_name: shape.type_identifier.to_string(),
                affinity: "char".to_string(),
            },
            ScalarType::F32 | ScalarType::F64 => ShapeInfo::Scalar {
                type_name: shape.type_identifier.to_string(),
                affinity: "float".to_string(),
            },
            ScalarType::I8
            | ScalarType::I16
            | ScalarType::I32
            | ScalarType::I64
            | ScalarType::I128
            | ScalarType::ISize => ShapeInfo::Scalar {
                type_name: shape.type_identifier.to_string(),
                affinity: "integer".to_string(),
            },
            ScalarType::U8
            | ScalarType::U16
            | ScalarType::U32
            | ScalarType::U64
            | ScalarType::U128
            | ScalarType::USize => ShapeInfo::Scalar {
                type_name: shape.type_identifier.to_string(),
                affinity: "unsigned".to_string(),
            },
            _ => ShapeInfo::Scalar {
                type_name: shape.type_identifier.to_string(),
                affinity: "unknown".to_string(),
            },
        };
    }

    // Check via Def for container types
    match &shape.def {
        Def::Option(option_def) => {
            return ShapeInfo::Option {
                inner: Box::new(shape_to_info(option_def.t())),
            };
        }
        Def::List(list_def) => {
            return ShapeInfo::List {
                item: Box::new(shape_to_info(list_def.t())),
            };
        }
        Def::Map(map_def) => {
            return ShapeInfo::Map {
                key: Box::new(shape_to_info(map_def.k())),
                value: Box::new(shape_to_info(map_def.v())),
            };
        }
        _ => {}
    }

    // Use `ty` for structs, enums
    match &shape.ty {
        Type::User(UserType::Struct(struct_type)) => {
            let fields = struct_type
                .fields
                .iter()
                .map(|field| FieldInfo {
                    name: field.name.to_string(),
                    shape: shape_to_info(field.shape()),
                })
                .collect();
            ShapeInfo::Struct {
                type_name: shape.type_identifier.to_string(),
                fields,
            }
        }
        Type::User(UserType::Enum(enum_type)) => {
            let variants = enum_type
                .variants
                .iter()
                .map(|variant| {
                    let fields = if variant.data.fields.is_empty() {
                        None
                    } else {
                        Some(
                            variant
                                .data
                                .fields
                                .iter()
                                .map(|field| FieldInfo {
                                    name: field.name.to_string(),
                                    shape: shape_to_info(field.shape()),
                                })
                                .collect(),
                        )
                    };
                    VariantInfo {
                        name: variant.name.to_string(),
                        fields,
                    }
                })
                .collect();
            ShapeInfo::Enum {
                type_name: shape.type_identifier.to_string(),
                variants,
            }
        }
        _ => {
            // Fallback for unhandled types
            match &shape.def {
                Def::Scalar => ShapeInfo::Scalar {
                    type_name: shape.type_identifier.to_string(),
                    affinity: "unknown".to_string(),
                },
                _ => ShapeInfo::Unknown {
                    type_name: shape.type_identifier.to_string(),
                },
            }
        }
    }
}

/// Details of an argument to a method.
#[derive(Clone, Debug, facet::Facet)]
pub struct ArgDetail {
    pub name: String,
    pub type_name: String,
    /// Shape information for generating typed form inputs
    pub shape: ShapeInfo,
}

/// Details of a method.
#[derive(Clone, Debug, facet::Facet)]
pub struct MethodDetail {
    pub id: u32,
    pub name: String,
    pub full_name: String,
    pub doc: String,
    pub args: Vec<ArgDetail>,
    pub is_streaming: bool,
    pub request_type: String,
    pub response_type: String,
}

/// Full details of a service including methods.
#[derive(Clone, Debug, facet::Facet)]
pub struct ServiceDetail {
    pub id: u32,
    pub name: String,
    pub doc: String,
    pub methods: Vec<MethodDetail>,
}

/// Request to call a method dynamically.
#[derive(Clone, Debug, facet::Facet)]
pub struct CallRequest {
    pub service: String,
    pub method: String,
    /// JSON-encoded arguments
    pub args_json: String,
}

/// Response from calling a method.
#[derive(Clone, Debug, facet::Facet)]
pub struct CallResponse {
    /// JSON-encoded result (or null on error)
    pub result_json: String,
    /// Error message if call failed
    pub error: Option<String>,
}

/// A single item from a streaming response.
#[derive(Clone, Debug, facet::Facet)]
pub struct StreamItem {
    /// JSON-encoded value
    pub value_json: String,
}

// ============================================================================
// ExplorerService trait
// ============================================================================

/// The Explorer service provides service discovery and dynamic method invocation.
///
/// This is the only service the frontend needs to know about - it acts as a
/// proxy to all other registered services.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Explorer {
    /// List all registered services.
    async fn list_services(&self) -> Vec<crate::ServiceSummary>;

    /// Get details for a specific service by ID.
    async fn get_service(&self, service_id: u32) -> Option<crate::ServiceDetail>;

    /// Call a unary method dynamically.
    ///
    /// Arguments are passed as a JSON string and the result is returned as JSON.
    async fn call_unary(&self, request: crate::CallRequest) -> crate::CallResponse;

    /// Call a streaming method dynamically.
    ///
    /// Arguments are passed as a JSON string and results are streamed as JSON.
    async fn call_streaming(&self, request: crate::CallRequest) -> Streaming<crate::StreamItem>;
}

// ============================================================================
// Demo services (Calculator, Greeter, Counter)
// ============================================================================

/// A calculator service for demonstration.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Calculator {
    /// Add two numbers together.
    async fn add(&self, a: i32, b: i32) -> i32;

    /// Multiply two numbers.
    async fn multiply(&self, a: i32, b: i32) -> i32;

    /// Compute the factorial of a number.
    async fn factorial(&self, n: u32) -> u64;
}

struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    async fn multiply(&self, a: i32, b: i32) -> i32 {
        a * b
    }

    async fn factorial(&self, n: u32) -> u64 {
        // Cap at 20 to avoid overflow (20! fits in u64, 21! doesn't)
        let n = n.min(20) as u64;
        (1..=n).product()
    }
}

/// A greeting service for demonstration.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Greeter {
    /// Generate a simple greeting.
    async fn greet(&self, name: String) -> String;

    /// Generate a formal greeting.
    async fn greet_formal(&self, title: String, name: String) -> String;
}

struct GreeterImpl;

impl Greeter for GreeterImpl {
    async fn greet(&self, name: String) -> String {
        format!("Hello, {}!", name)
    }

    async fn greet_formal(&self, title: String, name: String) -> String {
        format!("Good day, {} {}. How may I assist you?", title, name)
    }
}

/// A counter service that demonstrates streaming.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Counter {
    /// Count from 0 to n-1.
    async fn count_to(&self, n: u32) -> Streaming<u32>;

    /// Generate Fibonacci numbers.
    async fn fibonacci(&self, n: u32) -> Streaming<u64>;
}

struct CounterImpl;

impl Counter for CounterImpl {
    async fn count_to(&self, n: u32) -> Streaming<u32> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            for i in 0..n {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if tx.send(Ok(i)).await.is_err() {
                    break;
                }
            }
        });
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }

    async fn fibonacci(&self, n: u32) -> Streaming<u64> {
        // Cap at 93 - fib(93) is the largest that fits in u64
        let n = n.min(93);
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            let mut a: u64 = 0;
            let mut b: u64 = 1;
            for _ in 0..n {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if tx.send(Ok(a)).await.is_err() {
                    break;
                }
                let next = a.wrapping_add(b);
                a = b;
                b = next;
            }
        });
        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

// ============================================================================
// ExplorerService implementation
// ============================================================================

struct ExplorerImpl {
    registry: Arc<ServiceRegistry>,
    calculator: CalculatorImpl,
    greeter: GreeterImpl,
    counter: CounterImpl,
}

impl Explorer for ExplorerImpl {
    async fn list_services(&self) -> Vec<crate::ServiceSummary> {
        self.registry
            .services()
            .filter(|s| s.name != "Explorer") // Don't list ourselves
            .map(|service| ServiceSummary {
                id: service.id.0,
                name: service.name.to_string(),
                doc: service.doc.clone(),
                method_count: service.methods.len() as u32,
            })
            .collect()
    }

    async fn get_service(&self, service_id: u32) -> Option<crate::ServiceDetail> {
        let service = self.registry.service_by_id(ServiceId(service_id))?;

        // Don't expose Explorer service details
        if service.name == "Explorer" {
            return None;
        }

        let methods = service
            .methods
            .values()
            .map(|method| MethodDetail {
                id: method.id.0,
                name: method.name.to_string(),
                full_name: method.full_name.clone(),
                doc: method.doc.clone(),
                args: {
                    // Get field shapes from the request struct shape
                    // Note: For single-arg primitive methods, the request_shape is the primitive itself,
                    // not a struct wrapper
                    let field_shapes: Vec<(&str, &'static Shape)> =
                        if let Type::User(UserType::Struct(struct_type)) = &method.request_shape.ty
                        {
                            struct_type
                                .fields
                                .iter()
                                .map(|f| (f.name, f.shape()))
                                .collect()
                        } else {
                            // Single-arg method with primitive type - use the request shape directly
                            if method.args.len() == 1 {
                                vec![(method.args[0].name, method.request_shape)]
                            } else {
                                Vec::new()
                            }
                        };

                    // Match args with field shapes by position (they should be in the same order)
                    method
                        .args
                        .iter()
                        .enumerate()
                        .map(|(i, arg)| {
                            // Try to find by name first, then by position
                            let shape = field_shapes
                                .iter()
                                .find(|(name, _)| *name == arg.name)
                                .or_else(|| field_shapes.get(i))
                                .map(|(_, s)| shape_to_info(s))
                                .unwrap_or_else(|| ShapeInfo::Unknown {
                                    type_name: arg.type_name.to_string(),
                                });
                            ArgDetail {
                                name: arg.name.to_string(),
                                type_name: arg.type_name.to_string(),
                                shape,
                            }
                        })
                        .collect()
                },
                is_streaming: method.is_streaming,
                request_type: format!("{}", method.request_shape),
                response_type: format!("{}", method.response_shape),
            })
            .collect();

        Some(ServiceDetail {
            id: service.id.0,
            name: service.name.to_string(),
            doc: service.doc.clone(),
            methods,
        })
    }

    async fn call_unary(&self, request: crate::CallRequest) -> crate::CallResponse {
        // Parse the JSON args
        let args: serde_json::Value = match serde_json::from_str(&request.args_json) {
            Ok(v) => v,
            Err(e) => {
                return CallResponse {
                    result_json: "null".to_string(),
                    error: Some(format!("Invalid JSON args: {}", e)),
                }
            }
        };

        // Dispatch to the appropriate service
        let result = match request.service.as_str() {
            "Calculator" => self.dispatch_calculator(&request.method, &args).await,
            "Greeter" => self.dispatch_greeter(&request.method, &args).await,
            "Counter" => {
                return CallResponse {
                    result_json: "null".to_string(),
                    error: Some("Counter methods are streaming-only. Use call_streaming.".into()),
                }
            }
            _ => Err(format!("Unknown service: {}", request.service)),
        };

        match result {
            Ok(value) => CallResponse {
                result_json: serde_json::to_string(&value).unwrap_or("null".to_string()),
                error: None,
            },
            Err(e) => CallResponse {
                result_json: "null".to_string(),
                error: Some(e),
            },
        }
    }

    async fn call_streaming(&self, request: crate::CallRequest) -> Streaming<crate::StreamItem> {
        // Parse the JSON args
        let args: serde_json::Value = match serde_json::from_str(&request.args_json) {
            Ok(v) => v,
            Err(e) => {
                let (tx, rx) = tokio::sync::mpsc::channel(1);
                let _ = tx
                    .send(Err(RpcError::Status {
                        code: ErrorCode::InvalidArgument,
                        message: format!("Invalid JSON args: {}", e),
                    }))
                    .await;
                return Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx));
            }
        };

        // Only Counter has streaming methods
        if request.service != "Counter" {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx
                .send(Err(RpcError::Status {
                    code: ErrorCode::InvalidArgument,
                    message: format!("Service {} has no streaming methods", request.service),
                }))
                .await;
            return Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx));
        }

        let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as u32;

        let (tx, rx) = tokio::sync::mpsc::channel(16);

        match request.method.as_str() {
            "count_to" => {
                let mut stream = self.counter.count_to(n).await;
                tokio::spawn(async move {
                    use tokio_stream::StreamExt;
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(value) => {
                                let item = StreamItem {
                                    value_json: serde_json::to_string(&value)
                                        .unwrap_or("null".to_string()),
                                };
                                if tx.send(Ok(item)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                });
            }
            "fibonacci" => {
                let mut stream = self.counter.fibonacci(n).await;
                tokio::spawn(async move {
                    use tokio_stream::StreamExt;
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(value) => {
                                let item = StreamItem {
                                    value_json: serde_json::to_string(&value)
                                        .unwrap_or("null".to_string()),
                                };
                                if tx.send(Ok(item)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                });
            }
            _ => {
                let method = request.method.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(Err(RpcError::Status {
                            code: ErrorCode::Unimplemented,
                            message: format!("Unknown Counter method: {}", method),
                        }))
                        .await;
                });
            }
        }

        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}

impl ExplorerImpl {
    async fn dispatch_calculator(
        &self,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match method {
            "add" => {
                let a = args
                    .get("a")
                    .and_then(|v| v.as_i64())
                    .ok_or("Missing argument 'a'")? as i32;
                let b = args
                    .get("b")
                    .and_then(|v| v.as_i64())
                    .ok_or("Missing argument 'b'")? as i32;
                let result = self.calculator.add(a, b).await;
                Ok(serde_json::json!(result))
            }
            "multiply" => {
                let a = args
                    .get("a")
                    .and_then(|v| v.as_i64())
                    .ok_or("Missing argument 'a'")? as i32;
                let b = args
                    .get("b")
                    .and_then(|v| v.as_i64())
                    .ok_or("Missing argument 'b'")? as i32;
                let result = self.calculator.multiply(a, b).await;
                Ok(serde_json::json!(result))
            }
            "factorial" => {
                let n = args
                    .get("n")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing argument 'n'")? as u32;
                let result = self.calculator.factorial(n).await;
                Ok(serde_json::json!(result))
            }
            _ => Err(format!("Unknown Calculator method: {}", method)),
        }
    }

    async fn dispatch_greeter(
        &self,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match method {
            "greet" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing argument 'name'")?
                    .to_string();
                let result = self.greeter.greet(name).await;
                Ok(serde_json::json!(result))
            }
            "greet_formal" => {
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing argument 'title'")?
                    .to_string();
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing argument 'name'")?
                    .to_string();
                let result = self.greeter.greet_formal(title, name).await;
                Ok(serde_json::json!(result))
            }
            _ => Err(format!("Unknown Greeter method: {}", method)),
        }
    }
}

// ============================================================================
// WebSocket server for rapace RPC
// ============================================================================

const WS_PORT: u16 = 4268;
const HTTP_PORT: u16 = 4269;

async fn run_websocket_server(explorer: Arc<ExplorerImpl>) {
    let addr = format!("127.0.0.1:{}", WS_PORT);
    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind WebSocket server");

    while let Ok((stream, addr)) = listener.accept().await {
        let explorer = Arc::clone(&explorer);
        tokio::spawn(async move {
            println!("New WebSocket connection from {}", addr);

            let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    eprintln!("WebSocket handshake failed for {}: {}", addr, e);
                    return;
                }
            };

            let transport = Arc::new(WebSocketTransport::new(ws_stream));
            let server = ExplorerServer::new(explorer.as_ref().clone());

            if let Err(e) = server.serve(transport).await {
                eprintln!("Connection from {} ended with error: {:?}", addr, e);
            } else {
                println!("Connection from {} closed", addr);
            }
        });
    }
}

// We need Clone for ExplorerImpl to use with the server
impl Clone for ExplorerImpl {
    fn clone(&self) -> Self {
        Self {
            registry: Arc::clone(&self.registry),
            calculator: CalculatorImpl,
            greeter: GreeterImpl,
            counter: CounterImpl,
        }
    }
}

// ============================================================================
// HTTP server for static assets
// ============================================================================

async fn dashboard_ui() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    // Create and populate the registry
    let mut registry = ServiceRegistry::new();

    // Register demo services
    calculator_register(&mut registry);
    greeter_register(&mut registry);
    counter_register(&mut registry);

    // Register the Explorer service itself (for completeness, though we filter it out)
    explorer_register(&mut registry);

    let registry = Arc::new(registry);

    // Create the explorer service
    let explorer = Arc::new(ExplorerImpl {
        registry: Arc::clone(&registry),
        calculator: CalculatorImpl,
        greeter: GreeterImpl,
        counter: CounterImpl,
    });

    // Start WebSocket server in background
    let ws_explorer = Arc::clone(&explorer);
    tokio::spawn(async move {
        run_websocket_server(ws_explorer).await;
    });

    // Build the HTTP router - serves static files and wasm pkg
    let app = Router::new()
        .route("/", get(dashboard_ui))
        .nest_service("/pkg", ServeDir::new("demos/dashboard/static/pkg"))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let http_addr = format!("127.0.0.1:{}", HTTP_PORT);
    let http_url = format!("http://{}", http_addr);

    // Print a nice banner
    println!();
    println!(
        "  {} {} {}",
        "Rapace".bold().cyan(),
        "Dashboard".white(),
        "ready".green()
    );
    println!();
    println!(
        "  {}  \x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\",
        "Open:".dimmed(),
        http_url,
        http_url.bold().underline().cyan()
    );
    println!();
    println!("  {}  Ctrl+C", "Stop:".dimmed());
    println!();

    let listener = TcpListener::bind(&http_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
