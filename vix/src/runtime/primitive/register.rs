//! `PrimitiveSet` and the `register_function` typed adapter.
//!
//! `register_function` is the sugar path: it derives request/response schemas
//! from facet shapes, validates them into the lossless vir subset, and wraps a
//! plain `Fn(Req) -> Result<Resp, PrimitiveFailure>` as a [`Primitive`]. There
//! are no per-primitive match arms anywhere — every primitive is keyed by its
//! descriptor data (r[machine.primitive.registered]).
//!
//! Phase 02 lands registration ahead of the compiler manifest (phase 03) and the
//! scheduler (phase 05). Lookup/inspection accessors are exercised by unit tests
//! here; `dead_code` is expected until those consumers exist.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::runtime::identity::semantic_schema_id;

use super::bridge::vir_type_for;
use super::convert::{decode_value, intern_rust_value};
use super::descriptor::{
    MemoPolicy, PrimitiveDescriptor, PrimitiveId, PrimitiveName, RegisteredSchema, RegistrationError,
};
use super::traits::{
    Completion, EffectCtx, EffectProtocolError, EffectTicket, Primitive, PrimitiveFailure,
    RequestRef,
};

/// The version stamped on descriptors registered through the sugar path. The
/// full-control `register` path carries author-supplied versions.
const SUGAR_VERSION: u32 = 1;
const SUGAR_PROTOCOL: u32 = 1;

/// A name-keyed collection of registered primitives.
#[derive(Default)]
pub struct PrimitiveSet {
    entries: BTreeMap<String, Arc<dyn Primitive>>,
}

impl PrimitiveSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an already-built primitive. Rejects a duplicate name.
    pub(crate) fn register(
        &mut self,
        primitive: Arc<dyn Primitive>,
    ) -> Result<(), RegistrationError> {
        let name = primitive.descriptor().name.as_str().to_owned();
        if self.entries.contains_key(&name) {
            return Err(RegistrationError::DuplicateName { name });
        }
        self.entries.insert(name, primitive);
        Ok(())
    }

    /// Register a plain function as a primitive: derive its request/response
    /// schemas from `Req`/`Resp`, validate them, and wrap `f`.
    pub fn register_function<Resp, Req, F>(
        &mut self,
        name: &str,
        policy: MemoPolicy,
        f: F,
    ) -> Result<PrimitiveId, RegistrationError>
    where
        Req: facet::Facet<'static>,
        Resp: facet::Facet<'static>,
        F: Fn(Req) -> Result<Resp, PrimitiveFailure> + Send + Sync + 'static,
    {
        let name = PrimitiveName::new(name)?;
        let request = registered_schema::<Req>()?;
        let response = registered_schema::<Resp>()?;
        let id = PrimitiveId::derive(
            &name,
            SUGAR_VERSION,
            SUGAR_PROTOCOL,
            request.taxon_root,
            response.taxon_root,
        );
        let descriptor = PrimitiveDescriptor {
            id,
            name,
            version: SUGAR_VERSION,
            protocol: SUGAR_PROTOCOL,
            request,
            response,
            policy,
            capabilities: Vec::new(),
        };
        self.register(Arc::new(FunctionPrimitive {
            descriptor,
            call: f,
            _marker: PhantomData,
        }))?;
        Ok(id)
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Arc<dyn Primitive>> {
        self.entries.get(name)
    }

    pub(crate) fn by_id(&self, id: PrimitiveId) -> Option<&Arc<dyn Primitive>> {
        self.entries
            .values()
            .find(|primitive| primitive.descriptor().id == id)
    }

    /// The registered descriptors — the compiler manifest (phase 03).
    pub fn descriptors(&self) -> impl Iterator<Item = &PrimitiveDescriptor> {
        self.entries.values().map(|primitive| primitive.descriptor())
    }

    /// Project the registered descriptors into a compiler manifest — vir types
    /// and effect ids only, no handlers (r[machine.primitive.registered]). This
    /// is the `runtime -> compiler` boundary; `compiler` never imports `runtime`.
    #[must_use]
    pub fn compiler_manifest(&self) -> crate::compiler::PrimitiveManifest {
        let mut manifest = crate::compiler::PrimitiveManifest::new();
        for descriptor in self.descriptors() {
            manifest.insert(
                descriptor.name.as_str(),
                crate::compiler::PrimitiveSignature {
                    effect: descriptor.id.effect_id(),
                    request: descriptor.request.vix_type.clone(),
                    response: descriptor.response.vix_type.clone(),
                },
            );
        }
        manifest
    }
}

