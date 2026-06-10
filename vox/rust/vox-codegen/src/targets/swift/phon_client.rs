//! Swift phon client emitter: the `{Service}Caller` protocol + `{Service}Client`
//! whose method bodies encode args via the typed path, call the runtime with the
//! method's `ClientSchemaInfo`, decode the `Result<T, VoxError<E>>` response, and
//! unwrap (throw on `Err`). Replaces the postcard `client.rs` bodies.

use heck::{ToLowerCamelCase, ToUpperCamelCase};
use vox_types::{ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use super::phon_service::{
    element_auxiliary_decode_closure, element_encode_closure, method_global_prefix,
};
use super::types::{format_doc, swift_type_base, swift_type_client_arg, swift_type_client_return};
use crate::render::hex_u64;

/// The client-side Swift type of an argument. A channel arg's `shape` is an opaque
/// adapter (→ `Data`); its real element lives in `channel_element`, so emit the
/// `UnboundTx`/`UnboundRx` the caller binds.
fn client_arg_ty(a: &vox_types::ArgDescriptor) -> String {
    if is_tx(a.shape) {
        format!(
            "UnboundTx<{}>",
            swift_type_base(a.channel_element.expect("tx element"))
        )
    } else if is_rx(a.shape) {
        format!(
            "UnboundRx<{}>",
            swift_type_base(a.channel_element.expect("rx element"))
        )
    } else {
        swift_type_client_arg(a.shape)
    }
}

/// A method's signature `name(arg: T, …)` and its return type (or `Void`).
fn method_signature(method: &vox_types::MethodDescriptor) -> (String, String, String) {
    let name = method.method_name.to_lower_camel_case();
    let args: Vec<String> = method
        .args
        .iter()
        .map(|a| format!("{}: {}", a.name.to_lower_camel_case(), client_arg_ty(a)))
        .collect();
    let ret = swift_type_client_return(method.return_shape);
    (name, args.join(", "), ret)
}

pub fn generate_phon_client(service: &ServiceDescriptor) -> String {
    let service_name = service.service_name.to_upper_camel_case();
    let svc = service.service_name.to_lower_camel_case();
    let mut out = String::new();

    // r[impl rpc.caller]
    // The caller protocol.
    if let Some(doc) = &service.doc {
        out.push_str(&format_doc(doc, ""));
    }
    out.push_str(&format!(
        "public protocol {service_name}Caller: Sendable {{\n"
    ));
    for method in service.methods {
        if let Some(doc) = &method.doc {
            out.push_str(&format_doc(doc, "    "));
        }
        let (name, args, ret) = method_signature(method);
        if ret == "Void" {
            out.push_str(&format!("    func {name}({args}) async throws\n"));
        } else {
            out.push_str(&format!("    func {name}({args}) async throws -> {ret}\n"));
        }
    }
    out.push_str("}\n\n");

    // The client.
    // r[impl rpc.caller]
    out.push_str(&format!(
        "public final class {service_name}Client: {service_name}Caller, Sendable {{\n"
    ));
    out.push_str("    private let connection: VoxConnection\n");
    out.push_str("    private let timeout: TimeInterval?\n\n");
    out.push_str(
        "    public init(connection: VoxConnection, timeout: TimeInterval? = 30.0) {\n        self.connection = connection\n        self.timeout = timeout\n    }\n\n",
    );

    for method in service.methods {
        let (name, args, ret) = method_signature(method);
        // r[impl rpc.method-id]
        let method_id = hex_u64(crate::method_id(method));
        let prefix = method_global_prefix(service.service_name, method.method_name);
        let resp_ty = swift_type_base(method.response_wire_shape);
        let has_channels = method.args.iter().any(|a| is_tx(a.shape) || is_rx(a.shape));

        let sig = if ret == "Void" {
            format!("    public func {name}({args}) async throws {{\n")
        } else {
            format!("    public func {name}({args}) async throws -> {ret} {{\n")
        };
        out.push_str(&sig);

        // Encode args via the typed seam. A channel arg (`Tx`/`Rx`) rides out-of-band:
        // the caller allocates a `ChannelId`, binds the paired handle, and the arg in the
        // payload is the 4-byte LE wire index into `RequestCall.channels` (a `Data`/bytes
        // field per the args descriptor). Non-channel args encode by value. 0 args → empty;
        // 1 arg → the bare value; N args → a Swift tuple (the descriptor is positional).
        let mut arg_exprs: Vec<String> = Vec::new();
        let mut finalizers: Vec<String> = Vec::new();
        if has_channels {
            // r[impl rpc.channel.discovery]
            out.push_str("        var channelIds: [UInt64] = []\n");
        }
        for (i, a) in method.args.iter().enumerate() {
            let an = a.name.to_lower_camel_case();
            if is_rx(a.shape) {
                // Method wants an `Rx` → caller SENDS via the paired `Tx`: inject the phon
                // element ENCODE codec.
                let elem_ty = swift_type_base(a.channel_element.expect("rx element"));
                let ser =
                    element_encode_closure(&elem_ty, &format!("{prefix}_{an}_ElementEncoder"));
                out.push_str(&format!("        let {an}WireIndex = channelIds.count\n"));
                out.push_str(&format!(
                    "        channelIds.append(await connection.bindClientRxArg({an}, serialize: {ser}))\n"
                ));
                arg_exprs.push(format!("Data(channelWireIndexBytes({an}WireIndex))"));
                finalizers.push(format!("finalizeChannel({an})"));
            } else if is_tx(a.shape) {
                // Method wants a `Tx` → caller RECEIVES via the paired `Rx`: inject the phon
                // element DECODE codec reconciled from the callee's advertised auxiliary
                // element root.
                let elem_ty = swift_type_base(a.channel_element.expect("tx element"));
                let de = element_auxiliary_decode_closure(
                    &elem_ty,
                    "self.connection.schemaReceiveTracker",
                    &method_id,
                    &format!("channel.arg.{i}.tx.element"),
                    &format!("{prefix}_{an}_ElementDescriptor"),
                    &format!("{prefix}_{an}_ElementDescriptorBlocks"),
                    &format!("{svc}Registry"),
                );
                out.push_str(&format!("        let {an}WireIndex = channelIds.count\n"));
                out.push_str("        // r[impl schema.exchange.channels.tx-args]\n");
                out.push_str(&format!(
                    "        channelIds.append(await connection.bindClientTxArg({an}, deserialize: {de}))\n"
                ));
                arg_exprs.push(format!("Data(channelWireIndexBytes({an}WireIndex))"));
                finalizers.push(format!("finalizeChannel({an})"));
            } else {
                arg_exprs.push(an);
            }
        }
        match arg_exprs.len() {
            0 => out.push_str("        let payload: [UInt8] = []\n"),
            1 => out.push_str(&format!(
                "        let payload = encodeVoxTyped({}, {prefix}_ArgsEncoder)\n",
                arg_exprs[0]
            )),
            _ => out.push_str(&format!(
                "        let payload = encodeVoxTyped(({}), {prefix}_ArgsEncoder)\n",
                arg_exprs.join(", ")
            )),
        }

        // Call the runtime with this method's schema info (advertises args closure).
        // r[impl rpc.request]
        let channels_arg = if has_channels {
            "channels: channelIds, "
        } else {
            ""
        };
        let finalize_arg = if finalizers.is_empty() {
            "nil".to_string()
        } else {
            format!("{{ {} }}", finalizers.join("; "))
        };
        out.push_str(&format!(
            "        let response = try await connection.call(\n            methodId: {method_id}, metadata: .null, payload: payload, {channels_arg}timeout: timeout,\n            finalizeChannels: {finalize_arg},\n            schemaInfo: ClientSchemaInfo(methodSchemas: {svc}Methods[{method_id}]!, registry: {svc}Registry))\n"
        ));

        // Decode the wire `Result<T, VoxError<E>>` response by reconciling the server's
        // advertised (writer) response schema against this reader — the only decode path,
        // built/cached in the connection's SchemaTracker (no same-schema program). Then
        // unwrap into the method's return type: a fallible method (`Result<T, E>`) maps
        // the wire `User(E)` arm to the user `.failure(e)` and throws other VoxErrors; an
        // infallible method returns `T` (or `Void`) and throws on any error.
        // r[impl rpc.fallible.caller-signature]
        // r[impl schema.errors.call-level.caller]
        // r[impl schema.errors.same-peer-terminal]
        let is_fallible = matches!(
            classify_shape(method.return_shape),
            ShapeKind::Result { .. }
        );
        out.push_str(&format!(
            "        guard let respDecoder = connection.schemaReceiveTracker.buildDecodeFn({method_id}, .response, readerDescriptor: {prefix}_ResponseDescriptor, readerBlocks: {prefix}_ResponseDescriptorBlocks, local: {svc}Registry) else {{\n            throw VoxError<Infallible>.invalidPayload(\"no response schema advertised\")\n        }}\n"
        ));
        out.push_str(&format!(
            "        let result: {resp_ty} = try decodeVoxTyped(respDecoder, response)\n        switch result {{\n"
        ));
        if is_fallible {
            out.push_str("        case .success(let value): return .success(value)\n");
            out.push_str("        case .failure(.user(let e)): return .failure(e)\n");
            out.push_str("        case .failure(let voxError): throw voxError\n");
        } else if ret == "Void" {
            out.push_str("        case .success: return\n");
            out.push_str("        case .failure(let error): throw error\n");
        } else {
            out.push_str("        case .success(let value): return value\n");
            out.push_str("        case .failure(let error): throw error\n");
        }
        out.push_str("        }\n    }\n\n");
    }

    out.push_str("}\n\n");
    out
}
