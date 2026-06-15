//! Per-service phon schema emission for the Swift runtime.
//!
//! Mirrors `targets/typescript/phon.rs` but emits Swift: the `{service}Methods`
//! table of `PhonMethodSchemas` (args/response roots + closures + channel metadata)
//! AND — what the Swift typed path needs beyond TS — a `Descriptor` per method's
//! args tuple and response wire type (`Result<T, VoxError<E>>`), plus the merged
//! `{service}Registry`.

use base64::{Engine, engine::general_purpose::STANDARD};
use facet_core::Shape;
use heck::ToLowerCamelCase;
use vox_types::{ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use super::phon_descriptor::{blocks_literal, descriptor_with_blocks};
use crate::render::hex_u64;

/// The ok (success) type behind a method's declared return type.
fn ok_shape(return_shape: &'static Shape) -> &'static Shape {
    match classify_shape(return_shape) {
        ShapeKind::Result { ok, .. } => ok,
        _ => return_shape,
    }
}

/// The content-derived phon root id for a shape.
// r[impl schema.type-id]
fn root_id(shape: &'static Shape) -> u64 {
    vox_phon::schema_id_for_shape(shape)
        .expect("phon schema id")
        .0
}

/// A shape's schema closure bytes as a Swift expression yielding `[UInt8]`.
// r[impl schema.principles.once-per-type]
// r[impl schema.format.self-contained]
// r[impl schema.tracking.transitive]
// r[impl schema.format.binding-roots]
fn closure_expr(shape: &'static Shape, auxiliary_roots: &[(String, &'static Shape)]) -> String {
    let auxiliary_roots: Vec<(&str, &'static Shape)> = auxiliary_roots
        .iter()
        .map(|(role, shape)| (role.as_str(), *shape))
        .collect();
    let bytes = if auxiliary_roots.is_empty() {
        vox_phon::schema_bytes_for_shape(shape).expect("phon schema bytes")
    } else {
        vox_phon::schema_bytes_for_shape_with_auxiliary_roots(shape, &auxiliary_roots)
            .expect("phon schema bytes")
    };
    let encoded = STANDARD.encode(bytes);
    let mut out = String::from("voxSchemaClosure(\"\"\"\n");
    for chunk in encoded.as_bytes().chunks(100) {
        out.push_str("          ");
        out.push_str(std::str::from_utf8(chunk).expect("base64 is utf-8"));
        out.push('\n');
    }
    out.push_str("          \"\"\")");
    out
}

// r[impl schema.exchange.channels]
fn channel_auxiliary_roots(method: &vox_types::MethodDescriptor) -> Vec<(String, &'static Shape)> {
    method
        .args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            let direction = if is_tx(arg.shape) {
                "tx"
            } else if is_rx(arg.shape) {
                "rx"
            } else {
                return None;
            };
            let element = arg
                .channel_element
                .expect("Tx/Rx arg must carry its channel element shape");
            Some((format!("channel.arg.{index}.{direction}.element"), element))
        })
        .collect()
}

/// The wire error type `VoxError<E>` (`Result<T, VoxError<E>>` is every method's
/// response) + the empty `Infallible` (the `E` of an infallible method). Generated
/// once; the per-method response descriptors materialize them on the typed path.
pub fn generate_wire_error_types() -> String {
    let mut out = String::new();
    out.push_str("// MARK: - wire error type\n\n");
    // `Infallible`: Rust `core::convert::Infallible` (uninhabited) — an infallible
    // method's `User(E)` arm is never constructed.
    out.push_str("public enum Infallible: Sendable {}\n\n");
    out.push_str("/// The wire error of `Result<T, VoxError<E>>`. Variant order matches the\n");
    out.push_str("/// Rust `VoxError<E>` (User=0 … Indeterminate=7) so wire indices align.\n");
    out.push_str("public enum VoxError<E: Sendable>: Error, Sendable {\n");
    out.push_str("    case user(E)\n");
    out.push_str("    case unknownMethod\n");
    out.push_str("    case invalidPayload(String)\n");
    out.push_str("    case cancelled\n");
    out.push_str("    case connectionClosed\n");
    out.push_str("    case sessionShutdown\n");
    out.push_str("    case sendFailed\n");
    out.push_str("    case indeterminate\n");
    out.push_str("}\n\n");
    out
}

