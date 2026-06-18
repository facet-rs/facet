//! TypeScript client generation.
//!
//! Generates client interface and implementation for making caller-visible RPC
//! calls. Each generated method issues one request attempt.
//! The client uses the canonical service schema table for request/response
//! encode/decode. No method-specific serialization code is generated here.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use vox_types::{ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use super::types::{ts_type_client_arg, ts_type_client_return};

/// Format a doc comment for TypeScript/JSDoc.
fn format_doc_comment(doc: &str, indent: &str) -> String {
    let lines: Vec<&str> = doc.lines().collect();

    if lines.is_empty() {
        return String::new();
    }

    if lines.len() == 1 {
        format!("{}/** {} */\n", indent, lines[0].trim())
    } else {
        let mut out = format!("{}/**\n", indent);
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                out.push_str(&format!("{} *\n", indent));
            } else {
                out.push_str(&format!("{} * {}\n", indent, trimmed));
            }
        }
        out.push_str(&format!("{} */\n", indent));
        out
    }
}

/// Generate caller interface for making caller-visible RPC calls to the service.
///
/// r[impl rpc.channel.binding] - Caller binds channels in args.
pub fn generate_caller_interface(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();

    out.push_str(&format!("// Caller interface for {service_name}\n"));
    out.push_str(&format!("export interface {service_name}Caller {{\n"));

    for method in service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let args = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    ts_type_client_arg(a.shape)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let ret_ty = ts_type_client_return(method.return_shape);

        if let Some(doc) = &method.doc {
            out.push_str(&format_doc_comment(doc, "  "));
        }
        out.push_str(&format!("  {method_name}({args}): Promise<{ret_ty}>;\n"));
    }

    out.push_str("}\n\n");
    out
}

/// Generate client implementation.
///
/// Each generated client method represents one RPC call:
/// 1. Binds to its generated `MethodDescriptor` constant
/// 2. Binds any channel args (via canonical arg refs if streaming)
/// 3. Calls `caller.call({ method, args, descriptor, ... })` to start a
///    request attempt
/// 4. The runtime encodes/decodes using the canonical service schema table
pub fn generate_client_impl(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();
    let service_name_lower = service.service_name.to_lower_camel_case();

    out.push_str(&format!("// Client implementation for {service_name}\n"));
    out.push_str(&format!(
        "export class {service_name}Client implements {service_name}Caller {{\n"
    ));
    out.push_str("  static get descriptor(): ServiceDescriptor {\n");
    out.push_str(&format!("    return {service_name_lower}_descriptor;\n"));
    out.push_str("  }\n\n");
    out.push_str("  private caller: Caller;\n\n");
    out.push_str("  constructor(caller: Caller) {\n");
    out.push_str("    this.caller = caller;\n");
    out.push_str("  }\n\n");

    for method in service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let method_descriptor_name = format!("{service_name_lower}_{method_name}_method");

        let has_streaming_args = method.args.iter().any(|a| is_tx(a.shape) || is_rx(a.shape));

        let args = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    ts_type_client_arg(a.shape)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        let ret_ty = ts_type_client_return(method.return_shape);

        let args_record = if method.args.is_empty() {
            "{}".to_string()
        } else {
            let fields: Vec<_> = method
                .args
                .iter()
                .map(|a| a.name.to_lower_camel_case())
                .collect();
            format!("{{ {} }}", fields.join(", "))
        };

        if let Some(doc) = &method.doc {
            out.push_str(&format_doc_comment(doc, "  "));
        }
        out.push_str(&format!(
            "  async {method_name}({args}): Promise<{ret_ty}> {{\n"
        ));

        let _ = has_streaming_args;
        // r[impl rpc.method-id]
        let method_id_hex = crate::render::hex_u64(crate::method_id(method));
        // The per-call phon schema data: the method's args/ok roots + args schema
        // closure (from `{service}Methods`) and the service phon `Registry`.
        let call_fields = format!(
            "      method: \"{service_name}.{method_name}\",\n      args: {args_record},\n      descriptor: {method_descriptor_name},\n      methodSchemas: {service_name_lower}Methods[\"{method_id_hex}\"],\n      registry: {service_name_lower}Registry,\n",
        );

        let is_fallible = matches!(
            classify_shape(method.return_shape),
            ShapeKind::Result { .. }
        );

        if is_fallible {
            out.push_str("    try {\n");
            out.push_str("      const __voxResult = await this.caller.call({\n");
            out.push_str(&call_fields);
            out.push_str("      });\n");
            out.push_str(&format!(
                "      return {{ ok: true, value: __voxResult }} as {ret_ty};\n"
            ));
            out.push_str("    } catch (e: any) {\n");
            out.push_str("      if (e instanceof RpcError && e.isUserError()) {\n");
            out.push_str(&format!(
                "        return {{ ok: false, error: e.userError }} as {ret_ty};\n"
            ));
            out.push_str("      }\n");
            out.push_str("      throw e;\n");
            out.push_str("    }\n");
        } else {
            out.push_str("    const __voxResult = await this.caller.call({\n");
            out.push_str(&call_fields);
            out.push_str("    });\n");
            out.push_str(&format!("    return __voxResult as {ret_ty};\n"));
        }

        out.push_str("  }\n\n");
    }

    out.push_str("}\n\n");
    out
}

/// Generate a connect() helper function for WebSocket connections.
pub fn generate_connect_function(service: &ServiceDescriptor) -> String {
    let service_name = service.service_name.to_upper_camel_case();

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
        "export async function connect{service_name}(\n  url: string,\n  options: ConnectLaneOptions = {{}},\n): Promise<{service_name}Client> {{\n"
    ));
    out.push_str(&format!(
        "  return connectLane(wsConnector(url), {service_name}Client, {{\n    ...options,\n    laneMetadata: options.laneMetadata ?? voxServiceMetadata(\"{}\"),\n  }});\n",
        service.service_name
    ));
    out.push_str("}\n\n");
    out
}

pub fn generate_client_with_connect_helper(
    service: &ServiceDescriptor,
    connect_helper: bool,
) -> String {
    let mut out = String::new();
    out.push_str(&generate_caller_interface(service));
    out.push_str(&generate_client_impl(service));
    if connect_helper {
        out.push_str(&generate_connect_function(service));
    }
    out
}

/// Generate complete client code (interface + implementation + connect helper).
pub fn generate_client(service: &ServiceDescriptor) -> String {
    generate_client_with_connect_helper(service, true)
}
