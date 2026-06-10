//! Per-service phon schema emission for the TypeScript runtime.
//!
//! Emits a phon `Registry` covering every method's args + ok-type schemas, plus a
//! per-method table the runtime needs: the args root id, the args schema-closure
//! bytes to advertise in the `schemas:` field, the ok root id, and channel
//! metadata (which arg is a `Tx`/`Rx`, its direction, and its element root) —
//! since channels are opaque on the wire (`r[rpc.channel.payload-encoding]`).

use facet_core::Shape;
use vox_types::{ServiceDescriptor, ShapeKind, classify_shape, is_rx, is_tx};

use crate::render::hex_u64;

/// The ok (success) type shape behind a method's declared return type.
fn ok_shape(return_shape: &'static Shape) -> &'static Shape {
    match classify_shape(return_shape) {
        ShapeKind::Result { ok, .. } => ok,
        _ => return_shape,
    }
}

/// The content-derived phon root id for a single shape.
// r[impl schema.type-id]
fn root_id(shape: &'static Shape) -> u64 {
    phon_codegen::Module::from_shapes(&[shape])
        .expect("derive phon schema")
        .roots[0]
        .id
        .0
}

fn schema_closure_hex(
    shape: &'static Shape,
    auxiliary_roots: &[(String, &'static Shape)],
) -> String {
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
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
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

/// Generate the `{service}` phon registry + per-method schema table.
// r[impl schema.principles.once-per-type]
// r[impl schema.format.self-contained]
// r[impl schema.tracking.transitive]
pub fn generate_phon_service(service: &ServiceDescriptor) -> String {
    let name = lower_camel(service.service_name);

    // One registry over every method's args tuple, response wire type
    // (`Result<T, VoxError<E>>`), and direct channel element type, deduped and
    // transitive. Channel args are opaque in the args tuple, so their item DTOs
    // must be added explicitly for local per-item encode/decode.
    let mut roots: Vec<&'static Shape> = Vec::new();
    for m in service.methods {
        roots.push(m.args_shape);
        roots.push(m.response_wire_shape);
        for arg in m.args {
            if let Some(element) = arg.channel_element {
                roots.push(element);
            }
        }
    }
    let module = phon_codegen::Module::from_shapes(&roots).expect("derive service phon module");

    let mut out = String::new();
    out.push_str(
        "// phon schemas for this service (registry + per-method roots + channel metadata).\n",
    );
    out.push_str(
        "import { Registry, schemaFromBytes, hexToBytes } from \"@bearcove/phon-schema\";\n",
    );
    out.push_str("import type { Primitive } from \"@bearcove/phon-schema\";\n\n");

    out.push_str(&phon_codegen::typescript::render_registry(
        &module,
        &format!("{name}Registry"),
    ));
    out.push('\n');

    // An index signature (not `Record<…>`): a service may define a type named
    // `Record`, which would shadow the global utility type here.
    out.push_str(&format!(
        "export const {name}Methods: {{ [methodId: string]: import(\"@bearcove/vox-core\").PhonMethodSchemas }} = {{\n"
    ));
    for m in service.methods {
        // r[impl schema.method-id]
        // r[impl rpc.method-id.algorithm]
        // r[impl rpc.method-id.no-collisions]
        let method_id = crate::method_id(m);
        let args_root = root_id(m.args_shape);
        let ok_root = root_id(ok_shape(m.return_shape));
        let response_root = root_id(m.response_wire_shape);
        let channel_auxiliary_roots = channel_auxiliary_roots(m);
        let closure = schema_closure_hex(m.args_shape, &channel_auxiliary_roots);
        let response_closure = schema_closure_hex(m.response_wire_shape, &[]);

        out.push_str(&format!("  \"{}\": {{\n", hex_u64(method_id)));
        out.push_str(&format!("    argsRoot: {}n,\n", hex_u64(args_root)));
        out.push_str(&format!("    argsSchemaClosure: \"{closure}\",\n"));
        out.push_str(&format!("    okRoot: {}n,\n", hex_u64(ok_root)));
        out.push_str(&format!("    responseRoot: {}n,\n", hex_u64(response_root)));
        out.push_str(&format!(
            "    responseSchemaClosure: \"{response_closure}\",\n"
        ));
        out.push_str("    channels: [");
        let mut first = true;
        for (i, arg) in m.args.iter().enumerate() {
            let dir = if is_tx(arg.shape) {
                Some("tx")
            } else if is_rx(arg.shape) {
                Some("rx")
            } else {
                None
            };
            if let Some(dir) = dir {
                // `Tx`/`Rx` are opaque; the service macro captures the element.
                let element = arg
                    .channel_element
                    .expect("Tx/Rx arg must carry its channel element shape");
                if !first {
                    out.push_str(", ");
                }
                first = false;
                out.push_str(&format!(
                    "{{ index: {i}, direction: \"{dir}\", elementRoot: {}n }}",
                    hex_u64(root_id(element))
                ));
            }
        }
        out.push_str("],\n");
        out.push_str("  },\n");
    }
    out.push_str("};\n");
    out
}

fn lower_camel(s: &str) -> String {
    use heck::ToLowerCamelCase;
    s.to_lower_camel_case()
}
