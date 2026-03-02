//! TypeScript client generation.
//!
//! Generates client interface and implementation for making RPC calls.
//! The client uses the service descriptor for schema-driven encode/decode —
//! no serialization code is generated here.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_types::{ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

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

/// Generate caller interface (for making calls to the service).
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
        out.push_str(&format!(
            "  {method_name}({args}): CallBuilder<{ret_ty}>;\n"
        ));
    }

    out.push_str("}\n\n");
    out
}

/// Generate client implementation.
///
/// Each method:
/// 1. Looks up its `MethodDescriptor` from the service descriptor by index
/// 2. Binds any channel args (via `bindChannels` if streaming)
/// 3. Calls `caller.call({ method, args, descriptor, ... })`
/// 4. The runtime encodes/decodes using the descriptor's schemas
pub fn generate_client_impl(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();
    let service_name_lower = service.service_name.to_lower_camel_case();

    out.push_str(&format!("// Client implementation for {service_name}\n"));
    out.push_str(&format!(
        "export class {service_name}Client implements {service_name}Caller {{\n"
    ));
    out.push_str("  private caller: Caller;\n\n");
    out.push_str("  constructor(caller: Caller) {\n");
    out.push_str("    this.caller = caller;\n");
    out.push_str("  }\n\n");

    for (method_idx, method) in service.methods.iter().enumerate() {
        let method_name = method.method_name.to_lower_camel_case();

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
            "  {method_name}({args}): CallBuilder<{ret_ty}> {{\n"
        ));

        // Get the method descriptor by index (known at codegen time)
        out.push_str(&format!(
            "    const descriptor = {service_name_lower}_descriptor.methods[{method_idx}];\n"
        ));

        // Bind channel args if streaming
        if has_streaming_args {
            let arg_names: Vec<_> = method
                .args
                .iter()
                .map(|a| a.name.to_lower_camel_case())
                .collect();
            out.push_str("    // Bind any Tx/Rx channels in arguments and collect channel IDs\n");
            out.push_str(&format!(
                "    const channels = bindChannels(\n      descriptor.args.elements,\n      [{}],\n      this.caller.getChannelAllocator(),\n      this.caller.getChannelRegistry(),\n    );\n",
                arg_names.join(", ")
            ));
        }

        out.push_str("    return new CallBuilder(async (metadata) => {\n");

        let is_fallible = matches!(
            classify_shape(method.return_shape),
            ShapeKind::Result { .. }
        );

        if is_fallible {
            out.push_str("      try {\n");
            out.push_str("        const value = await this.caller.call({\n");
            out.push_str(&format!(
                "          method: \"{}.{}\",\n",
                service_name, method_name
            ));
            out.push_str(&format!("          args: {},\n", args_record));
            out.push_str("          descriptor,\n");
            if has_streaming_args {
                out.push_str("          channels,\n");
            }
            out.push_str("          metadata,\n");
            out.push_str("        });\n");
            out.push_str(&format!(
                "        return {{ ok: true, value }} as {ret_ty};\n"
            ));
            out.push_str("      } catch (e) {\n");
            out.push_str("        if (e instanceof RpcError && e.isUserError()) {\n");
            out.push_str(&format!(
                "          return {{ ok: false, error: e.userError }} as {ret_ty};\n"
            ));
            out.push_str("        }\n");
            out.push_str("        throw e;\n");
            out.push_str("      }\n");
            out.push_str("    });\n");
        } else {
            out.push_str("      const value = await this.caller.call({\n");
            out.push_str(&format!(
                "        method: \"{}.{}\",\n",
                service_name, method_name
            ));
            out.push_str(&format!("        args: {},\n", args_record));
            out.push_str("        descriptor,\n");
            if has_streaming_args {
                out.push_str("        channels,\n");
            }
            out.push_str("        metadata,\n");
            out.push_str("      });\n");
            out.push_str(&format!("      return value as {ret_ty};\n"));
            out.push_str("    });\n");
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
        "export async function connect{service_name}(url: string): Promise<{service_name}Client> {{\n"
    ));
    out.push_str("  const transport = await connectWs(url);\n");
    out.push_str("  const connection = await helloExchangeInitiator(transport, defaultHello());\n");
    out.push_str(&format!(
        "  return new {service_name}Client(connection.asCaller());\n"
    ));
    out.push_str("}\n\n");
    out
}

/// Generate complete client code (interface + implementation + connect helper).
pub fn generate_client(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    out.push_str(&generate_caller_interface(service));
    out.push_str(&generate_client_impl(service));
    out.push_str(&generate_connect_function(service));
    out
}
