use facet::Facet;
use roam_postcard::error::DeserializeError;
use roam_postcard::plan::{PlanInput, SchemaSet, TranslationPlan, build_plan};
use roam_types::{BindingDirection, MethodId, SchemaRecvTracker, extract_schemas};

/// Deserialize args from a request (caller → callee direction).
// r[impl schema.exchange.required]
pub fn schema_deserialize_args_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let resolved = resolve_plan::<T>(method_id, BindingDirection::Args, schema_tracker_any)?;
    roam_postcard::from_slice_borrowed_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
}

/// Deserialize a response (callee → caller direction), borrowed variant.
// r[impl schema.exchange.required]
pub fn schema_deserialize_response_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let resolved = resolve_plan::<T>(method_id, BindingDirection::Response, schema_tracker_any)?;
    roam_postcard::from_slice_borrowed_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
}

/// Deserialize a response (callee → caller direction), owned variant.
// r[impl schema.exchange.required]
pub fn schema_deserialize_response<T: Facet<'static>>(
    bytes: &[u8],
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<T, DeserializeError> {
    let resolved = resolve_plan::<T>(method_id, BindingDirection::Response, schema_tracker_any)?;
    roam_postcard::from_slice_with_plan(bytes, &resolved.plan, &resolved.remote.registry)
}

struct ResolvedPlan {
    plan: TranslationPlan,
    remote: SchemaSet,
}

fn resolve_plan<'facet, T: Facet<'facet>>(
    method_id: MethodId,
    direction: BindingDirection,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<ResolvedPlan, DeserializeError> {
    let tracker = schema_tracker_any
        .and_then(|a| a.downcast_ref::<SchemaRecvTracker>())
        .ok_or_else(|| {
            DeserializeError::protocol("no schema tracker available — protocol error")
        })?;

    let dir_name = match direction {
        BindingDirection::Args => "args",
        BindingDirection::Response => "response",
    };

    let remote_root_id = match direction {
        BindingDirection::Args => tracker.get_remote_args_root(method_id),
        BindingDirection::Response => tracker.get_remote_response_root(method_id),
    }
    .ok_or_else(|| {
        DeserializeError::protocol(&format!(
            "no remote {dir_name} schema received for method {method_id:?} — sender must send schemas before data"
        ))
    })?;

    let remote_root = tracker.get_received(&remote_root_id).ok_or_else(|| {
        DeserializeError::protocol(&format!(
            "remote root type ID {remote_root_id:?} not found in received schemas"
        ))
    })?;

    let remote = SchemaSet {
        root: remote_root,
        registry: tracker.received_registry(),
    };

    let local = SchemaSet::from_extracted(extract_schemas(T::SHAPE));

    let plan = build_plan(&PlanInput {
        remote: &remote,
        local: &local,
    })
    .map_err(|e| DeserializeError::protocol(&format!("translation plan failed: {e}")))?;

    Ok(ResolvedPlan { plan, remote })
}
