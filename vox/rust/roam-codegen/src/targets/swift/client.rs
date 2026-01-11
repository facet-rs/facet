//! Swift client generation.
//!
//! Generates caller protocol and client implementation for making RPC calls.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_schema::{MethodDetail, ServiceDetail, is_rx, is_tx};

use super::decode::{generate_decode_stmt, generate_decode_stmt_from};
use super::encode::generate_encode_expr;
use super::types::{format_doc, is_stream, swift_type_client_arg, swift_type_client_return};
use crate::code_writer::CodeWriter;
use crate::cw_writeln;
use crate::render::hex_u64;

/// Generate complete client code (caller protocol + client implementation).
pub fn generate_client(service: &ServiceDetail) -> String {
    let mut out = String::new();
    out.push_str(&generate_caller_protocol(service));
    out.push_str(&generate_client_impl(service));
    out
}

/// Generate caller protocol (for making calls to the service).
fn generate_caller_protocol(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();

    if let Some(doc) = &service.doc {
        out.push_str(&format_doc(doc, ""));
    }
    out.push_str(&format!("public protocol {service_name}Caller {{\n"));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();

        if let Some(doc) = &method.doc {
            out.push_str(&format_doc(doc, "    "));
        }

        let args: Vec<String> = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    swift_type_client_arg(a.ty)
                )
            })
            .collect();

        let ret_type = swift_type_client_return(method.return_type);

        if ret_type == "Void" {
            out.push_str(&format!(
                "    func {method_name}({}) async throws\n",
                args.join(", ")
            ));
        } else {
            out.push_str(&format!(
                "    func {method_name}({}) async throws -> {ret_type}\n",
                args.join(", ")
            ));
        }
    }

    out.push_str("}\n\n");
    out
}

/// Generate client implementation (for making calls to the service).
fn generate_client_impl(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let mut w = CodeWriter::with_indent_spaces(&mut out, 4);
    let service_name = service.name.to_upper_camel_case();

    w.writeln(&format!(
        "public final class {service_name}Client: {service_name}Caller {{"
    ))
    .unwrap();
    {
        let _indent = w.indent();
        w.writeln("private let connection: RoamConnection").unwrap();
        w.blank_line().unwrap();
        w.writeln("public init(connection: RoamConnection) {")
            .unwrap();
        {
            let _indent = w.indent();
            w.writeln("self.connection = connection").unwrap();
        }
        w.writeln("}").unwrap();

        for method in &service.methods {
            w.blank_line().unwrap();
            generate_client_method(&mut w, method, &service_name);
        }
    }
    w.writeln("}").unwrap();
    w.blank_line().unwrap();

    out
}

/// Generate a single client method implementation.
fn generate_client_method(
    w: &mut CodeWriter<&mut String>,
    method: &MethodDetail,
    service_name: &str,
) {
    let method_name = method.method_name.to_lower_camel_case();
    let method_id_name = method.method_name.to_lower_camel_case();

    let args: Vec<String> = method
        .args
        .iter()
        .map(|a| {
            format!(
                "{}: {}",
                a.name.to_lower_camel_case(),
                swift_type_client_arg(a.ty)
            )
        })
        .collect();

    let ret_type = swift_type_client_return(method.return_type);
    let has_streaming =
        method.args.iter().any(|a| is_stream(a.ty)) || is_stream(method.return_type);

    // Method signature
    if ret_type == "Void" {
        cw_writeln!(
            w,
            "public func {method_name}({}) async throws {{",
            args.join(", ")
        )
        .unwrap();
    } else {
        cw_writeln!(
            w,
            "public func {method_name}({}) async throws -> {ret_type} {{",
            args.join(", ")
        )
        .unwrap();
    }

    {
        let _indent = w.indent();

        if has_streaming {
            generate_streaming_client_body(w, method, service_name, &method_id_name);
        } else {
            // Encode arguments
            generate_encode_args(w, &method.args);

            // Make call
            let method_id = crate::method_id(method);
            if ret_type == "Void" {
                cw_writeln!(
                    w,
                    "_ = try await connection.call(methodId: {}, payload: payload)",
                    hex_u64(method_id)
                )
                .unwrap();
            } else {
                cw_writeln!(
                    w,
                    "let response = try await connection.call(methodId: {}, payload: payload)",
                    hex_u64(method_id)
                )
                .unwrap();
                // Decode return value
                w.writeln("var offset = 0").unwrap();
                let decode_stmt =
                    generate_decode_stmt_from(method.return_type, "result", "", "response");
                for line in decode_stmt.lines() {
                    w.writeln(line).unwrap();
                }
                w.writeln("return result").unwrap();
            }
        }
    }
    w.writeln("}").unwrap();
}

