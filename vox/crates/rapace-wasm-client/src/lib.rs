//! rapace-wasm-client: WebAssembly client for rapace RPC.
//!
//! This crate provides a browser-compatible WebSocket client for the rapace
//! ExplorerService, which allows dynamic service discovery and method invocation.
//!
//! # Usage
//!
//! ```javascript
//! import init, { ExplorerClient } from './rapace_wasm_client.js';
//!
//! await init();
//!
//! const client = await ExplorerClient.connect('ws://localhost:9001');
//!
//! // List all services
//! const services = await client.listServices();
//! console.log('Services:', services);
//!
//! // Get service details
//! const service = await client.getService(0);
//! console.log('Service 0:', service);
//!
//! // Call a unary method dynamically
//! const result = await client.callUnary('Calculator', 'add', { a: 5, b: 3 });
//! console.log('Result:', JSON.parse(result.result_json));
//!
//! // Call a streaming method
//! const stream = await client.callStreaming('Counter', 'count_to', { n: 5 });
//! for await (const item of stream) {
//!     console.log('Got:', JSON.parse(item.value_json));
//! }
//!
//! client.close();
//! ```

use std::sync::Arc;

use rapace_core::{Frame, FrameFlags, MsgDescHot, Transport, TransportError, INLINE_PAYLOAD_SIZE};
use rapace_transport_websocket::WebSocketTransport;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Compute a method ID by hashing "ServiceName.method_name" using FNV-1a.
/// Must match the implementation in rapace-macros.
const fn compute_method_id(service_name: &str, method_name: &str) -> u32 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;

    let service_bytes = service_name.as_bytes();
    let mut i = 0;
    while i < service_bytes.len() {
        hash ^= service_bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash ^= b'.' as u64;
    hash = hash.wrapping_mul(FNV_PRIME);
    let method_bytes = method_name.as_bytes();
    i = 0;
    while i < method_bytes.len() {
        hash ^= method_bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }

    ((hash >> 32) ^ hash) as u32
}

// ExplorerService method IDs (computed via FNV-1a hash to match rapace-macros)
const METHOD_LIST_SERVICES: u32 = compute_method_id("Explorer", "list_services");
const METHOD_GET_SERVICE: u32 = compute_method_id("Explorer", "get_service");
const METHOD_CALL_UNARY: u32 = compute_method_id("Explorer", "call_unary");
const METHOD_CALL_STREAMING: u32 = compute_method_id("Explorer", "call_streaming");

// ============================================================================
// ExplorerService types (must match dashboard types exactly)
// ============================================================================

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

#[derive(Clone, Debug, facet::Facet)]
pub struct ArgDetail {
    pub name: String,
    pub type_name: String,
    /// Shape information for generating typed form inputs
    pub shape: ShapeInfo,
}

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

#[derive(Clone, Debug, facet::Facet)]
pub struct ServiceDetail {
    pub id: u32,
    pub name: String,
    pub doc: String,
    pub methods: Vec<MethodDetail>,
}

#[derive(Clone, Debug, facet::Facet)]
pub struct CallRequest {
    pub service: String,
    pub method: String,
    pub args_json: String,
}

#[derive(Clone, Debug, facet::Facet)]
pub struct CallResponse {
    pub result_json: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, facet::Facet)]
pub struct StreamItem {
    pub value_json: String,
}

// ============================================================================
// ExplorerClient - the main client for the dashboard
// ============================================================================

/// A rapace Explorer client for use in WebAssembly.
///
/// Connects to the ExplorerService and provides methods for:
/// - Service discovery (list_services, get_service)
/// - Dynamic unary method invocation (call_unary)
/// - Dynamic streaming method invocation (call_streaming)
#[wasm_bindgen]
pub struct ExplorerClient {
    transport: Arc<WebSocketTransport>,
    next_msg_id: u64,
    next_channel_id: u32,
}

