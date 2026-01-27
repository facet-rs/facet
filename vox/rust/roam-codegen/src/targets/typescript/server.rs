//! TypeScript server/handler generation.
//!
//! Generates server handler interface and method dispatch logic.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_schema::{ServiceDetail, ShapeKind, classify_shape, is_rx, is_tx};

use super::decode::{generate_decode_stmt_server, generate_decode_stmt_server_streaming};
use super::encode::generate_encode_expr;
use super::types::{is_fully_supported, ts_type_server_arg, ts_type_server_return};

/// Generate handler interface (for handling incoming calls).
///
/// r[impl channeling.caller-pov] - Handler uses Rx for args, Tx for returns.
pub fn generate_handler_interface(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();

    out.push_str(&format!("// Handler interface for {service_name}\n"));
    out.push_str(&format!("export interface {service_name}Handler {{\n"));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        // Handler args: Tx becomes Rx (receives), Rx becomes Tx (sends)
        let args = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    ts_type_server_arg(a.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        // Handler returns
        let ret_ty = ts_type_server_return(method.return_type);

        out.push_str(&format!(
            "  {method_name}({args}): Promise<{ret_ty}> | {ret_ty};\n"
        ));
    }

    out.push_str("}\n\n");
    out
}

/// Generate RPC method handlers map.
///
/// r[impl call.error.invalid-payload] - Deserialization errors return InvalidPayload.
/// Handler errors for infallible methods propagate (they indicate bugs).
/// Handler errors for fallible methods are encoded as RoamError::User(E).
pub fn generate_method_handlers(service: &ServiceDetail) -> String {
    use crate::render::hex_u64;

    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();
    let service_name_lower = service.name.to_lower_camel_case();

    out.push_str(&format!("// Method handlers for {service_name}\n"));
    out.push_str(&format!("export const {}_methodHandlers = new Map<bigint, MethodHandler<{service_name}Handler>>([\n", service_name_lower));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let id = crate::method_id(method);

        // Check if this method uses streaming
        let method_has_streaming = method.args.iter().any(|a| is_tx(a.ty) || is_rx(a.ty))
            || is_tx(method.return_type)
            || is_rx(method.return_type);

        out.push_str(&format!(
            "  [{}n, async (handler, payload) => {{\n",
            hex_u64(id)
        ));

        // Check if we can fully implement this method
        let can_decode_args = method.args.iter().all(|a| is_fully_supported(a.ty));
        let can_encode_return = is_fully_supported(method.return_type);

        if can_decode_args && can_encode_return && !method_has_streaming {
            // Non-streaming method - decode and call directly
            //
            // Deserialization is wrapped in try/catch - errors here are InvalidPayload.
            // Handler execution is NOT wrapped - for infallible methods, errors propagate.
            // For fallible methods, the handler returns { ok, value/error } which we encode.

            // Step 1: Decode arguments (InvalidPayload on error)
            out.push_str("    // Decode arguments - errors here are InvalidPayload\n");
            out.push_str("    let args;\n");
            out.push_str("    try {\n");
            out.push_str("      const buf = payload;\n");
            out.push_str("      let offset = 0;\n");
            for arg in &method.args {
                let arg_name = arg.name.to_lower_camel_case();
                let decode_stmt = generate_decode_stmt_server(arg.ty, &arg_name, "offset");
                out.push_str(&format!("      {decode_stmt}\n"));
            }
            out.push_str(
                "      if (offset !== buf.length) throw new Error(\"args: trailing bytes\");\n",
            );

            // Collect decoded args into object
            let arg_names: Vec<_> = method
                .args
                .iter()
                .map(|a| a.name.to_lower_camel_case())
                .collect();
            if arg_names.is_empty() {
                out.push_str("      args = {};\n");
            } else {
                out.push_str(&format!("      args = {{ {} }};\n", arg_names.join(", ")));
            }
            out.push_str("    } catch (_decodeError) {\n");
            out.push_str("      return encodeResultErr(encodeInvalidPayload());\n");
            out.push_str("    }\n\n");

            // Step 2: Call handler (no try/catch for infallible, encode result for fallible)
            out.push_str("    // Call handler - errors propagate for infallible methods\n");
            let call_args = arg_names
                .iter()
                .map(|n| format!("args.{n}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "    const result = await handler.{method_name}({call_args});\n"
            ));

            // Step 3: Encode response
            // Check if return type is Result<T, E>
            if let ShapeKind::Result { ok, err } = classify_shape(method.return_type) {
                // Fallible method - handler returns { ok: true; value: T } | { ok: false; error: E }
                // Wire format: [0] + T for success, [1, 0] + E for User error
                let ok_encode = generate_encode_expr(ok, "result.value");
                let err_encode = generate_encode_expr(err, "result.error");
                out.push_str("    if (result.ok) {\n");
                out.push_str(&format!(
                    "      return pc.concat(pc.encodeU8(0), {ok_encode});\n"
                ));
                out.push_str("    } else {\n");
                out.push_str(&format!(
                    "      return pc.concat(pc.encodeU8(1), pc.encodeU8(0), {err_encode});\n"
                ));
                out.push_str("    }\n");
            } else {
                // Infallible method - just encode the result
                let encode_expr = generate_encode_expr(method.return_type, "result");
                out.push_str(&format!("    return encodeResultOk({encode_expr});\n"));
            }
        } else {
            // Streaming method - must use streaming dispatcher
            out.push_str(
                "    // Channeling method - use streamingDispatch() instead of simple RPC dispatch\n",
            );
            out.push_str("    return encodeResultErr(encodeInvalidPayload());\n");
        }

        out.push_str("  }],\n");
    }

    out.push_str("]);\n\n");
    out
}

