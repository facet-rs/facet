//! TypeScript server/handler generation.
//!
//! Generates the handler interface and a Dispatcher class that routes calls
//! to handler methods. All encode/decode is handled by the runtime via the
//! service descriptor — no serialization code in generated output.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_types::{ServiceDescriptor, ShapeKind, classify_shape};

use super::types::{ts_type_server_arg, ts_type_server_return};

/// Generate handler interface (for implementing the service).
///
/// r[impl rpc.channel.binding] - Handler binds channels in args.
pub fn generate_handler_interface(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();

    out.push_str(&format!("// Handler interface for {service_name}\n"));
    out.push_str(&format!("export interface {service_name}Handler {{\n"));

    for method in service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let args = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    ts_type_server_arg(a.shape)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let ret_ty = ts_type_server_return(method.return_shape);

        out.push_str(&format!(
            "  {method_name}({args}): Promise<{ret_ty}> | {ret_ty};\n"
        ));
    }

    out.push_str("}\n\n");
    out
}

/// Generate the Dispatcher class.
///
/// Implements `ChannelingDispatcher` from roam-core:
/// - `getDescriptor()` returns the service descriptor
/// - `dispatch(method, args, call)` routes by method ID and calls handler methods
///
/// The runtime handles all arg decoding (using method.args tuple schema) and
/// response encoding (using method.result schema via call.reply/replyErr).
/// Generated dispatch code only does type casts and handler invocation.
pub fn generate_dispatcher_class(service: &ServiceDescriptor) -> String {
    use crate::render::hex_u64;

    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();
    let service_name_lower = service.service_name.to_lower_camel_case();

    out.push_str(&format!("// Dispatcher for {service_name}\n"));
    out.push_str(&format!(
        "export class {service_name}Dispatcher implements ChannelingDispatcher {{\n"
    ));
    out.push_str(&format!(
        "  constructor(private readonly handler: {service_name}Handler) {{}}\n\n"
    ));

    // getDescriptor()
    out.push_str("  getDescriptor(): ServiceDescriptor {\n");
    out.push_str(&format!("    return {service_name_lower}_descriptor;\n"));
    out.push_str("  }\n\n");

    // dispatch()
    out.push_str(
        "  async dispatch(method: MethodDescriptor, args: unknown[], call: RoamCall): Promise<void> {\n",
    );

    let mut first = true;
    for method in service.methods {
        let method_name = method.method_name.to_lower_camel_case();
        let id = crate::method_id(method);
        let is_fallible = matches!(
            classify_shape(method.return_shape),
            ShapeKind::Result { .. }
        );

        // Build typed arg list from args array
        let arg_names: Vec<_> = method
            .args
            .iter()
            .map(|a| a.name.to_lower_camel_case())
            .collect();
        let typed_args: Vec<_> = method
            .args
            .iter()
            .enumerate()
            .map(|(i, a)| format!("args[{i}] as {}", ts_type_server_arg(a.shape)))
            .collect();

        // Find Tx arg indices for closing after handler returns
        let tx_arg_indices: Vec<usize> = method
            .args
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(classify_shape(a.shape), ShapeKind::Tx { .. }))
            .map(|(i, _)| i)
            .collect();

        let keyword = if first { "if" } else { "} else if" };
        first = false;

        out.push_str(&format!(
            "    {keyword} (method.id === {}n) {{\n",
            hex_u64(id)
        ));
        out.push_str("      try {\n");
        out.push_str(&format!(
            "        const result = await this.handler.{method_name}({});\n",
            typed_args.join(", ")
        ));

        // Close Tx args before replying (ensures Close messages precede Response)
        for i in &tx_arg_indices {
            let arg_name = &arg_names[*i];
            out.push_str(&format!(
                "        (args[{i}] as {{ close(): void }}).close(); // close {arg_name} before reply\n"
            ));
        }

        if is_fallible {
            out.push_str("        if (result.ok) call.reply(result.value); else call.replyErr(result.error);\n");
        } else {
            out.push_str("        call.reply(result);\n");
        }

        out.push_str("      } catch {\n");
        out.push_str("        call.replyInternalError();\n");
        out.push_str("      }\n");
    }

    if !first {
        out.push_str("    }\n");
    }

    out.push_str("  }\n");
    out.push_str("}\n\n");
    out
}

/// Generate complete server code (handler interface + dispatcher class).
pub fn generate_server(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    out.push_str(&generate_handler_interface(service));
    out.push_str(&generate_dispatcher_class(service));
    out
}
