//! TypeScript client generation.
//!
//! Generates client interface and implementation for making RPC calls.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_schema::{ServiceDetail, ShapeKind, classify_shape, is_rx, is_tx};

use super::types::{ts_type_client_arg, ts_type_client_return};

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
///
/// Uses schema-driven encoding/decoding via `encodeWithSchema`/`decodeWithSchema`.
pub fn generate_client_impl(service: &ServiceDetail) -> String {
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

        if let Some(doc) = &method.doc {
            out.push_str(&format!("  /** {} */\n", doc));
        }
        out.push_str(&format!(
            "  async {method_name}({args}): Promise<{ret_ty}> {{\n"
        ));

        // Get schema reference
        out.push_str(&format!(
            "    const schema = {service_name_lower}_schemas.{method_name};\n"
        ));

        // If method has streaming args, bind channels first
        if has_streaming_args {
            let arg_names: Vec<_> = method
                .args
                .iter()
                .map(|a| a.name.to_lower_camel_case())
                .collect();
            out.push_str("    // Bind any Tx/Rx channels in arguments and collect channel IDs\n");
            out.push_str(&format!(
                "    const channels = bindChannels(\n      schema.args,\n      [{}],\n      this.conn.getChannelAllocator(),\n      this.conn.getChannelRegistry(),\n      {service_name_lower}_serializers,\n    );\n",
                arg_names.join(", ")
            ));
        }

        // Encode payload using schema
        if method.args.is_empty() {
            out.push_str("    const payload = new Uint8Array(0);\n");
        } else if method.args.len() == 1 {
            let arg_name = method.args[0].name.to_lower_camel_case();
            out.push_str(&format!(
                "    const payload = encodeWithSchema({arg_name}, schema.args[0]);\n"
            ));
        } else {
            // Multiple args - encode as tuple
            let arg_names: Vec<_> = method
                .args
                .iter()
                .map(|a| a.name.to_lower_camel_case())
                .collect();
            out.push_str(&format!(
                "    const payload = encodeWithSchema([{}], {{ kind: 'tuple', elements: schema.args }});\n",
                arg_names.join(", ")
            ));
        }

        // Call the server
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

        // Check if this method returns Result<T, E>
        let is_fallible = matches!(classify_shape(method.return_type), ShapeKind::Result { .. });

        if is_fallible {
            // Fallible method: handle both success and user error
            out.push_str("    try {\n");
            out.push_str("      const offset = decodeRpcResult(response, 0);\n");
            out.push_str(
                "      const value = decodeWithSchema(response, offset, schema.returns).value;\n",
            );
            out.push_str(&format!(
                "      return {{ ok: true, value }} as {ret_ty};\n"
            ));
            out.push_str("    } catch (e) {\n");
            out.push_str("      if (e instanceof RpcError && e.isUserError() && e.payload && schema.error) {\n");
            out.push_str(
                "        const error = decodeWithSchema(e.payload, 0, schema.error).value;\n",
            );
            out.push_str(&format!(
                "        return {{ ok: false, error }} as {ret_ty};\n"
            ));
            out.push_str("      }\n");
            out.push_str("      throw e;\n");
            out.push_str("    }\n");
        } else {
            // Infallible method: just decode success
            out.push_str("    const offset = decodeRpcResult(response, 0);\n");
            out.push_str(
                "    const result = decodeWithSchema(response, offset, schema.returns).value;\n",
            );
            out.push_str(&format!("    return result as {ret_ty};\n"));
        }

        out.push_str("  }\n\n");
    }

    out.push_str("}\n\n");
    out
}

/// Generate a connect() helper function for WebSocket connections.
pub fn generate_connect_function(service: &ServiceDetail) -> String {
    use heck::ToUpperCamelCase;

    let service_name = service.name.to_upper_camel_case();

    let mut out = String::new();
    out.push_str(&format!(
        "/**\n * Connect to a {service_name} server over WebSocket.\n"
    ));
    out.push_str(" * @param url - WebSocket URL (e.g., \"ws://localhost:9000\")\n");
    out.push_str(&format!(
        " * @returns A connected {service_name}Client instance\n"
    ));
    out.push_str(" */\n");
    out.push_str(&format!(
        "export async function connect{service_name}(url: string): Promise<{service_name}Client<WsTransport>> {{\n"
    ));
    out.push_str("  const transport = await connectWs(url);\n");
    out.push_str("  const connection = await helloExchangeInitiator(transport, defaultHello());\n");
    out.push_str(&format!("  return new {service_name}Client(connection);\n"));
    out.push_str("}\n\n");
    out
}

/// Generate complete client code (interface + implementation + connect helper).
pub fn generate_client(service: &ServiceDetail) -> String {
    let mut out = String::new();
    out.push_str(&generate_caller_interface(service));
    out.push_str(&generate_client_impl(service));
    out.push_str(&generate_connect_function(service));
    out
}
