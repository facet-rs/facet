//! Payload decode: build a compatibility decode program from the **writer's** schema
//! (received via the `schemas` binding) to the **reader's** type and decode through
//! phon's `lower_decode` compatibility path (`r[schema.exchange.required]`).
//!
//! There is no same-version shortcut: every args/response decode goes through a
//! compat [`DecodeProgram`], built once per (method,
//! direction, reader type) from the peer's schema closure and cached on the tracker.

use std::sync::Arc;

use facet::Facet;
use vox_phon::{DecodeProgram, Error};
use vox_types::schema::{PlanCacheKey, SchemaRecvTracker};
use vox_types::{BindingDirection, MethodId};

/// Deserialize args from a request (caller → callee direction).
// r[impl schema.exchange.required]
pub fn schema_deserialize_args_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    tracker: &SchemaRecvTracker,
) -> Result<T, Error>
where
    'input: 'facet,
{
    let program = resolve_program::<T>(method_id, BindingDirection::Args, tracker)?;
    vox_phon::decode_with_program::<T>(&program, bytes)
}

/// Deserialize a response (callee → caller direction), borrowed variant.
// r[impl schema.exchange.required]
pub fn schema_deserialize_response_borrowed<'input, 'facet, T: Facet<'facet>>(
    bytes: &'input [u8],
    method_id: MethodId,
    tracker: &SchemaRecvTracker,
) -> Result<T, Error>
where
    'input: 'facet,
{
    let program = resolve_program::<T>(method_id, BindingDirection::Response, tracker)?;
    vox_phon::decode_with_program::<T>(&program, bytes)
}

/// Deserialize a response (callee → caller direction), owned variant.
// r[impl schema.exchange.required]
pub fn schema_deserialize_response<T: Facet<'static>>(
    bytes: &[u8],
    method_id: MethodId,
    tracker: &SchemaRecvTracker,
) -> Result<T, Error> {
    let program = resolve_program::<T>(method_id, BindingDirection::Response, tracker)?;
    vox_phon::decode_owned_with_program::<T>(&program, bytes)
}

/// Resolve (and cache) the compat decode program for `T` against the peer's schema
/// closure for `(method_id, direction)`. Built once and reused for every message.
fn resolve_program<'facet, T: Facet<'facet>>(
    method_id: MethodId,
    direction: BindingDirection,
    tracker: &SchemaRecvTracker,
) -> Result<Arc<DecodeProgram>, Error> {
    let cache_key = PlanCacheKey {
        method_id,
        direction,
        local_shape: T::SHAPE,
    };

    if let Some(cached) = tracker.get_cached_plan::<DecodeProgram>(&cache_key) {
        return Ok(cached);
    }

    let dir_name = match direction {
        BindingDirection::Args => "args",
        BindingDirection::Response => "response",
    };
    let writer_bytes = tracker
        .writer_schema_bytes(method_id, direction)
        .ok_or_else(|| {
            Error(format!(
                "no remote {dir_name} schema received for method {method_id:?} — \
                 sender must send schemas before data"
            ))
        })?;
    let writer = vox_phon::parse_schema_bytes(&writer_bytes)?;
    let program = Arc::new(vox_phon::build_decode_program::<T>(&writer)?);
    tracker.insert_cached_plan(cache_key, Arc::clone(&program));
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_deserialize_args_uses_writer_and_reader_schema() {
        // The writer sends `((i32, String),)`; the reader decodes the same shape.
        let method_id = MethodId(1);
        let writer_bytes =
            vox_phon::schema_bytes::<((i32, String),)>().expect("writer schema bytes");
        let tracker = SchemaRecvTracker::new();
        tracker.record_received(method_id, BindingDirection::Args, writer_bytes);

        let bytes = vox_phon::to_vec(&((42i32, "hello".to_string()),)).expect("encode args");
        let decoded: ((i32, String),) =
            schema_deserialize_args_borrowed(&bytes, method_id, &tracker)
                .expect("schema deserialize args");
        assert_eq!(decoded, ((42, "hello".to_string()),));
    }

    // r[verify schema.exchange.required]
    #[test]
    fn schema_deserialize_args_requires_received_binding() {
        let err = schema_deserialize_args_borrowed::<(u32,)>(
            &vox_phon::to_vec(&(7_u32,)).expect("encode args"),
            MethodId(99),
            &SchemaRecvTracker::new(),
        )
        .expect_err("decode without a received schema binding should fail");

        assert!(
            err.to_string()
                .contains("sender must send schemas before data"),
            "unexpected missing-schema error: {err:?}"
        );
    }
}
