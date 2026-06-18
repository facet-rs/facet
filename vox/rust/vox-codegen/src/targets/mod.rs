pub mod swift;
pub mod typescript;

use facet_core::Shape;
use vox_types::{ServiceDescriptor, ShapeKind, StructInfo, VariantKind, classify_shape};

pub(crate) fn validate_no_fd_shapes(service: &ServiceDescriptor, target: &str) {
    for method in service.methods {
        for arg in method.args {
            assert!(
                !shape_contains_fd(arg.shape),
                "{target} codegen does not support vox::Fd capabilities: {}.{} argument `{}` requires an fd-capable Rust/local transport",
                method.service_name,
                method.method_name,
                arg.name
            );
            if let Some(element) = arg.channel_element {
                assert!(
                    !shape_contains_fd(element),
                    "{target} codegen does not support vox::Fd channel items: {}.{} argument `{}` requires an fd-capable Rust/local transport",
                    method.service_name,
                    method.method_name,
                    arg.name
                );
            }
        }
        assert!(
            !shape_contains_fd(method.return_shape),
            "{target} codegen does not support vox::Fd capabilities: {}.{} return type requires an fd-capable Rust/local transport",
            method.service_name,
            method.method_name
        );
    }
}

fn shape_contains_fd(shape: &'static Shape) -> bool {
    let mut seen = std::collections::HashSet::new();
    shape_contains_fd_inner(shape, &mut seen)
}

fn shape_contains_fd_inner(
    shape: &'static Shape,
    seen: &mut std::collections::HashSet<usize>,
) -> bool {
    if vox_types::is_fd(shape) {
        return true;
    }
    if !seen.insert(shape as *const Shape as usize) {
        return false;
    }

    match classify_shape(shape) {
        ShapeKind::List { element }
        | ShapeKind::Slice { element }
        | ShapeKind::Option { inner: element }
        | ShapeKind::Array { element, .. }
        | ShapeKind::Set { element }
        | ShapeKind::Tx { inner: element }
        | ShapeKind::Rx { inner: element }
        | ShapeKind::Pointer { pointee: element } => shape_contains_fd_inner(element, seen),
        ShapeKind::Map { key, value } => {
            shape_contains_fd_inner(key, seen) || shape_contains_fd_inner(value, seen)
        }
        ShapeKind::Tuple { elements } => elements
            .iter()
            .any(|param| shape_contains_fd_inner(param.shape, seen)),
        ShapeKind::TupleStruct { fields } | ShapeKind::Struct(StructInfo { fields, .. }) => fields
            .iter()
            .any(|field| shape_contains_fd_inner(field.shape(), seen)),
        ShapeKind::Enum(info) => {
            info.variants
                .iter()
                .any(|variant| match vox_types::classify_variant(variant) {
                    VariantKind::Unit => false,
                    VariantKind::Newtype { inner } => shape_contains_fd_inner(inner, seen),
                    VariantKind::Tuple { fields } | VariantKind::Struct { fields } => fields
                        .iter()
                        .any(|field| shape_contains_fd_inner(field.shape(), seen)),
                })
        }
        ShapeKind::Result { ok, err } => {
            shape_contains_fd_inner(ok, seen) || shape_contains_fd_inner(err, seen)
        }
        ShapeKind::Scalar(_) | ShapeKind::Opaque => false,
    }
}