fn registered_schema<T: facet::Facet<'static>>() -> Result<RegisteredSchema, RegistrationError> {
    let derived = phon::derive::of_shape(T::SHAPE)
        .map_err(|error| RegistrationError::Derive { message: error.to_string() })?;
    let vix_type = vir_type_for(derived.root, &derived.schemas)?;
    let store_schema = semantic_schema_id(&vix_type);
    Ok(RegisteredSchema {
        taxon_root: derived.root,
        taxon_schemas: derived.schemas,
        vix_type,
        store_schema,
    })
}

struct FunctionPrimitive<Req, Resp, F> {
    descriptor: PrimitiveDescriptor,
    call: F,
    _marker: PhantomData<fn(Req) -> Resp>,
}

impl<Req, Resp, F> Primitive for FunctionPrimitive<Req, Resp, F>
where
    Req: facet::Facet<'static>,
    Resp: facet::Facet<'static>,
    F: Fn(Req) -> Result<Resp, PrimitiveFailure> + Send + Sync + 'static,
{
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(
        &self,
        request: RequestRef<'_>,
        ctx: &mut EffectCtx<'_>,
    ) -> Result<EffectTicket, EffectProtocolError> {
        // The compiler type-checked the call, so a request tree that does not
        // decode is a machine-plane protocol violation, not a language error.
        let decoded: Req = decode_value(request.frozen, &self.descriptor.request.vix_type)
            .map_err(|error| EffectProtocolError::RequestShape {
                message: error.to_string(),
            })?;
        match (self.call)(decoded) {
            Ok(response) => {
                let interned =
                    intern_rust_value(&response, &self.descriptor.response, ctx.store_mut())
                        .map_err(|error| EffectProtocolError::RequestShape {
                            message: error.to_string(),
                        })?;
                ctx.complete(Completion::Ok(interned));
            }
            Err(failure) => ctx.complete(Completion::Failed(failure)),
        }
        // Ticket ids become real in phase 05; the adapter's inline completion
        // makes the id inert here.
        Ok(EffectTicket(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::identity::{DemandKey, Digest};
    use crate::runtime::store::Store;

    #[derive(facet::Facet)]
    struct AddRequest {
        left: i64,
        right: i64,
    }

    #[derive(facet::Facet)]
    struct AddResponse {
        sum: i64,
    }

    fn test_demand_key() -> DemandKey {
        DemandKey(Digest([9u8; 32]))
    }

    #[test]
    fn register_and_invoke_round_trip() {
        let mut set = PrimitiveSet::new();
        let id = set
            .register_function::<AddResponse, AddRequest, _>(
                "add_numbers",
                MemoPolicy::Hermetic,
                |req: AddRequest| {
                    Ok(AddResponse {
                        sum: req.left + req.right,
                    })
                },
            )
            .unwrap();
        let primitive = set.by_id(id).unwrap().clone();
        let desc = primitive.descriptor();

        let mut store = Store::default();
        let req = AddRequest {
            left: 40,
            right: 2,
        };
        let interned = intern_rust_value(&req, &desc.request, &mut store).unwrap();
        let frozen = super::super::convert::encode_value(facet::Peek::new(&req), &desc.request.vix_type)
            .unwrap()
            .frozen;
        let response_type = desc.response.vix_type.clone();

        let mut ctx = EffectCtx::new(&mut store);
        primitive
            .begin(
                RequestRef {
                    identity: interned.identity,
                    frozen: &frozen,
                },
                &mut ctx,
            )
            .unwrap();
        let (completion, _receipt, _events) = ctx.finish(test_demand_key()).unwrap();
        let Completion::Ok(result) = completion else {
            panic!("expected ok")
        };
        let response_frozen = store
            .frozen_for(result.handle)
            .expect("response carries a frozen tree")
            .clone();
        let response: AddResponse = decode_value(&response_frozen, &response_type).unwrap();
        assert_eq!(response.sum, 42);
    }

    #[test]
    fn duplicate_and_unsupported_registrations_fail() {
        let mut set = PrimitiveSet::new();
        set.register_function::<AddResponse, AddRequest, _>(
            "add_numbers",
            MemoPolicy::Volatile,
            |r| Ok(AddResponse { sum: r.left }),
        )
        .unwrap();
        assert!(matches!(
            set.register_function::<AddResponse, AddRequest, _>(
                "add_numbers",
                MemoPolicy::Volatile,
                |r| Ok(AddResponse { sum: r.left })
            ),
            Err(RegistrationError::DuplicateName { .. })
        ));

        #[derive(facet::Facet)]
        struct BadRequest {
            weight: f64,
        }
        assert!(matches!(
            set.register_function::<AddResponse, BadRequest, _>("bad", MemoPolicy::Volatile, |_| {
                Ok(AddResponse { sum: 0 })
            }),
            Err(RegistrationError::UnsupportedShape { .. })
        ));
    }
}
