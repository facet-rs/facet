use facet::Facet;
use roam_postcard::error::DeserializeError;
use roam_postcard::plan::{PlanInput, SchemaSet, TranslationPlan, build_plan};
use roam_types::{BindingDirection, MethodId, Schema, SchemaRecvTracker, TypeRef, extract_schemas};

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
    roam_postcard::from_slice_borrowed_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
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
    roam_postcard::from_slice_borrowed_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
}

/// Deserialize a response (callee → caller direction), owned variant.
// r[impl schema.exchange.required]
pub fn schema_deserialize_response<T: Facet<'static>>(
    bytes: &[u8],
    method_id: MethodId,
    tracker: &SchemaRecvTracker,
) -> Result<T, DeserializeError> {
    let resolved = resolve_plan::<T>(method_id, BindingDirection::Response, tracker)?;
    roam_postcard::from_slice_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
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

    let local = SchemaSet::from_extracted(
        extract_schemas(T::SHAPE)
            .map_err(|e| DeserializeError::protocol(&format!("schema extraction failed: {e}")))?,
    );

    let plan = build_plan(&PlanInput {
        remote: &remote,
        local: &local,
    })
    .map_err(|e| DeserializeError::protocol(&format!("translation plan failed: {e}")))?;

    Ok(ResolvedPlan { plan, remote })
}
