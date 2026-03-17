use facet::Facet;
use roam_postcard::error::DeserializeError;
use roam_postcard::plan::{TranslationPlan, build_plan};
use roam_schema::SchemaRegistry;
use roam_schema_extract::SchemaTracker;
use roam_types::MethodId;

/// Deserialize postcard bytes using schema-aware translation plans.
///
/// If a schema tracker is available and has remote schemas for this method,
/// builds a translation plan that handles field reordering, unknown field
/// skipping, and default filling. Otherwise falls back to identity
/// deserialization (same types on both sides).
///
/// Used by macro-generated dispatcher and client code.
pub fn schema_deserialize_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    if let Some((plan, registry)) = resolve_plan::<T>(method_id, schema_tracker_any) {
        roam_postcard::from_slice_borrowed_with_plan(bytes, &plan, &registry)
    } else {
        roam_postcard::from_slice_borrowed(bytes)
    }
}

/// Owned variant for non-borrowed deserialization.
pub fn schema_deserialize<T: Facet<'static>>(
    bytes: &[u8],
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Result<T, DeserializeError> {
    if let Some((plan, registry)) = resolve_plan::<T>(method_id, schema_tracker_any) {
        roam_postcard::from_slice_with_plan(bytes, &plan, &registry)
    } else {
        roam_postcard::from_slice(bytes)
    }
}

fn resolve_plan<'facet, T: Facet<'facet>>(
    method_id: MethodId,
    schema_tracker_any: Option<&(dyn std::any::Any + Send + Sync)>,
) -> Option<(TranslationPlan, SchemaRegistry)> {
    let tracker = schema_tracker_any?.downcast_ref::<SchemaTracker>()?;
    let remote_root_id = tracker.get_remote_root(method_id)?;
    let remote_schema = tracker.get_received(&remote_root_id)?;
    let registry = tracker.received_registry();
    let local_shape = T::SHAPE;

    // Check if remote and local are the same type — skip plan building.
    let local_root_id = roam_schema::type_schema_id_of(local_shape);
    if remote_root_id == local_root_id {
        return None;
    }

    match build_plan(&remote_schema, local_shape, &registry) {
        Ok(plan) => Some((plan, registry)),
        Err(e) => {
            tracing::warn!(
                ?method_id,
                %e,
                "failed to build translation plan, falling back to identity"
            );
            None
        }
    }
}