#[wasm_bindgen]
impl ExplorerClient {
    /// Connect to a rapace ExplorerService WebSocket server.
    ///
    /// Returns a Promise that resolves to an ExplorerClient.
    #[wasm_bindgen]
    pub async fn connect(url: &str) -> Result<ExplorerClient, JsValue> {
        let transport = WebSocketTransport::connect(url)
            .await
            .map_err(transport_error)?;
        Ok(ExplorerClient {
            transport: Arc::new(transport),
            next_msg_id: 1,
            next_channel_id: 1,
        })
    }

    /// List all registered services.
    ///
    /// Returns a JSON string containing an array of ServiceSummary objects.
    #[wasm_bindgen(js_name = listServices)]
    pub async fn list_services(&mut self) -> Result<JsValue, JsValue> {
        // Request has no parameters for list_services
        #[derive(facet::Facet)]
        struct ListServicesRequest;

        let request = ListServicesRequest;
        let payload = facet_postcard::to_vec(&request)
            .map_err(|e| JsValue::from_str(&format!("encode error: {}", e)))?;

        let response_frame = self.call_method(METHOD_LIST_SERVICES, &payload).await?;

        // Decode response
        let services: Vec<ServiceSummary> = facet_postcard::from_slice(response_frame.payload())
            .map_err(|e| JsValue::from_str(&format!("decode error: {}", e)))?;

        // Convert to JS array
        let result = js_sys::Array::new();
        for service in services {
            let obj = js_sys::Object::new();
            js_sys::Reflect::set(&obj, &"id".into(), &JsValue::from(service.id))?;
            js_sys::Reflect::set(&obj, &"name".into(), &JsValue::from_str(&service.name))?;
            js_sys::Reflect::set(&obj, &"doc".into(), &JsValue::from_str(&service.doc))?;
            js_sys::Reflect::set(
                &obj,
                &"method_count".into(),
                &JsValue::from(service.method_count),
            )?;
            result.push(&obj);
        }
        Ok(result.into())
    }

