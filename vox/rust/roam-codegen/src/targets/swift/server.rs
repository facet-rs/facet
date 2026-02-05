//! Swift server/handler generation.
//!
//! Generates handler protocol and dispatcher for routing incoming calls.

use facet_core::{ScalarType, Shape};
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_schema::{
    MethodDetail, ServiceDetail, ShapeKind, StructInfo, classify_shape, is_rx, is_tx,
};

use super::decode::{generate_decode_stmt_with_cursor, generate_inline_decode};
use super::encode::generate_encode_closure;
use super::types::{format_doc, is_stream, swift_type_server_arg, swift_type_server_return};
use crate::code_writer::CodeWriter;
use crate::cw_writeln;
use crate::render::hex_u64;

/// Generate complete server code (handler protocol + dispatchers).
pub fn generate_server(service: &ServiceDetail) -> String {
    let mut out = String::new();
    out.push_str(&generate_handler_protocol(service));
    out.push_str(&generate_dispatcher(service));
    out.push_str(&generate_streaming_dispatcher(service));
    out
}

/// Generate handler protocol (for handling incoming calls).
fn generate_handler_protocol(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let service_name = service.name.to_upper_camel_case();

    if let Some(doc) = &service.doc {
        out.push_str(&format_doc(doc, ""));
    }
    out.push_str(&format!("public protocol {service_name}Handler {{\n"));

    for method in &service.methods {
        let method_name = method.method_name.to_lower_camel_case();

        if let Some(doc) = &method.doc {
            out.push_str(&format_doc(doc, "    "));
        }

        // Server perspective
        let args: Vec<String> = method
            .args
            .iter()
            .map(|a| {
                format!(
                    "{}: {}",
                    a.name.to_lower_camel_case(),
                    swift_type_server_arg(a.ty)
                )
            })
            .collect();

        let ret_type = swift_type_server_return(method.return_type);

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

/// Generate dispatcher for handling incoming calls.
fn generate_dispatcher(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let mut w = CodeWriter::with_indent_spaces(&mut out, 4);
    let service_name = service.name.to_upper_camel_case();

    cw_writeln!(w, "public final class {service_name}Dispatcher {{").unwrap();
    {
        let _indent = w.indent();
        cw_writeln!(w, "private let handler: {service_name}Handler").unwrap();
        w.blank_line().unwrap();
        cw_writeln!(w, "public init(handler: {service_name}Handler) {{").unwrap();
        {
            let _indent = w.indent();
            w.writeln("self.handler = handler").unwrap();
        }
        w.writeln("}").unwrap();
        w.blank_line().unwrap();

        // Main dispatch method
        w.writeln("public func dispatch(methodId: UInt64, payload: Data) async throws -> Data {")
            .unwrap();
        {
            let _indent = w.indent();
            w.writeln("switch methodId {").unwrap();
            for method in &service.methods {
                let method_name = method.method_name.to_lower_camel_case();
                let method_id = crate::method_id(method);
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                cw_writeln!(
                    w,
                    "    return try await dispatch{method_name}(payload: payload)"
                )
                .unwrap();
            }
            w.writeln("default:").unwrap();
            w.writeln("    throw RoamError.unknownMethod").unwrap();
            w.writeln("}").unwrap();
        }
        w.writeln("}").unwrap();

        // Individual dispatch methods
        for method in &service.methods {
            w.blank_line().unwrap();
            generate_dispatch_method(&mut w, method);
        }
    }
    w.writeln("}").unwrap();
    w.blank_line().unwrap();

    out
}

/// Generate a single dispatch method for non-streaming dispatcher.
fn generate_dispatch_method(w: &mut CodeWriter<&mut String>, method: &MethodDetail) {
    let method_name = method.method_name.to_lower_camel_case();
    let has_streaming =
        method.args.iter().any(|a| is_stream(a.ty)) || is_stream(method.return_type);

    cw_writeln!(
        w,
        "private func dispatch{method_name}(payload: Data) async throws -> Data {{"
    )
    .unwrap();
    {
        let _indent = w.indent();

        if has_streaming {
            w.writeln("// TODO: Implement streaming dispatch").unwrap();
            w.writeln("throw RoamError.notImplemented").unwrap();
        } else {
            // Decode arguments
            generate_decode_args(w, &method.args);

            // Call handler
            let arg_names: Vec<String> = method
                .args
                .iter()
                .map(|a| {
                    let name = a.name.to_lower_camel_case();
                    format!("{name}: {name}")
                })
                .collect();

            let ret_type = swift_type_server_return(method.return_type);

            if ret_type == "Void" {
                cw_writeln!(
                    w,
                    "try await handler.{method_name}({})",
                    arg_names.join(", ")
                )
                .unwrap();
                w.writeln("return Data()").unwrap();
            } else {
                cw_writeln!(
                    w,
                    "let result = try await handler.{method_name}({})",
                    arg_names.join(", ")
                )
                .unwrap();
                let encode_closure = generate_encode_closure(method.return_type);
                cw_writeln!(
                    w,
                    "return Data(encodeResultOk(result, encoder: {encode_closure}))"
                )
                .unwrap();
            }
        }
    }
    w.writeln("}").unwrap();
}

/// Generate code to decode method arguments (for dispatcher).
fn generate_decode_args(w: &mut CodeWriter<&mut String>, args: &[roam_schema::ArgDetail]) {
    if args.is_empty() {
        w.writeln("// No arguments to decode").unwrap();
        return;
    }

    let cursor_var = unique_decode_cursor_name(args);
    cw_writeln!(w, "var {cursor_var} = 0").unwrap();
    for arg in args {
        let arg_name = arg.name.to_lower_camel_case();
        let decode_stmt = generate_decode_stmt_with_cursor(arg.ty, &arg_name, "", &cursor_var);
        for line in decode_stmt.lines() {
            w.writeln(line).unwrap();
        }
    }
}

/// Generate streaming dispatcher for handling incoming calls with channel support.
fn generate_streaming_dispatcher(service: &ServiceDetail) -> String {
    let mut out = String::new();
    let mut w = CodeWriter::with_indent_spaces(&mut out, 4);
    let service_name = service.name.to_upper_camel_case();

    cw_writeln!(w, "public final class {service_name}StreamingDispatcher {{").unwrap();
    {
        let _indent = w.indent();
        cw_writeln!(w, "private let handler: {service_name}Handler").unwrap();
        w.writeln("private let registry: IncomingChannelRegistry")
            .unwrap();
        w.writeln("private let taskSender: TaskSender").unwrap();
        w.blank_line().unwrap();

        cw_writeln!(
            w,
            "public init(handler: {service_name}Handler, registry: IncomingChannelRegistry, taskSender: @escaping TaskSender) {{"
        )
        .unwrap();
        {
            let _indent = w.indent();
            w.writeln("self.handler = handler").unwrap();
            w.writeln("self.registry = registry").unwrap();
            w.writeln("self.taskSender = taskSender").unwrap();
        }
        w.writeln("}").unwrap();
        w.blank_line().unwrap();

        // Main dispatch method
        w.writeln(
            "public func dispatch(methodId: UInt64, requestId: UInt64, payload: Data) async {",
        )
        .unwrap();
        {
            let _indent = w.indent();
            w.writeln("switch methodId {").unwrap();
            for method in &service.methods {
                let method_name = method.method_name.to_lower_camel_case();
                let method_id = crate::method_id(method);
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                cw_writeln!(
                    w,
                    "    await dispatch{method_name}(requestId: requestId, payload: payload)"
                )
                .unwrap();
            }
            w.writeln("default:").unwrap();
            w.writeln(
                "    taskSender(.response(requestId: requestId, payload: encodeUnknownMethodError()))",
            )
            .unwrap();
            w.writeln("}").unwrap();
        }
        w.writeln("}").unwrap();
        w.blank_line().unwrap();

        // Generate preregisterChannels method
        generate_preregister_channels(&mut w, service);
        w.blank_line().unwrap();

        // Individual dispatch methods
        for method in &service.methods {
            generate_streaming_dispatch_method(&mut w, method);
            w.blank_line().unwrap();
        }
    }
    w.writeln("}").unwrap();
    w.blank_line().unwrap();

    out
}

/// Generate preregisterChannels method.
fn generate_preregister_channels(w: &mut CodeWriter<&mut String>, service: &ServiceDetail) {
    w.writeln("/// Pre-register channel IDs from a request payload.")
        .unwrap();
    w.writeln("/// Call this synchronously before spawning the dispatch task to avoid")
        .unwrap();
    w.writeln("/// race conditions where Data arrives before channels are registered.")
        .unwrap();
    w.writeln("public static func preregisterChannels(methodId: UInt64, payload: Data, registry: ChannelRegistry) async {")
        .unwrap();
    {
        let _indent = w.indent();
        w.writeln("switch methodId {").unwrap();

        for method in &service.methods {
            let method_id = crate::method_id(method);
            let has_rx_args = method.args.iter().any(|a| is_rx(a.ty));

            if has_rx_args {
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                w.writeln("    do {").unwrap();
                let cursor_var = unique_decode_cursor_name(&method.args);
                cw_writeln!(w, "        var {cursor_var} = 0").unwrap();

                // Parse channel IDs and mark them as known
                for arg in &method.args {
                    let arg_name = arg.name.to_lower_camel_case();
                    if is_rx(arg.ty) {
                        // Schema Rx = client sends, server receives → need to preregister
                        cw_writeln!(
                            w,
                            "        let {arg_name}ChannelId = try decodeVarint(from: payload, offset: &{cursor_var})"
                        )
                        .unwrap();
                        cw_writeln!(w, "        await registry.markKnown({arg_name}ChannelId)")
                            .unwrap();
                    } else if is_tx(arg.ty) {
                        // Schema Tx = server sends → just skip the varint
                        cw_writeln!(
                            w,
                            "        _ = try decodeVarint(from: payload, offset: &{cursor_var}) // {arg_name}"
                        )
                        .unwrap();
                    } else {
                        // Non-streaming arg - skip it based on type
                        generate_skip_arg(w, &arg_name, arg.ty, "        ", &cursor_var);
                    }
                }

                w.writeln("    } catch {").unwrap();
                w.writeln("        // Ignore parse errors - dispatch will handle them")
                    .unwrap();
                w.writeln("    }").unwrap();
            }
        }

        w.writeln("default:").unwrap();
        w.writeln("    break").unwrap();
        w.writeln("}").unwrap();
    }
    w.writeln("}").unwrap();
}

/// Generate code to skip over an argument during preregistration.
fn generate_skip_arg(
    w: &mut CodeWriter<&mut String>,
    name: &str,
    shape: &'static Shape,
    indent: &str,
    cursor_var: &str,
) {
    use roam_schema::is_bytes;

    if is_bytes(shape) {
        cw_writeln!(
            w,
            "{indent}_ = try decodeBytes(from: payload, offset: &{cursor_var}) // {name}"
        )
        .unwrap();
        return;
    }

    match classify_shape(shape) {
        ShapeKind::Scalar(scalar) => {
            let skip_code = match scalar {
                ScalarType::Bool | ScalarType::U8 | ScalarType::I8 => {
                    format!("{cursor_var} += 1")
                }
                ScalarType::U16 | ScalarType::I16 => format!("{cursor_var} += 2"),
                ScalarType::U32 | ScalarType::I32 | ScalarType::U64 | ScalarType::I64 => {
                    format!("_ = try decodeVarint(from: payload, offset: &{cursor_var})")
                }
                ScalarType::F32 => format!("{cursor_var} += 4"),
                ScalarType::F64 => format!("{cursor_var} += 8"),
                ScalarType::Unit => String::new(),
                ScalarType::Char => {
                    format!("_ = try decodeVarint(from: payload, offset: &{cursor_var})")
                }
                _ => String::from("// unknown scalar type"),
            };
            if !skip_code.is_empty() {
                cw_writeln!(w, "{indent}{skip_code} // {name}").unwrap();
            }
        }
        ShapeKind::List { .. } | ShapeKind::Slice { .. } | ShapeKind::Array { .. } => {
            cw_writeln!(
                w,
                "{indent}_ = try decodeBytes(from: payload, offset: &{cursor_var}) // {name} (skipped)"
            )
            .unwrap();
        }
        ShapeKind::Option { .. } => {
            cw_writeln!(w, "{indent}// TODO: skip option {name}").unwrap();
        }
        ShapeKind::Struct(StructInfo { fields, .. }) => {
            // For structs, recursively skip each field
            for field in fields {
                let field_name = format!("{}.{}", name, field.name);
                generate_skip_arg(w, &field_name, field.shape(), indent, cursor_var);
            }
        }
        _ => {
            cw_writeln!(w, "{indent}// TODO: skip {name}").unwrap();
        }
    }
}

/// Generate a single streaming dispatch method.
fn generate_streaming_dispatch_method(w: &mut CodeWriter<&mut String>, method: &MethodDetail) {
    let method_name = method.method_name.to_lower_camel_case();
    let has_streaming =
        method.args.iter().any(|a| is_stream(a.ty)) || is_stream(method.return_type);

    cw_writeln!(
        w,
        "private func dispatch{method_name}(requestId: UInt64, payload: Data) async {{"
    )
    .unwrap();
    {
        let _indent = w.indent();
        w.writeln("do {").unwrap();
        {
            let _indent = w.indent();
            let cursor_var = unique_decode_cursor_name(&method.args);
            cw_writeln!(w, "var {cursor_var} = 0").unwrap();

            // Decode arguments - for streaming, decode channel IDs and create Tx/Rx
            for arg in &method.args {
                let arg_name = arg.name.to_lower_camel_case();
                generate_streaming_decode_arg(w, &arg_name, arg.ty, &cursor_var);
            }

            // Call handler
            let arg_names: Vec<String> = method
                .args
                .iter()
                .map(|a| {
                    let name = a.name.to_lower_camel_case();
                    format!("{name}: {name}")
                })
                .collect();

            let ret_type = swift_type_server_return(method.return_type);

            if has_streaming {
                // For streaming methods, close any Tx channels after handler completes
                if ret_type == "Void" {
                    cw_writeln!(
                        w,
                        "try await handler.{method_name}({})",
                        arg_names.join(", ")
                    )
                    .unwrap();
                } else {
                    cw_writeln!(
                        w,
                        "let result = try await handler.{method_name}({})",
                        arg_names.join(", ")
                    )
                    .unwrap();
                }

                // Close any Tx channels
                for arg in &method.args {
                    if is_tx(arg.ty) {
                        let arg_name = arg.name.to_lower_camel_case();
                        cw_writeln!(w, "{arg_name}.close()").unwrap();
                    }
                }

                // Send response
                if ret_type == "Void" {
                    w.writeln("taskSender(.response(requestId: requestId, payload: encodeResultOk((), encoder: { _ in [] })))").unwrap();
                } else {
                    let encode_closure = generate_encode_closure(method.return_type);
                    cw_writeln!(
                        w,
                        "taskSender(.response(requestId: requestId, payload: encodeResultOk(result, encoder: {encode_closure})))"
                    )
                    .unwrap();
                }
            } else {
                // Non-streaming method
                if ret_type == "Void" {
                    cw_writeln!(
                        w,
                        "try await handler.{method_name}({})",
                        arg_names.join(", ")
                    )
                    .unwrap();
                    w.writeln("taskSender(.response(requestId: requestId, payload: encodeResultOk((), encoder: { _ in [] })))").unwrap();
                } else {
                    cw_writeln!(
                        w,
                        "let result = try await handler.{method_name}({})",
                        arg_names.join(", ")
                    )
                    .unwrap();
                    // Check if return type is Result<T, E> - if so, encode as Result<T, RoamError<User(E)>>
                    if let ShapeKind::Result { ok, err } = classify_shape(method.return_type) {
                        let ok_encode = generate_encode_closure(ok);
                        let err_encode = generate_encode_closure(err);
                        // Wire format: [0] + T for success, [1, 0] + E for User error
                        cw_writeln!(
                            w,
                            "taskSender(.response(requestId: requestId, payload: {{ switch result {{ case .success(let v): return [UInt8(0)] + {ok_encode}(v); case .failure(let e): return [UInt8(1), UInt8(0)] + {err_encode}(e) }} }}()))"
                        )
                        .unwrap();
                    } else {
                        let encode_closure = generate_encode_closure(method.return_type);
                        cw_writeln!(
                            w,
                            "taskSender(.response(requestId: requestId, payload: encodeResultOk(result, encoder: {encode_closure})))"
                        )
                        .unwrap();
                    }
                }
            }
        }
        w.writeln("} catch {").unwrap();
        {
            let _indent = w.indent();
            w.writeln(
                "taskSender(.response(requestId: requestId, payload: encodeInvalidPayloadError()))",
            )
            .unwrap();
        }
        w.writeln("}").unwrap();
    }
    w.writeln("}").unwrap();
}

/// Generate code to decode a single argument for streaming dispatch.
fn generate_streaming_decode_arg(
    w: &mut CodeWriter<&mut String>,
    name: &str,
    shape: &'static Shape,
    cursor_var: &str,
) {
    match classify_shape(shape) {
        ShapeKind::Rx { inner } => {
            // Schema Rx = client passes Rx to method, sends via paired Tx
            // Server needs to receive → create server Rx
            let inline_decode = generate_inline_decode(inner, "Data(bytes)", "off");
            cw_writeln!(
                w,
                "let {name}ChannelId = try decodeVarint(from: payload, offset: &{cursor_var})"
            )
            .unwrap();
            cw_writeln!(
                w,
                "let {name}Receiver = await registry.register({name}ChannelId)"
            )
            .unwrap();
            cw_writeln!(
                w,
                "let {name} = createServerRx(channelId: {name}ChannelId, receiver: {name}Receiver, deserialize: {{ bytes in"
            )
            .unwrap();
            cw_writeln!(w, "    var off = 0").unwrap();
            cw_writeln!(w, "    return try {inline_decode}").unwrap();
            w.writeln("})").unwrap();
        }
        ShapeKind::Tx { inner } => {
            // Schema Tx = client passes Tx to method, receives via paired Rx
            // Server needs to send → create server Tx
            let encode_closure = generate_encode_closure(inner);
            cw_writeln!(
                w,
                "let {name}ChannelId = try decodeVarint(from: payload, offset: &{cursor_var})"
            )
            .unwrap();
            cw_writeln!(
                w,
                "let {name} = createServerTx(channelId: {name}ChannelId, taskSender: taskSender, serialize: ({encode_closure}))"
            )
            .unwrap();
        }
        _ => {
            // Non-streaming argument - use standard decode
            let decode_stmt = generate_decode_stmt_with_cursor(shape, name, "", cursor_var);
            for line in decode_stmt.lines() {
                w.writeln(line).unwrap();
            }
        }
    }
}

fn unique_decode_cursor_name(args: &[roam_schema::ArgDetail]) -> String {
    let arg_names: Vec<String> = args.iter().map(|a| a.name.to_lower_camel_case()).collect();
    let mut candidate = String::from("cursor");
    while arg_names.iter().any(|name| name == &candidate) {
        candidate.push('_');
    }
    candidate
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

    fn offset_arg_service() -> ServiceDetail {
        ServiceDetail {
            name: Cow::Borrowed("Vfs"),
            doc: None,
            methods: vec![MethodDetail {
                service_name: Cow::Borrowed("Vfs"),
                method_name: Cow::Borrowed("read"),
                args: vec![
                    ArgDetail {
                        name: Cow::Borrowed("item_id"),
                        ty: <u64 as Facet>::SHAPE,
                    },
                    ArgDetail {
                        name: Cow::Borrowed("offset"),
                        ty: <u64 as Facet>::SHAPE,
                    },
                    ArgDetail {
                        name: Cow::Borrowed("len"),
                        ty: <u64 as Facet>::SHAPE,
                    },
                ],
                return_type: <u64 as Facet>::SHAPE,
                doc: None,
            }],
        }
    }

    fn cursor_arg_service() -> ServiceDetail {
        ServiceDetail {
            name: Cow::Borrowed("Vfs"),
            doc: None,
            methods: vec![MethodDetail {
                service_name: Cow::Borrowed("Vfs"),
                method_name: Cow::Borrowed("read"),
                args: vec![
                    ArgDetail {
                        name: Cow::Borrowed("cursor"),
                        ty: <u64 as Facet>::SHAPE,
                    },
                    ArgDetail {
                        name: Cow::Borrowed("cursor_"),
                        ty: <u64 as Facet>::SHAPE,
                    },
                ],
                return_type: <u64 as Facet>::SHAPE,
                doc: None,
            }],
        }
    }

    #[test]
    fn test_generate_handler_protocol() {
        let service = sample_service();
        let code = generate_handler_protocol(&service);

        assert!(code.contains("protocol EchoHandler"));
        assert!(code.contains("func echo(message: String)"));
        assert!(code.contains("async throws -> String"));
    }

    #[test]
    fn test_generate_dispatcher() {
        let service = sample_service();
        let code = generate_dispatcher(&service);

        assert!(code.contains("class EchoDispatcher"));
        assert!(code.contains("EchoHandler"));
        assert!(code.contains("dispatch(methodId:"));
        assert!(code.contains("dispatchecho"));
    }

    #[test]
    fn test_generate_streaming_dispatcher() {
        let service = sample_service();
        let code = generate_streaming_dispatcher(&service);

        assert!(code.contains("class EchoStreamingDispatcher"));
        assert!(code.contains("preregisterChannels"));
        assert!(code.contains("IncomingChannelRegistry"));
    }

    #[test]
    fn test_dispatcher_uses_cursor_for_offset_argument() {
        let code = generate_dispatcher(&offset_arg_service());

        assert!(code.contains("var cursor = 0"));
        assert!(code.contains("let offset = try decodeVarint(from: payload, offset: &cursor)"));
        assert!(!code.contains("var offset = 0"));
    }

    #[test]
    fn test_dispatcher_chooses_unique_cursor_variable_name() {
        let code = generate_dispatcher(&cursor_arg_service());

        assert!(code.contains("var cursor_ = 0") || code.contains("var cursor__ = 0"));
        assert!(
            code.contains("let cursor = try decodeVarint(from: payload, offset: &cursor_)")
                || code.contains("let cursor = try decodeVarint(from: payload, offset: &cursor__)")
        );
        assert!(!code.contains("var cursor = 0"));
    }
}
