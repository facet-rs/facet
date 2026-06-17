//! Swift phon server emitter: the `{Service}Handler` protocol (the user implements
//! it) + a `{Service}Dispatcher: ServiceDispatcher` that decodes args via the typed
//! path (reconciling the peer's writer schema), calls the handler, wraps the result
//! into `Result<T, VoxError<E>>`, encodes + advertises it, and replies. Replaces the
//! postcard `server.rs`.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use vox_types::{MethodDescriptor, ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use super::phon_service::{
    element_auxiliary_decode_closure, element_encode_closure, method_global_prefix,
};
use super::types::{format_doc, swift_type_base, swift_type_server_arg, swift_type_server_return};
use crate::render::hex_u64;

fn has_channels(method: &MethodDescriptor) -> bool {
    method.args.iter().any(|a| is_tx(a.shape) || is_rx(a.shape))
}

/// The server-side (handler) Swift type of an argument — channel args become
/// `Tx`/`Rx` over the `channel_element` (the arg `shape` is an opaque adapter).
fn server_arg_ty(a: &vox_types::ArgDescriptor) -> String {
    if is_tx(a.shape) {
        format!(
            "Tx<{}>",
            swift_type_base(a.channel_element.expect("tx element"))
        )
    } else if is_rx(a.shape) {
        format!(
            "Rx<{}>",
            swift_type_base(a.channel_element.expect("rx element"))
        )
    } else {
        swift_type_server_arg(a.shape)
    }
}

/// `(ret_ty, response_wire_ty, user_error_ty?)`. `user_error_ty` is `Some` only for a
/// fallible method (`Result<T, E>` return) — its `E` is the handler's thrown error.
fn method_types(method: &MethodDescriptor) -> (String, String, Option<String>) {
    let ret = swift_type_server_return(method.return_shape);
    let resp = swift_type_base(method.response_wire_shape);
    let user_err = match classify_shape(method.return_shape) {
        ShapeKind::Result { err, .. } => Some(swift_type_base(err)),
        _ => None,
    };
    (ret, resp, user_err)
}

pub fn generate_phon_server(service: &ServiceDescriptor) -> String {
    let service_name = service.service_name.to_upper_camel_case();
    let mut out = String::new();

    // r[impl rpc.service]
    // r[impl rpc.service.methods]
    // Handler protocol (implemented by the user).
    if let Some(doc) = &service.doc {
        out.push_str(&format_doc(doc, ""));
    }
    out.push_str(&format!(
        "public protocol {service_name}Handler: Sendable {{\n"
    ));
    for method in service.methods {
        if let Some(doc) = &method.doc {
            out.push_str(&format_doc(doc, "    "));
        }
        let name = method.method_name.to_lower_camel_case();
        let args: Vec<String> = method
            .args
            .iter()
            .map(|a| format!("{}: {}", a.name.to_lower_camel_case(), server_arg_ty(a)))
            .collect();
        let ret = swift_type_server_return(method.return_shape);
        if ret == "Void" {
            out.push_str(&format!(
                "    func {name}({}) async throws\n",
                args.join(", ")
            ));
        } else {
            out.push_str(&format!(
                "    func {name}({}) async throws -> {ret}\n",
                args.join(", ")
            ));
        }
    }
    out.push_str("}\n\n");

    // r[impl rpc.handler]
    // Dispatcher.
    out.push_str(&format!(
        "public final class {service_name}Dispatcher: ServiceDispatcher {{\n"
    ));
    out.push_str(&format!("    private let handler: {service_name}Handler\n"));
    out.push_str(&format!(
        "    public init(handler: {service_name}Handler) {{ self.handler = handler }}\n\n"
    ));

    // encodeVoxError — encode a runtime error through any method's response type (the
    // non-User Err arms are independent of `T`/`E` on the wire, so the first method's
    // response program suffices). A method-less service has no response program, so it
    // returns empty bytes.
    match service.methods.first() {
        None => out.push_str(
            "    public func encodeVoxError(_ error: VoxRuntimeError) -> [UInt8] { [] }\n\n",
        ),
        Some(m0) => {
            let prefix0 = method_global_prefix(service.service_name, m0.method_name);
            let resp0 = swift_type_base(m0.response_wire_shape);
            let wire0 = match classify_shape(m0.response_wire_shape) {
                ShapeKind::Result { err, .. } => swift_type_base(err),
                _ => "VoxError<Infallible>".to_string(),
            };
            out.push_str(&format!(
                "    public func encodeVoxError(_ error: VoxRuntimeError) -> [UInt8] {{\n        let wire: {wire0}\n        switch error {{\n        case .unknownMethod, .notImplemented: wire = .unknownMethod\n        case .invalidPayload(let s), .decodeError(let s), .encodeError(let s): wire = .invalidPayload(s)\n        case .cancelled: wire = .cancelled\n        case .connectionClosed: wire = .connectionClosed\n        case .timeout: wire = .timedOut\n        case .indeterminate: wire = .indeterminate\n        }}\n        let r: {resp0} = .failure(wire)\n        return encodeVoxTyped(r, {prefix0}_ResponseEncoder)\n    }}\n\n"
            ));
        }
    }

    // r[impl rpc.channel.discovery]
    // preregister — mark the call's out-of-band channel ids known so incoming Data on
    // them buffers (instead of being rejected as unknown) before dispatch binds them.
    out.push_str("    public func preregister(methodId: UInt64, payload: [UInt8], channels: [UInt64], registry: ChannelRegistry) async {\n        for id in channels { await registry.markKnown(id) }\n    }\n\n");

    // r[impl rpc.service.methods]
    // r[impl rpc.unknown-method]
    // dispatch — route to per-method helpers.
    out.push_str("    public func dispatch(methodId: UInt64, payload: [UInt8], requestId: UInt64, channels: [UInt64], registry: ChannelRegistry, schemaSendTracker: SchemaSendTracker, schemaReceiveTracker: SchemaTracker, context: RequestContext, taskTx: @escaping @Sendable (TaskMessage) -> Void) async {\n        switch methodId {\n");
    for m in service.methods {
        // r[impl rpc.method-id]
        let id = hex_u64(crate::method_id(m));
        let name = m.method_name.to_lower_camel_case();
        let extra = if has_channels(m) {
            "channels: channels, registry: registry, "
        } else {
            ""
        };
        out.push_str(&format!(
            "        case {id}: await dispatch_{name}(payload: payload, requestId: requestId, {extra}schemaSendTracker: schemaSendTracker, schemaReceiveTracker: schemaReceiveTracker, taskTx: taskTx)\n"
        ));
    }
    out.push_str("        default: taskTx(.response(requestId: requestId, payload: encodeVoxError(.unknownMethod), methodId: methodId))\n        }\n    }\n\n");

    // Per-method dispatch helpers.
    for m in service.methods {
        out.push_str(&generate_dispatch_method(service, m));
    }

    out.push_str("}\n\n");
    out
}

