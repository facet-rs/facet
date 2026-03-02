//! Swift server/handler generation.
//!
//! Generates handler protocol and dispatcher for routing incoming calls.

use facet_core::Shape;
use heck::{ToLowerCamelCase, ToUpperCamelCase};
use roam_types::{MethodDescriptor, ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use super::decode::{generate_decode_stmt_with_cursor, generate_inline_decode};
use super::encode::generate_encode_closure;
use super::types::{format_doc, is_channel, swift_type_server_arg, swift_type_server_return};
use crate::code_writer::CodeWriter;
use crate::cw_writeln;
use crate::render::hex_u64;

fn dispatch_helper_name(method_name: &str) -> String {
    format!("dispatch_{method_name}")
}

/// Generate complete server code (handler protocol + dispatchers).
pub fn generate_server(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    out.push_str(&generate_handler_protocol(service));
    // Emit only the channel-capable dispatcher.
    out.push_str(&generate_channeling_dispatcher(service));
    out
}

/// Generate handler protocol (for handling incoming calls).
fn generate_handler_protocol(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let service_name = service.service_name.to_upper_camel_case();

    if let Some(doc) = &service.doc {
        out.push_str(&format_doc(doc, ""));
    }
    out.push_str(&format!("public protocol {service_name}Handler {{\n"));

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

/// Generate channeling dispatcher for handling incoming calls with channel support.
fn generate_channeling_dispatcher(service: &ServiceDescriptor) -> String {
    let mut out = String::new();
    let mut w = CodeWriter::with_indent_spaces(&mut out, 4);
    let service_name = service.service_name.to_upper_camel_case();

    cw_writeln!(
        w,
        "public final class {service_name}ChannelingDispatcher {{"
    )
    .unwrap();
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
            "public func dispatch(methodId: UInt64, requestId: UInt64, channels: [UInt64], payload: Data) async {",
        )
        .unwrap();
        {
            let _indent = w.indent();
            w.writeln("switch methodId {").unwrap();
            for method in service.methods {
                let method_name = method.method_name.to_lower_camel_case();
                let method_id = crate::method_id(method);
                let dispatch_name = dispatch_helper_name(&method_name);
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                cw_writeln!(
                    w,
                    "    await {dispatch_name}(requestId: requestId, channels: channels, payload: payload)"
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
    w.writeln("/// Pre-register Rx channel IDs from request channels.")
        .unwrap();
    w.writeln("/// Call this synchronously before spawning the dispatch task to avoid")
        .unwrap();
    w.writeln("/// race conditions where Data arrives before channels are registered.")
        .unwrap();
    w.writeln("public static func preregisterChannels(methodId: UInt64, channels: [UInt64], registry: ChannelRegistry) async {")
        .unwrap();
    {
        let _indent = w.indent();
        w.writeln("switch methodId {").unwrap();

        for method in service.methods {
            let method_id = crate::method_id(method);
            let has_rx_args = method.args.iter().any(|a| is_rx(a.shape));

            if has_rx_args {
                let channel_arg_count = method
                    .args
                    .iter()
                    .filter(|a| is_rx(a.shape) || is_tx(a.shape))
                    .count();
                cw_writeln!(w, "case {}:", hex_u64(method_id)).unwrap();
                cw_writeln!(w, "    guard channels.count >= {channel_arg_count} else {{").unwrap();
                w.writeln("        return").unwrap();
                w.writeln("    }").unwrap();
                w.writeln("    var channelCursor = 0").unwrap();

                // Channel IDs are provided in declaration order.
                for arg in method.args {
                    let arg_name = arg.name.to_lower_camel_case();
                    if is_rx(arg.shape) {
                        // Schema Rx = client sends, server receives → need to preregister
                        cw_writeln!(w, "    let {arg_name}ChannelId = channels[channelCursor]")
                            .unwrap();
                        w.writeln("    channelCursor += 1").unwrap();
                        cw_writeln!(w, "    await registry.markKnown({arg_name}ChannelId)")
                            .unwrap();
                    } else if is_tx(arg.shape) {
                        cw_writeln!(w, "    _ = channels[channelCursor] // {arg_name}").unwrap();
                        w.writeln("    channelCursor += 1").unwrap();
                    }
                }
            }
        }

        w.writeln("default:").unwrap();
        w.writeln("    break").unwrap();
        w.writeln("}").unwrap();
    }
    w.writeln("}").unwrap();
}

/// Generate a single channeling dispatch method.
fn generate_channeling_dispatch_method(w: &mut CodeWriter<&mut String>, method: &MethodDescriptor) {
    let method_name = method.method_name.to_lower_camel_case();
    let dispatch_name = dispatch_helper_name(&method_name);
    let has_channeling = method.args.iter().any(|a| is_channel(a.shape));

    cw_writeln!(
        w,
        "private func {dispatch_name}(requestId: UInt64, channels: [UInt64], payload: Data) async {{"
    )
    .unwrap();
    {
        let _indent = w.indent();
        w.writeln("do {").unwrap();
        {
            let _indent = w.indent();
            let has_payload_args = method
                .args
                .iter()
                .any(|a| !is_rx(a.shape) && !is_tx(a.shape));
            let has_channel_args = method.args.iter().any(|a| is_rx(a.shape) || is_tx(a.shape));
            let cursor_var = if has_payload_args {
                let name = unique_decode_cursor_name(method.args);
                cw_writeln!(w, "var {name} = 0").unwrap();
                Some(name)
            } else {
                None
            };
            if has_channel_args {
                w.writeln("var channelCursor = 0").unwrap();
            }

            // Decode arguments - channel IDs come from Request.channels.
            for arg in method.args {
                let arg_name = arg.name.to_lower_camel_case();
                generate_channeling_decode_arg(
                    w,
                    &arg_name,
                    arg.shape,
                    cursor_var.as_deref(),
                    "channels",
                    Some("channelCursor"),
                );
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

            let ret_type = swift_type_server_return(method.return_shape);

            if has_channeling {
                // For channeling methods, close any Tx channels after handler completes
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
                for arg in method.args {
                    if is_tx(arg.shape) {
                        let arg_name = arg.name.to_lower_camel_case();
                        cw_writeln!(w, "{arg_name}.close()").unwrap();
                    }
                }

                // Send response
                if ret_type == "Void" {
                    w.writeln("taskSender(.response(requestId: requestId, payload: encodeResultOk((), encoder: { _ in [] })))").unwrap();
                } else {
                    let encode_closure = generate_encode_closure(method.return_shape);
                    cw_writeln!(
                        w,
                        "taskSender(.response(requestId: requestId, payload: encodeResultOk(result, encoder: {encode_closure})))"
                    )
                    .unwrap();
                }
            } else {
                // Non-channeling method
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
                    if let ShapeKind::Result { ok, err } = classify_shape(method.return_shape) {
                        let ok_encode = generate_encode_closure(ok);
                        let err_encode = generate_encode_closure(err);
                        // Wire format: [0] + T for success, [1, 0] + E for User error
                        cw_writeln!(
                            w,
                            "taskSender(.response(requestId: requestId, payload: {{ switch result {{ case .success(let v): return [UInt8(0)] + {ok_encode}(v); case .failure(let e): return [UInt8(1), UInt8(0)] + {err_encode}(e) }} }}()))"
                        )
                        .unwrap();
                    } else {
                        let encode_closure = generate_encode_closure(method.return_shape);
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

/// Generate code to decode a single argument for channeling dispatch.
fn generate_channeling_decode_arg(
    w: &mut CodeWriter<&mut String>,
    name: &str,
    shape: &'static Shape,
    cursor_var: Option<&str>,
    channels_var: &str,
    channel_cursor_var: Option<&str>,
) {
    match classify_shape(shape) {
        ShapeKind::Rx { inner } => {
            // Schema Rx = client passes Rx to method, sends via paired Tx
            // Server needs to receive → create server Rx
            let inline_decode = generate_inline_decode(inner, "Data(bytes)", "off");
            let channel_cursor_var =
                channel_cursor_var.expect("channel cursor required for channeling args");
            cw_writeln!(
                w,
                "guard {channel_cursor_var} < {channels_var}.count else {{ throw RoamError.decodeError(\"missing channel id for {name}\") }}"
            )
            .unwrap();
            cw_writeln!(
                w,
                "let {name}ChannelId = {channels_var}[{channel_cursor_var}]"
            )
            .unwrap();
            cw_writeln!(w, "{channel_cursor_var} += 1").unwrap();
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
            let channel_cursor_var =
                channel_cursor_var.expect("channel cursor required for channeling args");
            cw_writeln!(
                w,
                "guard {channel_cursor_var} < {channels_var}.count else {{ throw RoamError.decodeError(\"missing channel id for {name}\") }}"
            )
            .unwrap();
            cw_writeln!(
                w,
                "let {name}ChannelId = {channels_var}[{channel_cursor_var}]"
            )
            .unwrap();
            cw_writeln!(w, "{channel_cursor_var} += 1").unwrap();
            cw_writeln!(
                w,
                "let {name} = createServerTx(channelId: {name}ChannelId, taskSender: taskSender, serialize: ({encode_closure}))"
            )
            .unwrap();
        }
        _ => {
            // Non-channeling argument - use standard decode
            let cursor_var = cursor_var.expect("payload cursor required for non-channel args");
            let decode_stmt = generate_decode_stmt_with_cursor(shape, name, "", cursor_var);
            for line in decode_stmt.lines() {
                w.writeln(line).unwrap();
            }
        }
    }
}

fn unique_decode_cursor_name(args: &[roam_types::ArgDescriptor]) -> String {
    let arg_names: Vec<String> = args.iter().map(|a| a.name.to_lower_camel_case()).collect();
    let mut candidate = String::from("cursor");
    while arg_names.iter().any(|name| name == &candidate) {
        candidate.push('_');
    }
    candidate
}
