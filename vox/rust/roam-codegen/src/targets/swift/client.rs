//! Swift client generation.
//!
//! Generates the caller protocol and client implementation for making
//! application-level RPC calls. Each generated client method represents one
//! logical call; the runtime may realize that call with one request attempt or
//! multiple request attempts if retry/session recovery creates later attempts
//! for the same operation.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_types::{MethodDescriptor, ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use super::decode::generate_decode_stmt_from_with_cursor;
use super::encode::generate_encode_expr;
use super::types::{format_doc, is_channel, swift_type_client_arg, swift_type_client_return};
use crate::code_writer::CodeWriter;
use crate::cw_writeln;
use crate::render::hex_u64;

fn swift_retry_policy_literal(method: &MethodDescriptor) -> &'static str {
    match (method.retry.persist, method.retry.idem) {
        (false, false) => ".volatile",
        (false, true) => ".idem",
        (true, false) => ".persist",
        (true, true) => ".persistIdem",
    }
}

/// Generate complete client code (caller protocol + client implementation).
///
/// The generated API speaks in terms of application-level calls, while the
/// runtime beneath it sends wire-level request attempts and receives responses.
pub fn generate_client(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    out.push_str(&generate_caller_protocol(service));
    out.push_str(&generate_client_impl(service));
    out
}

/// Generate caller protocol for making application-level service calls.
fn generate_caller_protocol(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();

    if let Some(doc) = &service.doc {
        out.push_str(&format_doc(doc, ""));
    }
    out.push_str(&format!("public protocol {service_name}Caller {{\n"));

    for method in service.methods {
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
                    swift_type_client_arg(a.shape)
                )
            })
            .collect();

        let ret_type = swift_type_client_return(method.return_shape);

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

/// Generate client implementation for making application-level service calls.
fn generate_client_impl(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let mut w = CodeWriter::with_indent_spaces(&mut out, 4);
    let service_name = service.service_name.to_upper_camel_case();

    w.writeln(&format!(
        "public final class {service_name}Client: {service_name}Caller, Sendable {{"
    ))
    .unwrap();
    {
        let _indent = w.indent();
        w.writeln("private let connection: RoamConnection").unwrap();
        w.writeln("private let timeout: TimeInterval?").unwrap();
        w.blank_line().unwrap();
        w.writeln("public init(connection: RoamConnection, timeout: TimeInterval? = 30.0) {")
            .unwrap();
        {
            let _indent = w.indent();
            w.writeln("self.connection = connection").unwrap();
            w.writeln("self.timeout = timeout").unwrap();
        }
        w.writeln("}").unwrap();

        for method in service.methods {
            w.blank_line().unwrap();
            generate_client_method(&mut w, method, &service_name);
        }
    }
    w.writeln("}").unwrap();
    w.blank_line().unwrap();

    out
}

/// Generate a single client method implementation.
///
/// One generated method corresponds to one logical call. At runtime, the
/// underlying connection sends one request attempt immediately, and may later
/// send additional request attempts for the same logical operation if retry or
/// session recovery requires it.
fn generate_client_method(
    w: &mut CodeWriter<&mut String>,
    method: &MethodDescriptor,
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
                swift_type_client_arg(a.shape)
            )
        })
        .collect();

    let ret_type = swift_type_client_return(method.return_shape);
    let has_streaming = method.args.iter().any(|a| is_channel(a.shape));
    let retry_policy = swift_retry_policy_literal(method);

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
        let cursor_var = unique_decode_cursor_name(method.args);

        if has_streaming {
            generate_streaming_client_body(
                w,
                method,
                service_name,
                &method_id_name,
                &cursor_var,
                retry_policy,
            );
        } else {
            // Encode arguments
            generate_encode_args(w, method.args);

            // Start the first request attempt for this logical call.
            let method_id = crate::method_id(method);
            cw_writeln!(
                w,
                "let response = try await connection.call(methodId: {}, payload: payload, channels: [], retry: {retry_policy}, timeout: timeout)",
                hex_u64(method_id),
            )
            .unwrap();
            generate_response_decode(w, method, &cursor_var, "response");
        }
    }
    w.writeln("}").unwrap();
}

/// Generate code to encode method arguments (for client).
fn generate_encode_args(w: &mut CodeWriter<&mut String>, args: &[roam_types::ArgDescriptor]) {
    generate_encode_args_filtered(w, args, false);
}

fn generate_encode_args_filtered(
    w: &mut CodeWriter<&mut String>,
    args: &[roam_types::ArgDescriptor],
    skip_channels: bool,
) {
    if args.is_empty() {
        w.writeln("let payload = Data()").unwrap();
        return;
    }

    w.writeln("var payloadBytes: [UInt8] = []").unwrap();
    for arg in args {
        if skip_channels && (is_tx(arg.shape) || is_rx(arg.shape)) {
            continue;
        }
        let arg_name = arg.name.to_lower_camel_case();
        let encode_expr = generate_encode_expr(arg.shape, &arg_name);
        cw_writeln!(w, "payloadBytes += {encode_expr}").unwrap();
    }
    w.writeln("let payload = Data(payloadBytes)").unwrap();
}

