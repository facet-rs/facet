//! Swift server/handler generation.
//!
//! Generates handler protocol and dispatcher for routing incoming calls.

use facet_core::Shape;
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use vox_types::{MethodDescriptor, ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use super::decode::generate_decode_stmt_with_cursor;
use super::encode::{generate_encode_closure, generate_encode_stmt};
use super::types::{format_doc, is_channel, swift_type_server_arg, swift_type_server_return};
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

fn dispatch_helper_name(method_name: &str) -> String {
    format!("dispatch_{method_name}")
}

/// Generate complete server code (handler protocol + dispatchers).
pub fn generate_server(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    out.push_str(&generate_handler_protocol(service));
    // Emit only the channel-capable dispatcher.
    out.push_str(&generate_dispatcher(service));
    out
}

/// Generate handler protocol (for handling incoming calls).
fn generate_handler_protocol(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();

    if let Some(doc) = &service.doc {
        out.push_str(&format_doc(doc, ""));
    }
    out.push_str(&format!(
        "public protocol {service_name}Handler: Sendable {{\n"
    ));

    for method in service.methods {
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
                    swift_type_server_arg(a.shape)
                )
            })
            .collect();

        let ret_type = swift_type_server_return(method.return_shape);

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

/// Generate dispatcher for handling incoming calls with channel support.
fn generate_dispatcher(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let mut w = CodeWriter::with_indent_spaces(&mut out, 4);
    let service_name = service.service_name.to_upper_camel_case();

    let service_name_lower = service.service_name.to_lower_camel_case();

    cw_writeln!(
        w,
        "public final class {service_name}Dispatcher: ServiceDispatcher {{"
    )
    .unwrap();
    {
        let _indent = w.indent();
        cw_writeln!(w, "private let handler: {service_name}Handler").unwrap();
        cw_writeln!(w, "private let schemaRegistry: [UInt64: Schema]").unwrap();
        cw_writeln!(w, "private let methodSchemas: [UInt64: MethodSchemaInfo]").unwrap();
        w.blank_line().unwrap();

        cw_writeln!(
            w,
            "public init(handler: {service_name}Handler, schemaRegistry: [UInt64: Schema] = {service_name_lower}_schema_registry, methodSchemas: [UInt64: MethodSchemaInfo] = {service_name_lower}_method_schemas) {{"
        )
        .unwrap();
        {
            let _indent = w.indent();
            w.writeln("self.handler = handler").unwrap();
            w.writeln("self.schemaRegistry = schemaRegistry").unwrap();
            w.writeln("self.methodSchemas = methodSchemas").unwrap();
        }
        w.writeln("}").unwrap();
        w.blank_line().unwrap();

        // Main dispatch method matching ServiceDispatcher protocol
        w.writeln(
            "public func dispatch(methodId: UInt64, payload: [UInt8], requestId: UInt64, registry: ChannelRegistry, schemaSendTracker _: SchemaSendTracker, taskTx: @escaping @Sendable (TaskMessage) -> Void) async {",
        )
        .unwrap();
        {
            let _indent = w.indent();
            w.writeln("var buffer = ByteBufferAllocator().buffer(capacity: payload.count)")
                .unwrap();
            w.writeln("buffer.writeBytes(payload)").unwrap();
            w.writeln("let taskSender: TaskSender = taskTx").unwrap();
            w.writeln("switch methodId {").unwrap();
            for method in service.methods {
                let method_name = method.method_name.to_lower_camel_case();
                let method_id = crate::method_id(method);
                let dispatch_name = dispatch_helper_name(&method_name);
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                cw_writeln!(
                    w,
                    "    await {dispatch_name}(methodId: methodId, requestId: requestId, buffer: &buffer, registry: registry, taskSender: taskSender)"
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

        w.writeln("public func retryPolicy(methodId: UInt64) -> RetryPolicy {")
            .unwrap();
        {
            let _indent = w.indent();
            w.writeln("switch methodId {").unwrap();
            for method in service.methods {
                let method_id = crate::method_id(method);
                let retry_policy = swift_retry_policy_literal(method);
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                cw_writeln!(w, "    return {retry_policy}").unwrap();
            }
            w.writeln("default:").unwrap();
            w.writeln("    return .volatile").unwrap();
            w.writeln("}").unwrap();
        }
        w.writeln("}").unwrap();
        w.blank_line().unwrap();

        // Generate preregisterChannels method
        generate_preregister_channels(&mut w, service);
        w.blank_line().unwrap();

        // Individual dispatch methods
        for method in service.methods {
            generate_channeling_dispatch_method(&mut w, method);
            w.blank_line().unwrap();
        }
    }
    w.writeln("}").unwrap();
    w.blank_line().unwrap();

    out
}

/// Generate preregisterChannels method.
fn generate_preregister_channels(w: &mut CodeWriter<&mut String>, service: &ServiceDescriptor) {
    w.writeln("/// Pre-register Rx channel IDs from request payloads.")
        .unwrap();
    w.writeln("/// Call this synchronously before spawning the dispatch task to avoid")
        .unwrap();
    w.writeln("/// race conditions where Data arrives before channels are registered.")
        .unwrap();
    w.writeln(
        "public func preregister(methodId: UInt64, payload: [UInt8], registry: ChannelRegistry) async {",
    )
        .unwrap();
    {
        let _indent = w.indent();
        w.writeln("var buffer = ByteBufferAllocator().buffer(capacity: payload.count)")
            .unwrap();
        w.writeln("buffer.writeBytes(payload)").unwrap();
        w.writeln("switch methodId {").unwrap();

        for method in service.methods {
            let method_id = crate::method_id(method);
            let has_channel_args = method.args.iter().any(|a| is_rx(a.shape) || is_tx(a.shape));

            if has_channel_args {
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                w.writeln("    do {").unwrap();
                {
                    let _indent = w.indent();
                    for arg in method.args {
                        let arg_name = arg.name.to_lower_camel_case();
                        if is_rx(arg.shape) {
                            cw_writeln!(
                                w,
                                "let {arg_name}ChannelId = try decodeVarint(from: &buffer)"
                            )
                            .unwrap();
                            cw_writeln!(w, "await registry.markKnown({arg_name}ChannelId)")
                                .unwrap();
                        } else if is_tx(arg.shape) {
                            w.writeln("_ = try decodeVarint(from: &buffer)").unwrap();
                        } else {
                            let discard_name = format!("_discard_{arg_name}");
                            let decode_stmt = generate_decode_stmt_with_cursor(
                                arg.shape,
                                &discard_name,
                                "",
                                "buffer",
                            );
                            for line in decode_stmt.lines() {
                                w.writeln(line).unwrap();
                            }
                        }
                    }
                }
                w.writeln("    } catch {").unwrap();
                w.writeln("        return").unwrap();
                w.writeln("    }").unwrap();
            }
        }

        w.writeln("default:").unwrap();
        w.writeln("    break").unwrap();
        w.writeln("}").unwrap();
    }
    w.writeln("}").unwrap();
}

/// Generate a single dispatch method.
fn generate_channeling_dispatch_method(w: &mut CodeWriter<&mut String>, method: &MethodDescriptor) {
    let method_name = method.method_name.to_lower_camel_case();
    let dispatch_name = dispatch_helper_name(&method_name);
    let has_channeling = method.args.iter().any(|a| is_channel(a.shape));
    let handler_error_payload = if method.retry.persist {
        "encodeIndeterminateError()"
    } else {
        "encodeInvalidPayloadError()"
    };

    cw_writeln!(
        w,
        "private func {dispatch_name}(methodId: UInt64, requestId: UInt64, buffer: inout ByteBuffer, registry: IncomingChannelRegistry, taskSender: @escaping TaskSender) async {{"
    )
    .unwrap();
    {
        let _indent = w.indent();
        // Build response schema payload for this method.
        w.writeln("guard let methodInfo = methodSchemas[methodId] else {")
            .unwrap();
        w.writeln(
            "    taskSender(.response(requestId: requestId, payload: encodeUnknownMethodError()))",
        )
        .unwrap();
        w.writeln("    return").unwrap();
        w.writeln("}").unwrap();
        w.writeln(
            "let responseSchemaPayload = methodInfo.buildPayload(direction: .response, registry: schemaRegistry)",
        )
            .unwrap();
        w.writeln("do {").unwrap();
        {
            let _indent = w.indent();
            for arg in method.args {
                let arg_name = arg.name.to_lower_camel_case();
                generate_channeling_decode_arg(w, &arg_name, arg.shape);
            }
            let arg_names: Vec<String> = method
                .args
                .iter()
                .map(|a| {
                    let name = a.name.to_lower_camel_case();
                    format!("{name}: {name}")
                })
                .collect();

            let ret_type = swift_type_server_return(method.return_shape);

            w.writeln("do {").unwrap();
            {
                let _indent = w.indent();
                if has_channeling {
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

                    for arg in method.args {
                        if is_tx(arg.shape) {
                            let arg_name = arg.name.to_lower_camel_case();
                            cw_writeln!(w, "{arg_name}.close()").unwrap();
                        }
                    }

                    if ret_type == "Void" {
                        w.writeln(
                            "taskSender(.response(requestId: requestId, payload: encodeResultOkUnit(), methodId: methodId, schemaPayload: responseSchemaPayload))",
                        )
                        .unwrap();
                    } else {
                        let encode_closure = generate_encode_closure(method.return_shape);
                        cw_writeln!(
                            w,
                            "let _encoded = encodeResultOk(result, encoder: {encode_closure})"
                        )
                        .unwrap();
                        w.writeln(
                            "taskSender(.response(requestId: requestId, payload: _encoded, methodId: methodId, schemaPayload: responseSchemaPayload))",
                        )
                        .unwrap();
                    }
                } else if ret_type == "Void" {
                    cw_writeln!(
                        w,
                        "try await handler.{method_name}({})",
                        arg_names.join(", ")
                    )
                    .unwrap();
                    w.writeln(
                        "taskSender(.response(requestId: requestId, payload: encodeResultOkUnit(), methodId: methodId, schemaPayload: responseSchemaPayload))",
                    )
                    .unwrap();
                } else {
                    cw_writeln!(
                        w,
                        "let result = try await handler.{method_name}({})",
                        arg_names.join(", ")
                    )
                    .unwrap();
                    if let ShapeKind::Result { ok, err } = classify_shape(method.return_shape) {
                        let ok_encode = generate_encode_closure(ok);
                        let err_encode = generate_encode_closure(err);
                        cw_writeln!(
                            w,
                            "let _encoded: [UInt8] = {{ var buf = ByteBufferAllocator().buffer(capacity: 64); switch result {{ case .success(let v): encodeVarint(UInt64(0), into: &buf); {ok_encode}(v, &buf); case .failure(let e): encodeVarint(UInt64(1), into: &buf); encodeU8(0, into: &buf); {err_encode}(e, &buf) }}; return buf.readBytes(length: buf.readableBytes) ?? [] }}()"
                        )
                        .unwrap();
                        w.writeln(
                            "taskSender(.response(requestId: requestId, payload: _encoded, methodId: methodId, schemaPayload: responseSchemaPayload))",
                        )
                        .unwrap();
                    } else {
                        let encode_closure = generate_encode_closure(method.return_shape);
                        cw_writeln!(
                            w,
                            "let _encoded = encodeResultOk(result, encoder: {encode_closure})"
                        )
                        .unwrap();
                        w.writeln(
                            "taskSender(.response(requestId: requestId, payload: _encoded, methodId: methodId, schemaPayload: responseSchemaPayload))",
                        )
                        .unwrap();
                    }
                }
            }
            w.writeln("} catch {").unwrap();
            {
                let _indent = w.indent();
                cw_writeln!(
                    w,
                    "taskSender(.response(requestId: requestId, payload: {handler_error_payload}, methodId: methodId, schemaPayload: responseSchemaPayload))"
                )
                .unwrap();
            }
            w.writeln("}").unwrap();
        }
        w.writeln("} catch {").unwrap();
        {
            let _indent = w.indent();
            w.writeln(
                "taskSender(.response(requestId: requestId, payload: encodeInvalidPayloadError(), methodId: methodId, schemaPayload: responseSchemaPayload))",
            )
            .unwrap();
        }
        w.writeln("}").unwrap();
    }
    w.writeln("}").unwrap();
}

/// Generate code to decode a single argument for dispatch.
/// All decodes read from `buffer: inout ByteBuffer` in scope.
fn generate_channeling_decode_arg(
    w: &mut CodeWriter<&mut String>,
    name: &str,
    shape: &'static Shape,
) {
    match classify_shape(shape) {
        ShapeKind::Rx { inner } => {
            let decode_closure = generate_decode_closure_for_channel(inner);
            cw_writeln!(w, "let {name}ChannelId = try decodeVarint(from: &buffer)").unwrap();
            cw_writeln!(
                w,
                "let {name}Receiver = await registry.register({name}ChannelId, initialCredit: 16, onConsumed: {{ [taskSender] additional in taskSender(.grantCredit(channelId: {name}ChannelId, bytes: additional)) }})"
            )
            .unwrap();
            cw_writeln!(
                w,
                "let {name} = createServerRx(channelId: {name}ChannelId, receiver: {name}Receiver, deserialize: {decode_closure})"
            )
            .unwrap();
        }
        ShapeKind::Tx { inner } => {
            let encode_closure = generate_encode_closure(inner);
            cw_writeln!(w, "let {name}ChannelId = try decodeVarint(from: &buffer)").unwrap();
            cw_writeln!(
                w,
                "let {name} = await createServerTx(channelId: {name}ChannelId, taskSender: taskSender, registry: registry, initialCredit: 16, serialize: {encode_closure})"
            )
            .unwrap();
        }
        _ => {
            let decode_stmt = generate_decode_stmt_with_cursor(shape, name, "", "buffer");
            for line in decode_stmt.lines() {
                w.writeln(line).unwrap();
            }
        }
    }
}

/// Generate a deserialize closure for use with createServerRx.
/// The closure takes `inout ByteBuffer` and returns the decoded value.
fn generate_decode_closure_for_channel(inner: &'static Shape) -> String {
    use super::decode::generate_decode_closure;
    generate_decode_closure(inner)
}

fn unique_decode_cursor_name(_args: &[vox_types::ArgDescriptor]) -> String {
    // kept for any remaining call sites; buffer is always named `buffer` now
    let arg_names: Vec<String> = _args.iter().map(|a| a.name.to_lower_camel_case()).collect();
    let mut candidate = String::from("cursor");
    while arg_names.iter().any(|name| name == &candidate) {
        candidate.push('_');
    }
    candidate
}