/// Generate code to encode method arguments (for client).
fn generate_encode_args(w: &mut CodeWriter<&mut String>, args: &[roam_schema::ArgDetail]) {
    if args.is_empty() {
        w.writeln("let payload = Data()").unwrap();
        return;
    }

    w.writeln("var payloadBytes: [UInt8] = []").unwrap();
    for arg in args {
        let arg_name = arg.name.to_lower_camel_case();
        let encode_expr = generate_encode_expr(arg.ty, &arg_name);
        cw_writeln!(w, "payloadBytes += {encode_expr}").unwrap();
    }
    w.writeln("let payload = Data(payloadBytes)").unwrap();
}

/// Generate client body for streaming methods.
fn generate_streaming_client_body(
    w: &mut CodeWriter<&mut String>,
    method: &MethodDetail,
    service_name: &str,
    method_id_name: &str,
) {
    let service_name_lower = service_name.to_lower_camel_case();

    // Bind channels
    let arg_names: Vec<String> = method
        .args
        .iter()
        .map(|a| a.name.to_lower_camel_case())
        .collect();

    w.writeln("// Bind channels using schema").unwrap();
    w.writeln("await bindChannels(").unwrap();
    {
        let _indent = w.indent();
        cw_writeln!(
            w,
            "schemas: {service_name_lower}_schemas[\"{method_id_name}\"]!.args,"
        )
        .unwrap();
        cw_writeln!(w, "args: [{}],", arg_names.join(", ")).unwrap();
        w.writeln("allocator: connection.channelAllocator,")
            .unwrap();
        w.writeln("incomingRegistry: connection.incomingChannelRegistry,")
            .unwrap();
        w.writeln("taskSender: connection.taskSender,").unwrap();
        cw_writeln!(w, "serializers: {service_name}Serializers()").unwrap();
    }
    w.writeln(")").unwrap();
    w.blank_line().unwrap();

    // Encode payload (channel IDs for Tx/Rx, values for regular args)
    w.writeln("// Encode payload with channel IDs").unwrap();
    w.writeln("var payloadBytes: [UInt8] = []").unwrap();
    for arg in &method.args {
        let arg_name = arg.name.to_lower_camel_case();
        if is_tx(arg.ty) || is_rx(arg.ty) {
            cw_writeln!(w, "payloadBytes += encodeVarint({arg_name}.channelId)").unwrap();
        } else {
            let encode_expr = generate_encode_expr(arg.ty, &arg_name);
            cw_writeln!(w, "payloadBytes += {encode_expr}").unwrap();
        }
    }
    w.writeln("let payload = Data(payloadBytes)").unwrap();
    w.blank_line().unwrap();

    // Make the call
    let ret_type = swift_type_client_return(method.return_type);
    let method_id = crate::method_id(method);
    if ret_type == "Void" {
        cw_writeln!(
            w,
            "_ = try await connection.call(methodId: {}, payload: payload)",
            hex_u64(method_id)
        )
        .unwrap();
    } else {
        cw_writeln!(
            w,
            "let response = try await connection.call(methodId: {}, payload: payload)",
            hex_u64(method_id)
        )
        .unwrap();
        w.writeln("var offset = 0").unwrap();
        let decode_stmt = generate_decode_stmt(method.return_type, "result", "");
        for line in decode_stmt.lines() {
            w.writeln(line).unwrap();
        }
        w.writeln("return result").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use roam_schema::{ArgDetail, MethodDetail, ServiceDetail};
    use std::borrow::Cow;

    fn sample_service() -> ServiceDetail {
        ServiceDetail {
            name: Cow::Borrowed("Echo"),
            doc: Some(Cow::Borrowed("Simple echo service")),
            methods: vec![MethodDetail {
                service_name: Cow::Borrowed("Echo"),
                method_name: Cow::Borrowed("echo"),
                args: vec![ArgDetail {
                    name: Cow::Borrowed("message"),
                    ty: <String as Facet>::SHAPE,
                }],
                return_type: <String as Facet>::SHAPE,
                doc: Some(Cow::Borrowed("Echo back the message")),
            }],
        }
    }

    #[test]
    fn test_generate_caller_protocol() {
        let service = sample_service();
        let code = generate_caller_protocol(&service);

        assert!(code.contains("protocol EchoCaller"));
        assert!(code.contains("func echo(message: String)"));
        assert!(code.contains("async throws -> String"));
    }

    #[test]
    fn test_generate_client_impl() {
        let service = sample_service();
        let code = generate_client_impl(&service);

        assert!(code.contains("class EchoClient"));
        assert!(code.contains("EchoCaller"));
        assert!(code.contains("RoamConnection"));
        assert!(code.contains("public func echo"));
    }
}
