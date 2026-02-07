//! TypeScript server/handler generation.
//!
//! Generates server handler interface and method dispatch logic.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_schema::{ServiceDetail, ShapeKind, classify_shape};

use super::decode::generate_decode_stmt_server_channels;
use super::encode::generate_encode_expr;
use super::types::{ts_type_server_arg, ts_type_server_return};

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

/// Generate channel-capable method handlers.
pub fn generate_channel_handlers(service: &ServiceDetail) -> String {
    use crate::render::hex_u64;

    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();
    let service_name_lower = service.name.to_lower_camel_case();

    // Type for channel-capable method handlers.
    out.push_str(&format!("// Channel handler type for {service_name}\n"));
    out.push_str("export type ChannelingMethodHandler<H> = (\n  handler: H,\n  payload: Uint8Array,\n  requestId: bigint,\n  registry: ChannelRegistry,\n  taskSender: TaskSender,\n) => Promise<void>;\n\n");

    // Generate channel handler map.
    out.push_str(&format!("// Channel handlers for {service_name}\n"));
    out.push_str(&format!(
        "export const {service_name_lower}_channelingHandlers = new Map<bigint, ChannelingMethodHandler<{service_name}Handler>>([\n"
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

        // Decode all arguments with proper channel binding.
        for arg in &method.args {
            let arg_name = arg.name.to_lower_camel_case();
            let decode_stmt = generate_decode_stmt_server_channels(
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

        // Close any Tx channels that were passed as arguments.
        for arg in &method.args {
            if matches!(classify_shape(arg.ty), ShapeKind::Tx { .. }) {
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

/// Generate complete server code (interface + channel-capable handlers).
pub fn generate_server(service: &ServiceDetail) -> String {
    let mut out = String::new();

    // Generate handler interface
    out.push_str(&generate_handler_interface(service));

    // Always generate the channel-capable handler map.
    out.push_str(&generate_channel_handlers(service));

    out
}