/// Generate the `{service}` phon registry + per-method schema table + descriptors.
pub fn generate_phon_service(service: &ServiceDescriptor) -> String {
    let name = service.service_name.to_lower_camel_case();
    let mut out = String::new();

    out.push_str(&generate_wire_error_types());
    out.push_str(
        "// MARK: - phon service schemas (registry + per-method roots/descriptors/channels)\n\n",
    );
    out.push_str(
        r#"private func voxSchemaClosure(_ base64: String) -> [UInt8] {
    guard let data = Data(base64Encoded: base64, options: .ignoreUnknownCharacters) else {
        preconditionFailure("invalid generated phon schema closure")
    }
    return [UInt8](data)
}

"#,
    );

    // Per-method descriptor globals (the args tuple + the `Result<T, VoxError<E>>`
    // wire type). Built once; immutable — `nonisolated(unsafe)` like the envelope.
    for m in service.methods {
        let mname = m.method_name.to_lower_camel_case();
        let (args_desc, args_blocks) = descriptor_with_blocks(m.args_shape);
        let (resp_desc, resp_blocks) = descriptor_with_blocks(m.response_wire_shape);
        out.push_str(&format!(
            "nonisolated(unsafe) let {name}_{mname}_ArgsDescriptor: Descriptor = {args_desc}\n"
        ));
        out.push_str(&format!(
            "nonisolated(unsafe) let {name}_{mname}_ArgsDescriptorBlocks: [SchemaId: Descriptor] = {}\n",
            blocks_literal(&args_blocks)
        ));
        out.push_str(&format!(
            "nonisolated(unsafe) let {name}_{mname}_ResponseDescriptor: Descriptor = {resp_desc}\n"
        ));
        out.push_str(&format!(
            "nonisolated(unsafe) let {name}_{mname}_ResponseDescriptorBlocks: [SchemaId: Descriptor] = {}\n",
            blocks_literal(&resp_blocks)
        ));
    }
    out.push('\n');

    out.push_str(&format!(
        "public let {name}Methods: [UInt64: PhonMethodSchemas] = [\n"
    ));
    for m in service.methods {
        let mname = m.method_name.to_lower_camel_case();
        // r[impl schema.method-id]
        // r[impl rpc.method-id.algorithm]
        // r[impl rpc.method-id.no-collisions]
        let method_id = crate::method_id(m);
        let args_root = root_id(m.args_shape);
        let ok_root = root_id(ok_shape(m.return_shape));
        let response_root = root_id(m.response_wire_shape);
        let channel_auxiliary_roots = channel_auxiliary_roots(m);
        let args_closure = closure_expr(m.args_shape, &channel_auxiliary_roots);
        let response_closure = closure_expr(m.response_wire_shape, &[]);

        out.push_str(&format!("    {}: PhonMethodSchemas(\n", hex_u64(method_id)));
        out.push_str(&format!(
            "        argsRoot: SchemaId({}),\n",
            hex_u64(args_root)
        ));
        out.push_str(&format!("        argsSchemaClosure: {args_closure},\n"));
        out.push_str(&format!(
            "        argsDescriptor: {name}_{mname}_ArgsDescriptor,\n"
        ));
        out.push_str(&format!(
            "        argsDescriptorBlocks: {name}_{mname}_ArgsDescriptorBlocks,\n"
        ));
        out.push_str(&format!(
            "        okRoot: SchemaId({}),\n",
            hex_u64(ok_root)
        ));
        out.push_str(&format!(
            "        responseRoot: SchemaId({}),\n",
            hex_u64(response_root)
        ));
        out.push_str(&format!(
            "        responseSchemaClosure: {response_closure},\n"
        ));
        out.push_str(&format!(
            "        responseDescriptor: {name}_{mname}_ResponseDescriptor,\n"
        ));
        out.push_str(&format!(
            "        responseDescriptorBlocks: {name}_{mname}_ResponseDescriptorBlocks,\n"
        ));
        out.push_str("        channels: [");
        let mut first = true;
        for (i, arg) in m.args.iter().enumerate() {
            let dir = if is_tx(arg.shape) {
                Some(true)
            } else if is_rx(arg.shape) {
                Some(false)
            } else {
                None
            };
            if let Some(is_tx_dir) = dir {
                let element = arg
                    .channel_element
                    .expect("Tx/Rx arg must carry its channel element shape");
                if !first {
                    out.push_str(", ");
                }
                first = false;
                out.push_str(&format!(
                    "PhonChannelMeta(index: {i}, isTx: {is_tx_dir}, elementRoot: SchemaId({}), elementSchemaClosure: {})",
                    hex_u64(root_id(element)),
                    closure_expr(element, &[])
                ));
            }
        }
        out.push_str("]),\n");
    }
    out.push_str("]\n\n");

    out.push_str(&format!(
        "nonisolated(unsafe) public let {name}Registry: Registry = buildServiceRegistry({name}Methods)\n\n"
    ));

    // Per-method ENCODE programs only (own-schema `lowerTyped`): the client encodes
    // args, the server encodes the response. There are NO cached decode programs —
    // every decode reconciles `lowerDecode(writer → reader)` against the peer's
    // advertised schema, built and cached in the connection's `SchemaTracker`.
    out.push_str("// MARK: - per-method encode programs\n\n");
    for m in service.methods {
        let mname = m.method_name.to_lower_camel_case();
        out.push_str(&format!(
            "nonisolated(unsafe) let {name}_{mname}_ArgsEncodeProgram: Lowered = try! lowerTyped({name}_{mname}_ArgsDescriptor, {name}Registry, {name}_{mname}_ArgsDescriptorBlocks)\n"
        ));
        out.push_str(&format!(
            "nonisolated(unsafe) let {name}_{mname}_ResponseEncodeProgram: Lowered = try! lowerTyped({name}_{mname}_ResponseDescriptor, {name}Registry, {name}_{mname}_ResponseDescriptorBlocks)\n"
        ));
        out.push_str(&format!(
            "let {name}_{mname}_ArgsEncoder = VoxTypedEncoder({name}_{mname}_ArgsEncodeProgram)\n"
        ));
        out.push_str(&format!(
            "let {name}_{mname}_ResponseEncoder = VoxTypedEncoder({name}_{mname}_ResponseEncodeProgram)\n"
        ));
    }
    out.push('\n');

    // Per-channel-argument element encode programs + reader descriptors. Receive-side
    // decode programs are built lazily from the peer's advertised auxiliary element root
    // (`channel.arg.N.{tx|rx}.element`) by `SchemaTracker`.
    let mut emitted_elements = false;
    for m in service.methods {
        let mname = m.method_name.to_lower_camel_case();
        for arg in m.args {
            if !(is_tx(arg.shape) || is_rx(arg.shape)) {
                continue;
            }
            if !emitted_elements {
                out.push_str("// MARK: - per-channel element codec programs\n\n");
                emitted_elements = true;
            }
            let an = arg.name.to_lower_camel_case();
            let element = arg
                .channel_element
                .expect("Tx/Rx arg must carry its channel element shape");
            let (elem_desc, elem_blocks) = descriptor_with_blocks(element);
            let elem_blocks_lit = blocks_literal(&elem_blocks);
            out.push_str(&format!(
                "nonisolated(unsafe) let {name}_{mname}_{an}_ElementDescriptor: Descriptor = {elem_desc}\n"
            ));
            out.push_str(&format!(
                "nonisolated(unsafe) let {name}_{mname}_{an}_ElementDescriptorBlocks: [SchemaId: Descriptor] = {elem_blocks_lit}\n"
            ));
            out.push_str(&format!(
                "nonisolated(unsafe) let {name}_{mname}_{an}_ElementEncodeProgram: Lowered = try! lowerTyped({name}_{mname}_{an}_ElementDescriptor, {name}Registry, {name}_{mname}_{an}_ElementDescriptorBlocks)\n"
            ));
            out.push_str(&format!(
                "let {name}_{mname}_{an}_ElementEncoder = VoxTypedEncoder({name}_{mname}_{an}_ElementEncodeProgram)\n"
            ));
        }
    }
    if emitted_elements {
        out.push('\n');
    }
    out
}