    /// Get details for a specific service by ID.
    ///
    /// Returns a JSON object with service details, or null if not found.
    #[wasm_bindgen(js_name = getService)]
    pub async fn get_service(&mut self, service_id: u32) -> Result<JsValue, JsValue> {
        #[derive(facet::Facet)]
        struct GetServiceRequest {
            service_id: u32,
        }

        let request = GetServiceRequest { service_id };
        let payload = facet_postcard::to_vec(&request)
            .map_err(|e| JsValue::from_str(&format!("encode error: {}", e)))?;

        let response_frame = self.call_method(METHOD_GET_SERVICE, &payload).await?;

        // Decode response
        let service: Option<ServiceDetail> =
            facet_postcard::from_slice(response_frame.payload())
                .map_err(|e| JsValue::from_str(&format!("decode error: {}", e)))?;

        match service {
            Some(s) => {
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &"id".into(), &JsValue::from(s.id))?;
                js_sys::Reflect::set(&obj, &"name".into(), &JsValue::from_str(&s.name))?;
                js_sys::Reflect::set(&obj, &"doc".into(), &JsValue::from_str(&s.doc))?;

                let methods = js_sys::Array::new();
                for m in s.methods {
                    let method_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&method_obj, &"id".into(), &JsValue::from(m.id))?;
                    js_sys::Reflect::set(&method_obj, &"name".into(), &JsValue::from_str(&m.name))?;
                    js_sys::Reflect::set(
                        &method_obj,
                        &"full_name".into(),
                        &JsValue::from_str(&m.full_name),
                    )?;
                    js_sys::Reflect::set(&method_obj, &"doc".into(), &JsValue::from_str(&m.doc))?;
                    js_sys::Reflect::set(
                        &method_obj,
                        &"is_streaming".into(),
                        &JsValue::from(m.is_streaming),
                    )?;
                    js_sys::Reflect::set(
                        &method_obj,
                        &"request_type".into(),
                        &JsValue::from_str(&m.request_type),
                    )?;
                    js_sys::Reflect::set(
                        &method_obj,
                        &"response_type".into(),
                        &JsValue::from_str(&m.response_type),
                    )?;

                    let args = js_sys::Array::new();
                    for arg in m.args {
                        let arg_obj = js_sys::Object::new();
                        js_sys::Reflect::set(
                            &arg_obj,
                            &"name".into(),
                            &JsValue::from_str(&arg.name),
                        )?;
                        js_sys::Reflect::set(
                            &arg_obj,
                            &"type_name".into(),
                            &JsValue::from_str(&arg.type_name),
                        )?;
                        js_sys::Reflect::set(&arg_obj, &"shape".into(), &shape_to_js(&arg.shape)?)?;
                        args.push(&arg_obj);
                    }
                    js_sys::Reflect::set(&method_obj, &"args".into(), &args)?;

                    methods.push(&method_obj);
                }
                js_sys::Reflect::set(&obj, &"methods".into(), &methods)?;

                Ok(obj.into())
            }
            None => Ok(JsValue::NULL),
        }
    }

    /// Call a unary method dynamically.
    ///
    /// Arguments:
    /// - service: The service name (e.g., "Calculator")
    /// - method: The method name (e.g., "add")
    /// - args: A JavaScript object with the method arguments
    ///
    /// Returns a CallResponse with result_json containing the JSON-encoded result.
    #[wasm_bindgen(js_name = callUnary)]
    pub async fn call_unary(
        &mut self,
        service: &str,
        method: &str,
        args: JsValue,
    ) -> Result<JsValue, JsValue> {
        let args_json = js_sys::JSON::stringify(&args)
            .map_err(|_| JsValue::from_str("Failed to stringify args"))?
            .as_string()
            .unwrap_or_else(|| "{}".to_string());

        let request = CallRequest {
            service: service.to_string(),
            method: method.to_string(),
            args_json,
        };
        let payload = facet_postcard::to_vec(&request)
            .map_err(|e| JsValue::from_str(&format!("encode error: {}", e)))?;

        let response_frame = self.call_method(METHOD_CALL_UNARY, &payload).await?;

        // Decode response
        let response: CallResponse = facet_postcard::from_slice(response_frame.payload())
            .map_err(|e| JsValue::from_str(&format!("decode error: {}", e)))?;

        // Convert to JS object
        let obj = js_sys::Object::new();
        js_sys::Reflect::set(
            &obj,
            &"result_json".into(),
            &JsValue::from_str(&response.result_json),
        )?;
        match response.error {
            Some(err) => js_sys::Reflect::set(&obj, &"error".into(), &JsValue::from_str(&err))?,
            None => js_sys::Reflect::set(&obj, &"error".into(), &JsValue::NULL)?,
        };
        Ok(obj.into())
    }

    /// Call a streaming method dynamically.
    ///
    /// Arguments:
    /// - service: The service name (e.g., "Counter")
    /// - method: The method name (e.g., "count_to")
    /// - args: A JavaScript object with the method arguments
    ///
    /// Returns a StreamingCall that can be used to iterate over results.
    #[wasm_bindgen(js_name = callStreaming)]
    pub fn call_streaming(
        &mut self,
        service: &str,
        method: &str,
        args: JsValue,
    ) -> Result<StreamingCall, JsValue> {
        let args_json = js_sys::JSON::stringify(&args)
            .map_err(|_| JsValue::from_str("Failed to stringify args"))?
            .as_string()
            .unwrap_or_else(|| "{}".to_string());

        let channel_id = self.next_channel_id;
        self.next_channel_id += 1;

        let msg_id = self.next_msg_id;
        self.next_msg_id += 1;

        Ok(StreamingCall {
            transport: Arc::clone(&self.transport),
            channel_id,
            msg_id,
            service: service.to_string(),
            method: method.to_string(),
            args_json,
            started: false,
            finished: false,
        })
    }

    /// Close the connection.
    #[wasm_bindgen]
    pub fn close(&self) {
        let transport = Arc::clone(&self.transport);
        spawn_local(async move {
            let _ = transport.close().await;
        });
    }

    // Internal helper to call a method and get the response
    async fn call_method(&mut self, method_id: u32, payload: &[u8]) -> Result<Frame, JsValue> {
        let channel_id = self.next_channel_id;
        self.next_channel_id += 1;

        let msg_id = self.next_msg_id;
        self.next_msg_id += 1;

        let mut desc = MsgDescHot::new();
        desc.msg_id = msg_id;
        desc.channel_id = channel_id;
        desc.method_id = method_id;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::with_inline_payload(desc, payload)
                .ok_or_else(|| JsValue::from_str("payload too large for inline"))?
        } else {
            Frame::with_payload(desc, payload.to_vec())
        };

        // Send request
        self.send_frame(&frame).await?;

        // Wait for response
        let response_frame = self.recv_frame().await?;

        // Check for error
        if response_frame.desc.flags.contains(FrameFlags::ERROR) {
            let error_msg = String::from_utf8_lossy(response_frame.payload()).to_string();
            return Err(JsValue::from_str(&error_msg));
        }

        Ok(response_frame)
    }

    async fn send_frame(&self, frame: &Frame) -> Result<(), JsValue> {
        self.transport
            .send_frame(frame)
            .await
            .map_err(transport_error)
    }

    async fn recv_frame(&self) -> Result<Frame, JsValue> {
        let view = self.transport.recv_frame().await.map_err(transport_error)?;
        Ok(view.to_owned())
    }
}