fn generate_dispatch_method(service: &ServiceDescriptor, m: &MethodDescriptor) -> String {
    let id = hex_u64(crate::method_id(m));
    let name = m.method_name.to_lower_camel_case();
    let prefix = method_global_prefix(service.service_name, m.method_name);
    let svc = service.service_name.to_lower_camel_case();
    let (ret_ty, resp_ty, user_err) = method_types(m);
    let args_ty = swift_type_base(m.args_shape);
    let arity = m.args.len();
    let channels = has_channels(m);
    let mut out = String::new();
    let response_schema_closure = format!("{svc}Methods[{id}]!.responseSchemaClosure");

    let extra_params = if channels {
        "channels: [UInt64], registry: ChannelRegistry, "
    } else {
        ""
    };
    out.push_str(&format!(
        "    private func dispatch_{name}(payload: [UInt8], requestId: UInt64, {extra_params}schemaSendTracker: SchemaSendTracker, schemaReceiveTracker: SchemaTracker, taskTx: @escaping @Sendable (TaskMessage) -> Void) async {{\n"
    ));

    // Decode args (reconciling the peer's writer schema when advertised).
    // r[impl schema.errors.call-level.callee]
    if arity == 0 {
        // No args to decode.
    } else {
        // Reconcile the caller's advertised (writer) args schema against this reader —
        // the only decode path. No same-schema fallback: a missing writer schema is a
        // protocol error (the caller advertises it on the first call).
        out.push_str(&format!(
            "        guard let argsDecoder = schemaReceiveTracker.buildDecodeFn({id}, .args, readerDescriptor: {prefix}_ArgsDescriptor, readerBlocks: {prefix}_ArgsDescriptorBlocks, local: {svc}Registry) else {{\n            taskTx(.response(requestId: requestId, payload: encodeVoxError(.invalidPayload(\"no args schema advertised\")), methodId: {id}, responseSchemaClosure: {response_schema_closure}))\n            return\n        }}\n"
        ));
        out.push_str(&format!(
            "        let args: {args_ty}\n        do {{ args = try decodeVoxTyped(argsDecoder, payload) }} catch {{\n            taskTx(.response(requestId: requestId, payload: encodeVoxError(.invalidPayload(\"decode args\")), methodId: {id}, responseSchemaClosure: {response_schema_closure}))\n            return\n        }}\n"
        ));
    }

    // r[impl rpc.channel.discovery]
    // Bind out-of-band channels: a channel arg in the decoded tuple is the u32 LE wire
    // index into `channels`; resolve it to a `ChannelId` and create a server-side
    // `Tx`/`Rx`. A `Tx` arg means the handler SENDS (callee→caller); an `Rx` arg means
    // the handler RECEIVES (caller→callee). The bound handle replaces the arg slot.
    for (i, a) in m.args.iter().enumerate() {
        if !(is_tx(a.shape) || is_rx(a.shape)) {
            continue;
        }
        let an = a.name.to_lower_camel_case();
        let slot = if arity == 1 {
            "args".to_string()
        } else {
            format!("args.{i}")
        };
        out.push_str(&format!(
            "        let {an}WireIndex = channelWireIndex({slot})\n"
        ));
        out.push_str(&format!(
            "        guard {an}WireIndex < channels.count else {{\n            taskTx(.response(requestId: requestId, payload: encodeVoxError(.invalidPayload(\"channel wire index out of range\")), methodId: {id}, responseSchemaClosure: {response_schema_closure}))\n            return\n        }}\n"
        ));
        let elem_ty = swift_type_base(a.channel_element.expect("channel arg element shape"));
        if is_tx(a.shape) {
            // Handler SENDS → phon element ENCODE codec.
            let ser = element_encode_closure(&elem_ty, &format!("{prefix}_{an}_ElementEncoder"));
            out.push_str("        // r[impl schema.exchange.channels.tx-args]\n");
            out.push_str(&format!(
                "        let {an} = await bindServerTx(channelId: channels[{an}WireIndex], registry: registry, taskTx: taskTx, methodId: {id}, argsSchemaClosure: {svc}Methods[{id}]!.argsSchemaClosure, schemaSendTracker: schemaSendTracker, serialize: {ser})\n"
            ));
        } else {
            // Handler RECEIVES → phon element DECODE codec reconciled from the caller's
            // advertised auxiliary element root.
            let de = element_auxiliary_decode_closure(
                &elem_ty,
                "schemaReceiveTracker",
                &id,
                &format!("channel.arg.{i}.rx.element"),
                &format!("{prefix}_{an}_ElementDescriptor"),
                &format!("{prefix}_{an}_ElementDescriptorBlocks"),
                &format!("{svc}Registry"),
            );
            out.push_str("        // r[impl schema.exchange.channels.rx-args]\n");
            out.push_str(&format!(
                "        let {an} = await bindServerRx(channelId: channels[{an}WireIndex], registry: registry, taskTx: taskTx, deserialize: {de})\n"
            ));
        }
    }

    // The handler call expression, with labels. Channel args use the bound `Tx`/`Rx`
    // local just created; other args use the decoded value (`args` or `args.i`).
    let call_args: Vec<String> = m
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let label = a.name.to_lower_camel_case();
            let value = if is_tx(a.shape) || is_rx(a.shape) {
                label.clone()
            } else if arity == 1 {
                "args".to_string()
            } else {
                format!("args.{i}")
            };
            format!("{label}: {value}")
        })
        .collect();
    let call = format!("handler.{name}({})", call_args.join(", "));

    // r[impl rpc.fallible]
    // r[impl rpc.fallible.vox-error]
    // Call + wrap into the wire `Result<T, VoxError<E>>`. A fallible handler returns
    // `Result<T, E>` (its `.failure(e)` becomes the wire `User(e)`); an infallible one
    // returns `T`/`Void`. An unexpected throw maps to `Indeterminate`.
    // `voxResult`/`voxValue` are namespaced so they never collide with a handler arg
    // name (e.g. a channel arg literally named `result`, as in `postReplySum`).
    out.push_str(&format!(
        "        let voxResult: {resp_ty}\n        do {{\n"
    ));
    if user_err.is_some() {
        out.push_str(&format!("            let voxValue = try await {call}\n"));
        out.push_str(
            "            switch voxValue {\n            case .success(let v): voxResult = .success(v)\n            case .failure(let e): voxResult = .failure(.user(e))\n            }\n",
        );
    } else if ret_ty == "Void" {
        out.push_str(&format!(
            "            try await {call}\n            voxResult = .success(())\n"
        ));
    } else {
        out.push_str(&format!(
            "            let voxValue = try await {call}\n            voxResult = .success(voxValue)\n"
        ));
    }
    out.push_str(
        "        } catch {\n            voxResult = .failure(.indeterminate)\n        }\n",
    );

    // r[impl rpc.response]
    // Encode the response and reply, carrying the method's response schema closure.
    // The driver advertises it idempotently at the sequential send point, so the first
    // response written for this method carries the schema even under pipelining (the
    // schema decision must NOT be made here in the concurrent dispatch task).
    out.push_str(&format!(
        "        let respPayload = encodeVoxTyped(voxResult, {prefix}_ResponseEncoder)\n        taskTx(.response(requestId: requestId, payload: respPayload, methodId: {id}, responseSchemaClosure: {response_schema_closure}))\n    }}\n\n"
    ));

    out
}