/// Generate client body for channel-bearing methods.
///
/// These methods still represent one logical call at the API level, but the
/// request payload and channel bindings may need to be rebuilt for later
/// request attempts if retry/session recovery triggers another attempt for the
/// same operation.
fn generate_streaming_client_body(
    w: &mut CodeWriter<&mut String>,
    method: &MethodDescriptor,
    service_name: &str,
    method_id_name: &str,
    cursor_var: &str,
    retry_policy: &str,
) {
    let service_name_lower = service_name.to_lower_camel_case();

    let arg_names: Vec<String> = method
        .args
        .iter()
        .map(|a| a.name.to_lower_camel_case())
        .collect();

    w.writeln("let prepareRetry: @Sendable () async -> PreparedRetryRequest = { [connection] in")
        .unwrap();
    {
        let _indent = w.indent();
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
        generate_encode_args_filtered(w, method.args, true);
        cw_writeln!(
            w,
            "let channels = collectChannelIds(schemas: {service_name_lower}_schemas[\"{method_id_name}\"]!.args, args: [{}])",
            arg_names.join(", ")
        )
        .unwrap();
        w.writeln("return PreparedRetryRequest(payload: Array(payload), channels: channels)")
            .unwrap();
    }
    w.writeln("}").unwrap();
    w.writeln("let prepared = await prepareRetry()").unwrap();
    w.blank_line().unwrap();

    // Start the first request attempt for this logical call.
    let ret_type = swift_type_client_return(method.return_shape);
    let method_id = crate::method_id(method);
    let _ = ret_type;
    cw_writeln!(
        w,
        "let response = try await connection.call(methodId: {}, payload: Data(prepared.payload), channels: prepared.channels, retry: {retry_policy}, timeout: timeout, prepareRetry: prepareRetry, finalizeChannels: {{ finalizeBoundChannels(schemas: {service_name_lower}_schemas[\"{method_id_name}\"]!.args, args: [{}]) }})",
        hex_u64(method_id),
        arg_names.join(", ")
    )
    .unwrap();
    generate_response_decode(w, method, cursor_var, "response");
}

fn unique_decode_cursor_name(args: &[roam_types::ArgDescriptor]) -> String {
    let arg_names: Vec<String> = args.iter().map(|a| a.name.to_lower_camel_case()).collect();
    let mut candidate = String::from("cursor");
    while arg_names.iter().any(|name| name == &candidate) {
        candidate.push('_');
    }
    candidate
}

/// Generate code to decode the wire response payload for one request attempt:
/// `Result<T, RoamError<E>>`.
fn generate_response_decode(
    w: &mut CodeWriter<&mut String>,
    method: &MethodDescriptor,
    cursor_var: &str,
    response_var: &str,
) {
    let ret_type = swift_type_client_return(method.return_shape);
    let result_disc_var = format!("_{cursor_var}_resultDisc");
    let error_code_var = format!("_{cursor_var}_errorCode");
    let is_fallible = matches!(
        classify_shape(method.return_shape),
        ShapeKind::Result { .. }
    );

    cw_writeln!(w, "var {cursor_var} = 0").unwrap();
    cw_writeln!(
        w,
        "let {result_disc_var} = try decodeVarint(from: {response_var}, offset: &{cursor_var})"
    )
    .unwrap();
    cw_writeln!(w, "switch {result_disc_var} {{").unwrap();

    w.writeln("case 0:").unwrap();
    {
        let _indent = w.indent();
        if is_fallible {
            let ShapeKind::Result { ok, .. } = classify_shape(method.return_shape) else {
                unreachable!()
            };
            let decode_ok =
                generate_decode_stmt_from_with_cursor(ok, "value", "", response_var, cursor_var);
            for line in decode_ok.lines() {
                w.writeln(line).unwrap();
            }
            w.writeln("return .success(value)").unwrap();
        } else if ret_type == "Void" {
            w.writeln("return").unwrap();
        } else {
            let decode_stmt = generate_decode_stmt_from_with_cursor(
                method.return_shape,
                "result",
                "",
                response_var,
                cursor_var,
            );
            for line in decode_stmt.lines() {
                w.writeln(line).unwrap();
            }
            w.writeln("return result").unwrap();
        }
    }

    w.writeln("case 1:").unwrap();
    {
        let _indent = w.indent();
        cw_writeln!(
            w,
            "let {error_code_var} = try decodeU8(from: {response_var}, offset: &{cursor_var})"
        )
        .unwrap();
        cw_writeln!(w, "switch {error_code_var} {{").unwrap();

        w.writeln("case 0:").unwrap();
        {
            let _indent = w.indent();
            if is_fallible {
                let ShapeKind::Result { err, .. } = classify_shape(method.return_shape) else {
                    unreachable!()
                };
                let decode_err = generate_decode_stmt_from_with_cursor(
                    err,
                    "userError",
                    "",
                    response_var,
                    cursor_var,
                );
                for line in decode_err.lines() {
                    w.writeln(line).unwrap();
                }
                w.writeln("return .failure(userError)").unwrap();
            } else {
                w.writeln(
                    "throw RoamError.decodeError(\"unexpected user error for infallible method\")",
                )
                .unwrap();
            }
        }
        w.writeln("case 1:").unwrap();
        w.writeln("    throw RoamError.unknownMethod").unwrap();
        w.writeln("case 2:").unwrap();
        w.writeln("    throw RoamError.decodeError(\"invalid payload\")")
            .unwrap();
        w.writeln("case 3:").unwrap();
        w.writeln("    throw RoamError.cancelled").unwrap();
        w.writeln("case 4:").unwrap();
        w.writeln("    throw RoamError.indeterminate").unwrap();
        w.writeln("default:").unwrap();
        cw_writeln!(
            w,
            "    throw RoamError.decodeError(\"invalid RoamError discriminant: \\({error_code_var})\")"
        )
        .unwrap();
        w.writeln("}").unwrap();
    }

    w.writeln("default:").unwrap();
    cw_writeln!(
        w,
        "    throw RoamError.decodeError(\"invalid Result discriminant: \\({result_disc_var})\")"
    )
    .unwrap();
    w.writeln("}").unwrap();
}