// ============================================================================
// StreamingCall - for handling streaming method responses
// ============================================================================

/// Async iterator for streaming RPC results.
#[wasm_bindgen]
pub struct StreamingCall {
    transport: Arc<WebSocketTransport>,
    channel_id: u32,
    msg_id: u64,
    service: String,
    method: String,
    args_json: String,
    started: bool,
    finished: bool,
}

#[wasm_bindgen]
impl StreamingCall {
    /// Get the next value from the stream.
    ///
    /// Returns a StreamItem with value_json, or null when the stream is complete.
    #[wasm_bindgen]
    pub async fn next(&mut self) -> Result<JsValue, JsValue> {
        if self.finished {
            return Ok(JsValue::NULL);
        }

        // Send initial request if not started
        if !self.started {
            self.started = true;

            let request = CallRequest {
                service: self.service.clone(),
                method: self.method.clone(),
                args_json: self.args_json.clone(),
            };
            let payload = facet_postcard::to_vec(&request)
                .map_err(|e| JsValue::from_str(&format!("encode error: {}", e)))?;

            let mut desc = MsgDescHot::new();
            desc.msg_id = self.msg_id;
            desc.channel_id = self.channel_id;
            desc.method_id = METHOD_CALL_STREAMING;
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
                Frame::with_inline_payload(desc, &payload)
                    .ok_or_else(|| JsValue::from_str("payload too large for inline"))?
            } else {
                Frame::with_payload(desc, payload.clone())
            };

            self.send_frame(&frame).await?;
        }

        // Receive next frame
        let frame = self.recv_frame().await?;

        // Check for error
        if frame.desc.flags.contains(FrameFlags::ERROR) {
            self.finished = true;
            let error_msg = String::from_utf8_lossy(frame.payload()).to_string();
            return Err(JsValue::from_str(&error_msg));
        }

        // Check for end of stream
        if frame.desc.flags.contains(FrameFlags::EOS) {
            self.finished = true;
            if frame.payload().is_empty() {
                return Ok(JsValue::NULL);
            }
        }

        // Decode streaming item
        let item: StreamItem = facet_postcard::from_slice(frame.payload())
            .map_err(|e| JsValue::from_str(&format!("decode error: {}", e)))?;