/// Generate streaming method handlers.
///
/// These handlers receive the registry and taskSender to properly bind streams.
pub fn generate_streaming_handlers(service: &ServiceDetail) -> String {
    use crate::render::hex_u64;

    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();
    let service_name_lower = service.name.to_lower_camel_case();

    // Type for streaming method handler
    out.push_str(&format!(
        "// Streaming method handler type for {service_name}\n"
    ));
    out.push_str("export type ChannelingMethodHandler<H> = (\n  handler: H,\n  payload: Uint8Array,\n  requestId: bigint,\n  registry: ChannelRegistry,\n  taskSender: TaskSender,\n) => Promise<void>;\n\n");

    // Generate streaming handlers map
    out.push_str(&format!(
        "// Streaming method handlers for {service_name}\n"
    ));
    out.push_str(&format!(
        "export const {service_name_lower}_streamingHandlers = new Map<bigint, ChannelingMethodHandler<{service_name}Handler>>([\n"
    ));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let id = crate::method_id(method);

        out.push_str(&format!(
            "  [{}n, async (handler, payload, requestId, registry, taskSender) => {{\n",
            hex_u64(id)
        ));
        out.push_str("    try {\n");
        out.push_str("      const buf = payload;\n");
        out.push_str("      let offset = 0;\n");

        // Decode all arguments with proper stream binding
        for arg in &method.args {
            let arg_name = arg.name.to_lower_camel_case();
            let decode_stmt = generate_decode_stmt_server_streaming(
                arg.ty,
                &arg_name,
                "offset",
                "registry",
                "taskSender",
            );
            out.push_str(&format!("      {decode_stmt}\n"));
        }
        out.push_str(
            "      if (offset !== buf.length) throw new Error(\"args: trailing bytes\");\n",
        );

        // Call handler
        let arg_names = method
            .args
            .iter()
            .map(|a| a.name.to_lower_camel_case())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "      const result = await handler.{method_name}({arg_names});\n"
        ));

        // Close any Tx streams that were passed as arguments
        for arg in &method.args {
            if is_tx(arg.ty) {
                let arg_name = arg.name.to_lower_camel_case();
                out.push_str(&format!("      {arg_name}.close();\n"));
            }
        }

        // Encode and send response via taskSender
        // Check if return type is Result<T, E> - if so, encode as Result<T, RoamError<User(E)>>
        if let ShapeKind::Result { ok, err } = classify_shape(method.return_type) {
            // Handler returns { ok: true; value: T } | { ok: false; error: E }
            // Wire format: [0] + T for success, [1, 0] + E for User error
            let ok_encode = generate_encode_expr(ok, "result.value");
            let err_encode = generate_encode_expr(err, "result.error");
            out.push_str("      if (result.ok) {\n");
            out.push_str(&format!(
                "        taskSender({{ kind: 'response', requestId, payload: pc.concat(pc.encodeU8(0), {ok_encode}) }});\n"
            ));
            out.push_str("      } else {\n");
            out.push_str(&format!(
                "        taskSender({{ kind: 'response', requestId, payload: pc.concat(pc.encodeU8(1), pc.encodeU8(0), {err_encode}) }});\n"
            ));
            out.push_str("      }\n");
        } else {
            let encode_expr = generate_encode_expr(method.return_type, "result");
            out.push_str(&format!(
                "      taskSender({{ kind: 'response', requestId, payload: encodeResultOk({encode_expr}) }});\n"
            ));
        }

        out.push_str("    } catch (e) {\n");
        out.push_str(
            "      taskSender({ kind: 'response', requestId, payload: encodeResultErr(encodeInvalidPayload()) });\n",
        );
        out.push_str("    }\n");
        out.push_str("  }],\n");
    }

    out.push_str("]);\n\n");
    out
}

/// Generate complete server code (interface + handlers).
pub fn generate_server(service: &ServiceDetail) -> String {
    let mut out = String::new();

    // Generate handler interface
    out.push_str(&generate_handler_interface(service));

    // Generate RPC method handlers
    out.push_str(&generate_method_handlers(service));

    // Check if any method uses streaming
    let has_streaming = service.methods.iter().any(|m| {
        m.args.iter().any(|a| is_tx(a.ty) || is_rx(a.ty))
            || is_tx(m.return_type)
            || is_rx(m.return_type)
    });

    // Generate streaming handlers if needed
    if has_streaming {
        out.push_str(&generate_streaming_handlers(service));
    }

    out
}