/// Swift closure literal `(T, inout ByteBuffer) -> Void` that phon-encodes a channel
/// element via `program` and appends its bytes (a channel Data frame *is* the element).
/// `elem_ty` annotates the value so the closure type (and thus the `ByteBuffer` buffer
/// param) is inferable at the call site — keeping NIO out of the generated code.
pub fn element_encode_closure(elem_ty: &str, program: &str) -> String {
    format!("{{ (v: {elem_ty}, buf) in buf.writeBytes(encodeVoxTyped(v, {program})) }}")
}

/// Swift closure literal `(inout ByteBuffer) throws -> T` that reads a channel Data
/// frame's bytes and phon-decodes the element through the peer's advertised auxiliary
/// root. The return annotation pins the element type so the buffer param infers as
/// `inout ByteBuffer` (no NIO name).
pub fn element_auxiliary_decode_closure(
    elem_ty: &str,
    tracker: &str,
    method_id: &str,
    role: &str,
    descriptor: &str,
    descriptor_blocks: &str,
    registry: &str,
) -> String {
    format!(
        "{{ (buf) throws -> {elem_ty} in let bytes = buf.readBytes(length: buf.readableBytes) ?? []; guard let decoder = {tracker}.buildAuxiliaryDecodeFn({method_id}, .args, role: \"{role}\", readerDescriptor: {descriptor}, readerBlocks: {descriptor_blocks}, local: {registry}) else {{ throw VoxError<Infallible>.invalidPayload(\"no channel element schema advertised\") }}; return try decodeVoxTyped(decoder, bytes) }}"
    )
}

/// The name of the Swift descriptor/program globals for a method's args/response.
pub fn method_global_prefix(service_name: &str, method_name: &str) -> String {
    use heck::ToLowerCamelCase;
    format!(
        "{}_{}",
        service_name.to_lower_camel_case(),
        method_name.to_lower_camel_case()
    )
}
