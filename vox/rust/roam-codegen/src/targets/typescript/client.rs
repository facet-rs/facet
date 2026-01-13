//! TypeScript client generation.
//!
//! Generates client interface and implementation for making RPC calls.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_schema::{ServiceDetail, is_rx, is_tx};

use super::types::{is_fully_supported, ts_type_client_arg, ts_type_client_return};

/// Generate caller interface (for making calls to the service).
///
/// r[impl channeling.caller-pov] - Caller uses Tx for args, Rx for returns.
pub fn generate_caller_interface(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();

    out.push_str(&format!("// Caller interface for {service_name}\n"));
    out.push_str(&format!("export interface {service_name}Caller {{\n"));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        // Caller args: Tx stays Tx, Rx stays Rx
        let args = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    ts_type_client_arg(a.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        // Caller returns
        let ret_ty = ts_type_client_return(method.return_type);

        if let Some(doc) = &method.doc {
            out.push_str(&format!("  /** {} */\n", doc));
        }
        out.push_str(&format!("  {method_name}({args}): Promise<{ret_ty}>;\n"));
    }

    out.push_str("}\n\n");
    out
}

/// Generate client implementation (for making calls to the service).
pub fn generate_client_impl(service: &ServiceDetail) -> String {
    use super::decode::generate_decode_stmt_client;
    use super::encode::generate_encode_expr;
    use crate::render::hex_u64;

    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();
    let service_name_lower = service.name.to_lower_camel_case();

    out.push_str(&format!("// Client implementation for {service_name}\n"));
    out.push_str(&format!(
        "export class {service_name}Client<T extends MessageTransport = MessageTransport> implements {service_name}Caller {{\n"
    ));
    out.push_str("  private conn: Connection<T>;\n\n");
    out.push_str("  constructor(conn: Connection<T>) {\n");
    out.push_str("    this.conn = conn;\n");
    out.push_str("  }\n\n");

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let id = crate::method_id(method);

        // Check if this method has streaming args (Tx or Rx)
        let has_streaming_args = method.args.iter().any(|a| is_tx(a.ty) || is_rx(a.ty));

        // Build args list
        let args = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    ts_type_client_arg(a.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        // Return type
        let ret_ty = ts_type_client_return(method.return_type);

        // Check if we can generate encoding/decoding for this method
        let can_encode_args = method.args.iter().all(|a| is_fully_supported(a.ty));
        let can_decode_return = is_fully_supported(method.return_type);

        if let Some(doc) = &method.doc {
            out.push_str(&format!("  /** {} */\n", doc));
        }
        out.push_str(&format!(
            "  async {method_name}({args}): Promise<{ret_ty}> {{\n"
        ));

        if can_encode_args && can_decode_return {
            // If method has streaming args, bind channels first
            if has_streaming_args {
                // Build args array for binding
                let arg_names: Vec<_> = method
                    .args
                    .iter()
                    .map(|a| a.name.to_lower_camel_case())
                    .collect();
                out.push_str(
                    "    // Bind any Tx/Rx channels in arguments and collect channel IDs\n",
                );
                out.push_str(&format!(
                    "    const channels = bindChannels(\n      {service_name_lower}_schemas.{method_name}.args,\n      [{}],\n      this.conn.getChannelAllocator(),\n      this.conn.getChannelRegistry(),\n      {service_name_lower}_serializers,\n    );\n",
                    arg_names.join(", ")
                ));
            }

            // Generate payload encoding
            if method.args.is_empty() {
                out.push_str("    const payload = new Uint8Array(0);\n");
            } else if method.args.len() == 1 {
                let arg_name = method.args[0].name.to_lower_camel_case();
                let encode_expr = generate_encode_expr(method.args[0].ty, &arg_name);
                out.push_str(&format!("    const payload = {encode_expr};\n"));
            } else {
                // Multiple args - concat their encodings
                let parts: Vec<_> = method
                    .args
                    .iter()
                    .map(|a| {
                        let arg_name = a.name.to_lower_camel_case();
                        generate_encode_expr(a.ty, &arg_name)
                    })
                    .collect();
                out.push_str(&format!(
                    "    const payload = concat({});\n",
                    parts.join(", ")
                ));
            }

            // Call the server - pass channels if method has streaming args
            if has_streaming_args {
                out.push_str(&format!(
                    "    const response = await this.conn.call({}n, payload, 30000, channels);\n",
                    hex_u64(id)
                ));
            } else {
                out.push_str(&format!(
                    "    const response = await this.conn.call({}n, payload);\n",
                    hex_u64(id)
                ));
            }

            // Parse the result (CallResult<T, RoamError>) - throws RpcError on failure
            out.push_str("    const buf = response;\n");
            out.push_str("    let offset = decodeRpcResult(buf, 0);\n");
            // For client returns, use client-aware decode
            let decode_stmt = generate_decode_stmt_client(method.return_type, "result", "offset");
            out.push_str(&format!("    {decode_stmt}\n"));
            out.push_str("    return result;\n");
        } else {
            // Unsupported - throw error
            out.push_str(
                "    throw new Error(\"Not yet implemented: encoding/decoding for this method\");\n",
            );
        }

        out.push_str("  }\n\n");
    }

    out.push_str("}\n\n");
    out
}

/// Generate complete client code (interface + implementation).
pub fn generate_client(service: &ServiceDetail) -> String {
    let mut out = String::new();
    out.push_str(&generate_caller_interface(service));
    out.push_str(&generate_client_impl(service));
    out
}
