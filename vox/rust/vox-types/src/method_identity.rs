use std::collections::HashSet;

use facet::Facet;
use facet_core::{Def, Shape, Type, UserType};
use heck::ToKebabCase;

use crate::{ArgDescriptor, MethodDescriptor, MethodId, is_rx, is_tx};

/// Compute a method ID from service and method names.
///
/// Method IDs depend only on names, not on type signatures. This enables
/// schema exchange — two peers can use different type versions and still
/// route calls to the correct method.
// r[impl schema.method-id]
// r[impl rpc.method-id.algorithm]
// r[impl rpc.method-id.no-collisions]
pub fn method_id_name_only(service_name: &str, method_name: &str) -> MethodId {
    let mut input = Vec::new();
    input.extend_from_slice(service_name.to_kebab_case().as_bytes());
    input.push(b'.');
    input.extend_from_slice(method_name.to_kebab_case().as_bytes());
    let h = blake3::hash(&input);
    let first8: [u8; 8] = h.as_bytes()[0..8].try_into().expect("slice len");
    MethodId(u64::from_le_bytes(first8))
}

/// Build and leak a `MethodDescriptor`.
pub struct MethodDescriptorOptions {
    pub response_wire_shape: &'static Shape,
    pub doc: Option<&'static str>,
}

