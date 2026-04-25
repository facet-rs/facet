use std::collections::HashSet;

use facet::Facet;
use facet_core::{Def, Shape, Type, UserType};
use heck::ToKebabCase;

use crate::{ArgDescriptor, MethodDescriptor, MethodId, RetryPolicy, is_rx, is_tx};

/// Compute a method ID from service and method names.
///
/// Method IDs depend only on names, not on type signatures. This enables
/// schema exchange — two peers can use different type versions and still
/// route calls to the correct method.
// r[impl schema.method-id]
pub fn method_id_name_only(service_name: &str, method_name: &str) -> MethodId {
    let mut input = Vec::new();
    input.extend_from_slice(service_name.to_kebab_case().as_bytes());
    input.push(b'.');
    input.extend_from_slice(method_name.to_kebab_case().as_bytes());
    let h = blake3::hash(&input);
    let first8: [u8; 8] = h.as_bytes()[0..8].try_into().expect("slice len");
    MethodId(u64::from_le_bytes(first8))
}

/// Build and leak a `MethodDescriptor` with default volatile retry policy.
pub fn method_descriptor<'a, 'r, A: Facet<'a>, R: Facet<'r>>(
    service_name: &'static str,
    method_name: &'static str,
    arg_names: &[&'static str],
    doc: Option<&'static str>,
) -> &'static MethodDescriptor {
    method_descriptor_with_retry::<A, R>(
        service_name,
        method_name,
        arg_names,
        doc,
        RetryPolicy::VOLATILE,
    )
}

/// Build and leak a `MethodDescriptor` with an explicit retry policy.
pub fn method_descriptor_with_retry<'a, 'r, A: Facet<'a>, R: Facet<'r>>(
    service_name: &'static str,
    method_name: &'static str,
    arg_names: &[&'static str],
    doc: Option<&'static str>,
    retry: RetryPolicy,
) -> &'static MethodDescriptor {
    assert!(
        !shape_contains_channel(R::SHAPE),
        "channels are not allowed in return types: {service_name}.{method_name}"
    );
    let args_have_channels = shape_contains_channel(A::SHAPE);
    assert!(
        !(retry.persist && args_have_channels),
        "persist methods cannot carry channels: {service_name}.{method_name}"
    );

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

    let args: &'static [ArgDescriptor] = Box::leak(
        arg_names
            .iter()
            .zip(arg_shapes.iter())
            .map(|(&name, &shape)| ArgDescriptor { name, shape })
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
        args_have_channels,
        retry,
        doc,
    }))
}

pub fn shape_contains_channel(shape: &'static Shape) -> bool {
    fn visit(shape: &'static Shape, seen: &mut HashSet<&'static Shape>) -> bool {
        if is_tx(shape) || is_rx(shape) {
            return true;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_id_name_only_is_stable_across_case_variations() {
        let a = method_id_name_only("MyService", "DoThingFast");
        let b = method_id_name_only("my-service", "do-thing-fast");
        let c = method_id_name_only("MY_SERVICE", "DO_THING_FAST");
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn method_id_name_only_different_methods_produce_different_ids() {
        let a = method_id_name_only("Svc", "alpha");
        let b = method_id_name_only("Svc", "beta");
        assert_ne!(a, b);
    }
}
