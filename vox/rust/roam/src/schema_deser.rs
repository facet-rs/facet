use facet::Facet;
use roam_postcard::error::DeserializeError;
use roam_postcard::plan::{TranslationPlan, build_plan};
use roam_schema::SchemaRegistry;
use roam_schema_extract::SchemaTracker;
use roam_types::MethodId;

/// Deserialize postcard bytes using a translation plan built from remote schemas.
///
/// The remote peer MUST have sent schemas before sending data. If schemas
/// are missing, this is a protocol error.
// r[impl schema.exchange.required]
pub fn schema_deserialize_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let (plan, registry) = resolve_plan::<T>(method_id, schema_tracker_any)?;
    roam_postcard::from_slice_borrowed_with_plan(bytes, &plan, &registry)
}

/// Owned variant for non-borrowed deserialization.
// r[impl schema.exchange.required]
pub fn schema_deserialize<T: Facet<'static>>(
    bytes: &[u8],
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<T, DeserializeError> {
    let (plan, registry) = resolve_plan::<T>(method_id, schema_tracker_any)?;
    roam_postcard::from_slice_with_plan(bytes, &plan, &registry)
}

fn resolve_plan<'facet, T: Facet<'facet>>(
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<(TranslationPlan, SchemaRegistry), DeserializeError> {
    let tracker = schema_tracker_any
        .and_then(|a| a.downcast_ref::<SchemaTracker>())
        .ok_or_else(|| {
            DeserializeError::protocol("no schema tracker available — protocol error")
        })?;

    let remote_root_id = tracker.get_remote_root(method_id).ok_or_else(|| {
        DeserializeError::protocol(&format!(
            "no remote schema received for method {method_id:?} — sender must send schemas before data"
        ))
    })?;

    let remote_schema = tracker.get_received(&remote_root_id).ok_or_else(|| {
        DeserializeError::protocol(&format!(
            "remote root type ID {remote_root_id:?} not found in received schemas"
        ))
    })?;

    let registry = tracker.received_registry();
    let local_shape = T::SHAPE;

    let plan = build_plan(&remote_schema, local_shape, &registry).map_err(|e| {
        eprintln!(
            "[schema_deser] translation plan FAILED for local={local_shape} method={method_id:?}"
        );
        eprintln!("[schema_deser]   remote_root={remote_root_id:?}");
        eprintln!("[schema_deser]   registry ({} entries):", registry.len());
        for (id, schema) in &registry {
            eprintln!("[schema_deser]     {id:?} => {:?}", schema.kind);
        }
        eprintln!("[schema_deser]   error: {e}");
        DeserializeError::protocol(&format!("translation plan failed: {e}"))
    })?;

    eprintln!(
        "[schema_deser] plan built OK for local={local_shape} method={method_id:?} root={remote_root_id:?}"
    );

    Ok((plan, registry))
}