        let obj = js_sys::Object::new();
        js_sys::Reflect::set(
            &obj,
            &"value_json".into(),
            &JsValue::from_str(&item.value_json),
        )?;
        Ok(obj.into())
    }

    async fn send_frame(&self, frame: &Frame) -> Result<(), JsValue> {
        self.transport
            .send_frame(frame)
            .await
            .map_err(transport_error)
    }

    async fn recv_frame(&self) -> Result<Frame, JsValue> {
        let view = self.transport.recv_frame().await.map_err(transport_error)?;
        Ok(view.to_owned())
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert ShapeInfo to a JS object.
fn shape_to_js(shape: &ShapeInfo) -> Result<JsValue, JsValue> {
    let obj = js_sys::Object::new();

    match shape {
        ShapeInfo::Scalar {
            type_name,
            affinity,
        } => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("Scalar"))?;
            js_sys::Reflect::set(&obj, &"type_name".into(), &JsValue::from_str(type_name))?;
            js_sys::Reflect::set(&obj, &"affinity".into(), &JsValue::from_str(affinity))?;
        }
        ShapeInfo::String => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("String"))?;
        }
        ShapeInfo::Option { inner } => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("Option"))?;
            js_sys::Reflect::set(&obj, &"inner".into(), &shape_to_js(inner)?)?;
        }
        ShapeInfo::List { item } => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("List"))?;
            js_sys::Reflect::set(&obj, &"item".into(), &shape_to_js(item)?)?;
        }
        ShapeInfo::Struct { type_name, fields } => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("Struct"))?;
            js_sys::Reflect::set(&obj, &"type_name".into(), &JsValue::from_str(type_name))?;
            let fields_arr = js_sys::Array::new();
            for field in fields {
                let field_obj = js_sys::Object::new();
                js_sys::Reflect::set(&field_obj, &"name".into(), &JsValue::from_str(&field.name))?;
                js_sys::Reflect::set(&field_obj, &"shape".into(), &shape_to_js(&field.shape)?)?;
                fields_arr.push(&field_obj);
            }
            js_sys::Reflect::set(&obj, &"fields".into(), &fields_arr)?;
        }
        ShapeInfo::Enum {
            type_name,
            variants,
        } => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("Enum"))?;
            js_sys::Reflect::set(&obj, &"type_name".into(), &JsValue::from_str(type_name))?;
            let variants_arr = js_sys::Array::new();
            for variant in variants {
                let variant_obj = js_sys::Object::new();
                js_sys::Reflect::set(
                    &variant_obj,
                    &"name".into(),
                    &JsValue::from_str(&variant.name),
                )?;
                if let Some(fields) = &variant.fields {
                    let fields_arr = js_sys::Array::new();
                    for field in fields {
                        let field_obj = js_sys::Object::new();
                        js_sys::Reflect::set(
                            &field_obj,
                            &"name".into(),
                            &JsValue::from_str(&field.name),
                        )?;
                        js_sys::Reflect::set(
                            &field_obj,
                            &"shape".into(),
                            &shape_to_js(&field.shape)?,
                        )?;
                        fields_arr.push(&field_obj);
                    }
                    js_sys::Reflect::set(&variant_obj, &"fields".into(), &fields_arr)?;
                } else {
                    js_sys::Reflect::set(&variant_obj, &"fields".into(), &JsValue::NULL)?;
                }
                variants_arr.push(&variant_obj);
            }
            js_sys::Reflect::set(&obj, &"variants".into(), &variants_arr)?;
        }
        ShapeInfo::Map { key, value } => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("Map"))?;
            js_sys::Reflect::set(&obj, &"key".into(), &shape_to_js(key)?)?;
            js_sys::Reflect::set(&obj, &"value".into(), &shape_to_js(value)?)?;
        }
        ShapeInfo::Unknown { type_name } => {
            js_sys::Reflect::set(&obj, &"kind".into(), &JsValue::from_str("Unknown"))?;
            js_sys::Reflect::set(&obj, &"type_name".into(), &JsValue::from_str(type_name))?;
        }
    }

    Ok(obj.into())
}

fn transport_error(err: TransportError) -> JsValue {
    JsValue::from_str(&format!("transport error: {err}"))
}