pub fn method_descriptor<'a, 'r, A: Facet<'a>, R: Facet<'r>>(
    service_name: &'static str,
    method_name: &'static str,
    arg_names: &[&'static str],
    channel_elements: &[Option<&'static Shape>],
    options: MethodDescriptorOptions,
) -> &'static MethodDescriptor {
    assert!(
        !shape_contains_channel(R::SHAPE),
        "channels are not allowed in return types: {service_name}.{method_name}"
    );
    // r[impl rpc.channel.no-collections]
    assert!(
        !shape_contains_channel_in_collection(A::SHAPE),
        "channels are not allowed inside collections: {service_name}.{method_name}"
    );
    // r[impl rpc.channel.direct-args]
    assert!(
        !shape_contains_indirect_channel_arg(A::SHAPE),
        "channels are only allowed as direct method arguments: {service_name}.{method_name}"
    );
    let args_have_channels = shape_contains_channel(A::SHAPE);

    // r[impl rpc.method-id]
    let id = method_id_name_only(service_name, method_name);

    let arg_shapes: &[&'static Shape] = match A::SHAPE.ty {
        Type::User(UserType::Struct(s)) => {
            let fields: Vec<&'static Shape> = s.fields.iter().map(|f| f.shape()).collect();
            Box::leak(fields.into_boxed_slice())
        }
        _ => &[],
    };

    assert_eq!(
        arg_names.len(),
        arg_shapes.len(),
        "arg_names length mismatch for {service_name}.{method_name}"
    );
    assert_eq!(
        arg_names.len(),
        channel_elements.len(),
        "channel_elements length mismatch for {service_name}.{method_name}"
    );

    let args: &'static [ArgDescriptor] = Box::leak(
        arg_names
            .iter()
            .zip(arg_shapes.iter())
            .zip(channel_elements.iter())
            .map(|((&name, &shape), &channel_element)| ArgDescriptor {
                name,
                shape,
                channel_element,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );

    Box::leak(Box::new(MethodDescriptor {
        id,
        service_name,
        method_name,
        args_shape: A::SHAPE,
        args,
        return_shape: R::SHAPE,
        response_wire_shape: options.response_wire_shape,
        args_have_channels,
        doc: options.doc,
    }))
}

pub fn shape_contains_channel(shape: &'static Shape) -> bool {
    fn visit(shape: &'static Shape, seen: &mut HashSet<&'static Shape>) -> bool {
        if is_tx(shape) || is_rx(shape) {
            return true;
        }

        if matches!(shape.def, Def::DynamicValue(_)) {
            return false;
        }

        if !seen.insert(shape) {
            return false;
        }

        if let Some(inner) = shape.inner
            && visit(inner, seen)
        {
            return true;
        }

        if shape.type_params.iter().any(|t| visit(t.shape, seen)) {
            return true;
        }

        match shape.def {
            Def::List(list_def) => visit(list_def.t(), seen),
            Def::Array(array_def) => visit(array_def.t(), seen),
            Def::Slice(slice_def) => visit(slice_def.t(), seen),
            Def::Map(map_def) => visit(map_def.k(), seen) || visit(map_def.v(), seen),
            Def::Set(set_def) => visit(set_def.t(), seen),
            Def::Option(opt_def) => visit(opt_def.t(), seen),
            Def::Result(result_def) => visit(result_def.t(), seen) || visit(result_def.e(), seen),
            Def::Pointer(ptr_def) => ptr_def.pointee.is_some_and(|p| visit(p, seen)),
            _ => match shape.ty {
                Type::User(UserType::Struct(s)) => s.fields.iter().any(|f| visit(f.shape(), seen)),
                Type::User(UserType::Enum(e)) => e
                    .variants
                    .iter()
                    .any(|v| v.data.fields.iter().any(|f| visit(f.shape(), seen))),
                _ => false,
            },
        }
    }

    let mut seen = HashSet::new();
    visit(shape, &mut seen)
}

// r[impl rpc.channel.no-collections]
pub fn shape_contains_channel_in_collection(shape: &'static Shape) -> bool {
    fn is_collection(def: Def) -> bool {
        matches!(
            def,
            Def::List(_) | Def::Array(_) | Def::Slice(_) | Def::Map(_) | Def::Set(_)
        )
    }

    fn visit(
        shape: &'static Shape,
        inside_collection: bool,
        seen: &mut HashSet<(&'static Shape, bool)>,
    ) -> bool {
        if is_tx(shape) || is_rx(shape) {
            return inside_collection;
        }

        if matches!(shape.def, Def::DynamicValue(_)) {
            return false;
        }

        if !seen.insert((shape, inside_collection)) {
            return false;
        }

        let nested_inside_collection = inside_collection || is_collection(shape.def);

        if let Some(inner) = shape.inner
            && visit(inner, nested_inside_collection, seen)
        {
            return true;
        }

        if shape
            .type_params
            .iter()
            .any(|t| visit(t.shape, nested_inside_collection, seen))
        {
            return true;
        }

        match shape.def {
            Def::List(list_def) => visit(list_def.t(), true, seen),
            Def::Array(array_def) => visit(array_def.t(), true, seen),
            Def::Slice(slice_def) => visit(slice_def.t(), true, seen),
            Def::Map(map_def) => visit(map_def.k(), true, seen) || visit(map_def.v(), true, seen),
            Def::Set(set_def) => visit(set_def.t(), true, seen),
            Def::Option(opt_def) => visit(opt_def.t(), nested_inside_collection, seen),
            Def::Result(result_def) => {
                visit(result_def.t(), nested_inside_collection, seen)
                    || visit(result_def.e(), nested_inside_collection, seen)
            }
            Def::Pointer(ptr_def) => ptr_def
                .pointee
                .is_some_and(|p| visit(p, nested_inside_collection, seen)),
            _ => match shape.ty {
                Type::User(UserType::Struct(s)) => s
                    .fields
                    .iter()
                    .any(|f| visit(f.shape(), nested_inside_collection, seen)),
                Type::User(UserType::Enum(e)) => e.variants.iter().any(|v| {
                    v.data
                        .fields
                        .iter()
                        .any(|f| visit(f.shape(), nested_inside_collection, seen))
                }),
                _ => false,
            },
        }
    }

    let mut seen = HashSet::new();
    visit(shape, false, &mut seen)
}

// r[impl rpc.channel.direct-args]
pub fn shape_contains_indirect_channel_arg(args_shape: &'static Shape) -> bool {
    match args_shape.ty {
        Type::User(UserType::Struct(s)) => s.fields.iter().any(|field| {
            let field_shape = field.shape();
            !(is_tx(field_shape) || is_rx(field_shape)) && shape_contains_channel(field_shape)
        }),
        _ => shape_contains_channel(args_shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    fn unit_response_options() -> MethodDescriptorOptions {
        MethodDescriptorOptions {
            response_wire_shape: <() as Facet>::SHAPE,
            doc: None,
        }
    }

    // r[verify schema.method-id]
    // r[verify rpc.method-id.algorithm]
    #[test]
    fn method_id_name_only_is_stable_across_case_variations() {
        let a = method_id_name_only("MyService", "DoThingFast");
        let b = method_id_name_only("my-service", "do-thing-fast");
        let c = method_id_name_only("MY_SERVICE", "DO_THING_FAST");
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    // r[verify rpc.method-id]
    #[test]
    fn method_id_name_only_different_methods_produce_different_ids() {
        let a = method_id_name_only("Svc", "alpha");
        let b = method_id_name_only("Svc", "beta");
        assert_ne!(a, b);
    }

    // r[verify rpc.method-id.no-collisions]
    #[test]
    fn method_id_name_only_includes_service_name() {
        let a = method_id_name_only("AlphaSvc", "echo");
        let b = method_id_name_only("BetaSvc", "echo");
        assert_ne!(a, b);
    }

    // r[verify rpc.channel.no-collections]
    #[test]
    fn method_descriptor_rejects_channel_inside_nested_collection_arg() {
        #[derive(Facet)]
        struct Nested {
            stream: Vec<crate::Tx<u8>>,
        }

        #[derive(Facet)]
        struct Args {
            nested: Nested,
        }

        let result = std::panic::catch_unwind(|| {
            let _ = method_descriptor::<Args, ()>(
                "StreamService",
                "bad",
                &["nested"],
                &[None],
                unit_response_options(),
            );
        });

        let panic = result.expect_err("descriptor should reject channels in collections");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&'static str>().copied())
            .expect("panic payload should be a string");
        assert!(
            message.contains("channels are not allowed inside collections: StreamService.bad"),
            "unexpected panic message: {message}"
        );
    }

    // r[verify rpc.channel.direct-args]
    #[test]
    fn method_descriptor_rejects_channel_inside_nested_struct_arg() {
        #[derive(Facet)]
        struct Nested {
            stream: crate::Tx<u8>,
        }

        #[derive(Facet)]
        struct Args {
            nested: Nested,
        }

        let result = std::panic::catch_unwind(|| {
            let _ = method_descriptor::<Args, ()>(
                "StreamService",
                "bad",
                &["nested"],
                &[None],
                unit_response_options(),
            );
        });

        let panic = result.expect_err("descriptor should reject nested channel args");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&'static str>().copied())
            .expect("panic payload should be a string");
        assert!(
            message.contains(
                "channels are only allowed as direct method arguments: StreamService.bad"
            ),
            "unexpected panic message: {message}"
        );
    }

    // r[verify rpc.channel.direct-args]
    #[test]
    fn method_descriptor_rejects_channel_inside_option_arg() {
        #[derive(Facet)]
        struct Args {
            maybe: Option<crate::Tx<u8>>,
        }

        let result = std::panic::catch_unwind(|| {
            let _ = method_descriptor::<Args, ()>(
                "StreamService",
                "bad",
                &["maybe"],
                &[None],
                unit_response_options(),
            );
        });

        let panic = result.expect_err("descriptor should reject optional channel args");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&'static str>().copied())
            .expect("panic payload should be a string");
        assert!(
            message.contains(
                "channels are only allowed as direct method arguments: StreamService.bad"
            ),
            "unexpected panic message: {message}"
        );
    }
}
