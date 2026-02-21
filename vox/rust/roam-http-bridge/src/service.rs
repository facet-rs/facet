//! Generic bridge service implementation.
//!
//! This module provides `GenericBridgeService`, which wraps a roam `ConnectionHandle`
//! and `ServiceDescriptor` to implement `BridgeService` using runtime transcoding.

use std::collections::HashMap;

use facet_core::Shape;
use roam_schema::contains_channels;
use roam_session::{ConnectionHandle, MethodDescriptor, ServiceDescriptor};

use crate::{
    BoxFuture, BridgeError, BridgeMetadata, BridgeResponse, BridgeService, ProtocolErrorKind,
    transcode::{json_args_to_postcard, postcard_to_json_with_shape},
};

/// A generic bridge service that wraps a roam connection.
///
/// This uses runtime transcoding via `facet_value::Value` - no per-service
/// code generation required.
pub struct GenericBridgeService {
    /// The roam connection handle for making calls.
    handle: ConnectionHandle,
    /// Service descriptor (name, methods, types).
    descriptor: &'static ServiceDescriptor,
    /// Precomputed method info for fast lookup.
    methods: HashMap<String, MethodInfo>,
}

/// Cached method information for fast lookup.
struct MethodInfo {
    descriptor: &'static MethodDescriptor,
    has_channels: bool,
    /// The return type shape (for decoding responses).
    /// Unwrapped from Result<T, E> if the method is fallible.
    return_shape: &'static Shape,
    /// The error type shape (for decoding user errors), if any.
    /// For methods returning Result<T, E>, this is E's shape.
    error_shape: Option<&'static Shape>,
}

impl GenericBridgeService {
    /// Create a new bridge service wrapping a connection.
    ///
    /// # Arguments
    /// * `handle` - The roam connection handle for making RPC calls
    /// * `descriptor` - Static service descriptor (from generated code)
    pub fn new(handle: ConnectionHandle, descriptor: &'static ServiceDescriptor) -> Self {
        let mut methods = HashMap::new();

        for &desc in descriptor.methods {
            let has_channels = desc.arg_shapes.iter().any(|s| contains_channels(s))
                || contains_channels(desc.return_shape);

            let (return_shape, error_shape) = extract_result_types(desc.return_shape);

            methods.insert(
                desc.method_name.to_string(),
                MethodInfo {
                    descriptor: desc,
                    has_channels,
                    return_shape,
                    error_shape,
                },
            );
        }

        Self {
            handle,
            descriptor,
            methods,
        }
    }
}

/// Extract the success and error types from a method's return type.
///
/// After Issue #19, methods have natural return types:
/// - Infallible methods return `T` directly
/// - Fallible methods return `Result<T, E>` where E is the user error type
///
/// The wire protocol always wraps in Result with protocol errors,
/// but the schema reflects the natural signature.
fn extract_result_types(return_shape: &'static Shape) -> (&'static Shape, Option<&'static Shape>) {
    // Check if the return type is Result<T, E>
    if let facet_core::Def::Result(result_def) = return_shape.def {
        let success_shape = result_def.t();
        let error_shape = result_def.e();
        return (success_shape, Some(error_shape));
    }

    // Infallible method: return type is T directly, no user error possible
    (return_shape, None)
}

impl BridgeService for GenericBridgeService {
    fn service_descriptor(&self) -> &'static ServiceDescriptor {
        self.descriptor
    }

    fn connection_handle(&self) -> &ConnectionHandle {
        &self.handle
    }

    fn call_json<'a>(
        &'a self,
        method_name: &'a str,
        json_body: &'a [u8],
        metadata: BridgeMetadata,
    ) -> BoxFuture<'a, Result<BridgeResponse, BridgeError>> {
        Box::pin(async move {
            // Look up method
            let method_info = self.methods.get(method_name).ok_or_else(|| {
                // r[bridge.response.protocol-error]
                BridgeError::new(
                    http::StatusCode::OK,
                    format!("Unknown method: {}", method_name),
                )
            })?;

            // r[bridge.json.channels-forbidden]
            if method_info.has_channels {
                return Err(BridgeError::bad_request(
                    "Channel methods require WebSocket",
                ));
            }

            // r[bridge.json.facet]
            // Transcode JSON array → postcard tuple using arg shapes
            let postcard_payload =
                json_args_to_postcard(json_body, method_info.descriptor.arg_shapes)?;

            // Convert metadata to wire format
            let wire_metadata = metadata.to_wire_metadata();

            // Make the roam call
            let response_bytes = self
                .handle
                .call_raw_with_metadata(method_info.descriptor, postcard_payload, wire_metadata)
                .await
                .map_err(|e| BridgeError::backend_unavailable(format!("Call failed: {e}")))?;

            // Parse the response envelope
            // The response is wrapped: 0x00 + value_bytes for Ok, 0x01 + error_bytes for Err
            // Error variants: User(0) + E, UnknownMethod(1), InvalidPayload(2), Cancelled(3)
            if response_bytes.is_empty() {
                return Err(BridgeError::internal("Empty response from backend"));
            }

            match response_bytes[0] {
                0x00 => {
                    // Result::Ok(value) - transcode the value part using the return shape
                    let value_bytes = &response_bytes[1..];
                    let json_bytes =
                        postcard_to_json_with_shape(value_bytes, method_info.return_shape)?;
                    Ok(BridgeResponse::Success(json_bytes))
                }
                0x01 => {
                    // Result::Err - decode which error variant
                    if response_bytes.len() < 2 {
                        return Err(BridgeError::internal("Truncated error response"));
                    }
                    match response_bytes[1] {
                        0x00 => {
                            // User error - transcode the error value
                            let error_bytes = &response_bytes[2..];
                            // Use error shape if available, otherwise return raw
                            if let Some(error_shape) = method_info.error_shape {
                                let json_bytes =
                                    postcard_to_json_with_shape(error_bytes, error_shape)?;
                                Ok(BridgeResponse::UserError(json_bytes))
                            } else {
                                // No error type (Never) - shouldn't have user errors
                                Ok(BridgeResponse::UserError(b"null".to_vec()))
                            }
                        }
                        0x01 => {
                            // UnknownMethod
                            Ok(BridgeResponse::ProtocolError(
                                ProtocolErrorKind::UnknownMethod,
                            ))
                        }
                        0x02 => {
                            // InvalidPayload
                            Ok(BridgeResponse::ProtocolError(
                                ProtocolErrorKind::InvalidPayload,
                            ))
                        }
                        0x03 => {
                            // Cancelled
                            Ok(BridgeResponse::ProtocolError(ProtocolErrorKind::Cancelled))
                        }
                        tag => Err(BridgeError::internal(format!(
                            "Unknown error variant: {tag}"
                        ))),
                    }
                }
                tag => Err(BridgeError::internal(format!(
                    "Unknown Result variant: {tag}"
                ))),
            }
        })
    }
}
