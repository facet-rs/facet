use facet::Facet;
use vox_postcard::error::DeserializeError;
use vox_postcard::plan::{PlanInput, SchemaSet, TranslationPlan, build_plan};
use vox_types::{BindingDirection, MethodId, Schema, SchemaRecvTracker, TypeRef, extract_schemas};

/// Deserialize args from a request (caller → callee direction).
// r[impl schema.exchange.required]
pub fn schema_deserialize_args_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    tracker: &SchemaRecvTracker,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let resolved = resolve_plan::<T>(method_id, BindingDirection::Args, tracker)?;
    vox_postcard::from_slice_borrowed_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
}

/// Deserialize a response (callee → caller direction), borrowed variant.
// r[impl schema.exchange.required]
pub fn schema_deserialize_response_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    tracker: &SchemaRecvTracker,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let resolved = resolve_plan::<T>(method_id, BindingDirection::Response, tracker)?;
    vox_postcard::from_slice_borrowed_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
}

/// Deserialize a response (callee → caller direction), owned variant.
// r[impl schema.exchange.required]
pub fn schema_deserialize_response<T: Facet<'static>>(
    bytes: &[u8],
    method_id: MethodId,
    tracker: &SchemaRecvTracker,
) -> Result<T, DeserializeError> {
    let resolved = resolve_plan::<T>(method_id, BindingDirection::Response, tracker)?;
    vox_postcard::from_slice_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
}

struct ResolvedPlan {
    plan: TranslationPlan,
    remote: SchemaSet,
}

fn resolve_plan<'facet, T: Facet<'facet>>(
    method_id: MethodId,
    direction: BindingDirection,
    tracker: &SchemaRecvTracker,
) -> Result<ResolvedPlan, DeserializeError> {
    let dir_name = match direction {
        BindingDirection::Args => "args",
        BindingDirection::Response => "response",
    };

    let remote_root_ref = match direction {
        BindingDirection::Args => tracker.get_remote_args_root(method_id),
        BindingDirection::Response => tracker.get_remote_response_root(method_id),
    }
    .ok_or_else(|| {
        DeserializeError::protocol(&format!(
            "no remote {dir_name} schema received for method {method_id:?} — sender must send schemas before data"
        ))
    })?;

    let registry = tracker.received_registry();
    let root_kind = remote_root_ref.resolve_kind(&registry).ok_or_else(|| {
        DeserializeError::protocol(&format!(
            "remote root type ref {remote_root_ref:?} not found in received schemas"
        ))
    })?;
    ::std::eprintln!(
        "[schema-deser] resolve_plan remote: method={:?} direction={:?} t={} root_ref={:?} root_kind={:?}",
        method_id,
        direction,
        ::std::any::type_name::<T>(),
        remote_root_ref,
        root_kind
    );
    let root_id = match &remote_root_ref {
        TypeRef::Concrete { type_id, .. } => *type_id,
        TypeRef::Var { .. } => {
            return Err(DeserializeError::protocol(
                "remote root type ref is a Var — protocol error",
            ));
        }
    };
    let remote = SchemaSet {
        root: Schema {
            id: root_id,
            type_params: vec![],
            kind: root_kind,
        },
        registry,
    };

    let local_extracted = extract_schemas(T::SHAPE)
        .map_err(|e| DeserializeError::protocol(&format!("schema extraction failed: {e}")))?;
    let local_root_ref = local_extracted.root.clone();
    let local = SchemaSet::from_root_and_schemas(local_extracted.root, local_extracted.schemas);
    ::std::eprintln!(
        "[schema-deser] resolve_plan local: method={:?} direction={:?} shape={} root_ref={:?} root_kind={:?}",
        method_id,
        direction,
        T::SHAPE,
        local_root_ref,
        local.root.kind
    );

    let plan = build_plan(&PlanInput {
        remote: &remote,
        local: &local,
    })
    .map_err(|e| DeserializeError::protocol(&format!("translation plan failed: {e}")))?;

    Ok(ResolvedPlan { plan, remote })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vox_types::SchemaPayload;

    #[test]
    fn schema_deserialize_args_handles_nested_unary_tuple() {
        let method_id = MethodId(1);
        let extracted =
            extract_schemas(<((i32, String),) as Facet>::SHAPE).expect("schema extraction");
        let tracker = SchemaRecvTracker::new();
        tracker
            .record_received(
                method_id,
                BindingDirection::Args,
                SchemaPayload {
                    schemas: extracted.schemas.clone(),
                    root: extracted.root.clone(),
                },
            )
            .expect("record received schemas");

        let bytes =
            vox_postcard::to_vec(&((42i32, "hello".to_string()),)).expect("serialize tuple args");
        let decoded: ((i32, String),) =
            schema_deserialize_args_borrowed(&bytes, method_id, &tracker)
                .expect("schema deserialize args");

        assert_eq!(decoded, ((42, "hello".to_string()),));
    }
}
