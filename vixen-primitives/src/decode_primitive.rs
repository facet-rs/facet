use vix::schema::SchemaPattern;
use vix::vir::{decode_primitive_id, decode_request_type};

use crate::rt::{
    EffectCtx, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveMemoPolicy, RawEffectTicket,
    RawPrimitive, ValueId, decode_request,
};

pub struct DecodePrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for DecodePrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: decode_primitive_id(),
                request_schema: SchemaPattern::exact(&decode_request_type().schema_ref()),
                response_schema: SchemaPattern::Var {
                    name: "Response".to_owned(),
                },
                failure_schema: SchemaPattern::Var {
                    name: "Failure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Hermetic,
                protocol_version: 1,
                capability_schemas: Vec::new(),
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for DecodePrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, _app: &Ctx) -> RawEffectTicket {
        // The document-decoding logic is language-level and lives in
        // `vix::runtime::decode_request`; this is just the RawPrimitive
        // registration wrapper (ticket + intern + finish) around it.
        let (ticket, completer) = ctx.ticket(|| {});
        let completion = decode_request(&request, &ctx)
            .and_then(|value| ctx.intern_value(value))
            .map(PrimitiveCompletion::Ok)
            .unwrap_or_else(PrimitiveCompletion::MachineError);
        let publication =
            ctx.finish(completion)
                .unwrap_or_else(|error| crate::rt::PrimitivePublication {
                    completion: PrimitiveCompletion::MachineError(error),
                    receipt: crate::rt::Receipt {
                        demand: ctx.demand(),
                        reads: Vec::new(),
                    },
                    journal: Vec::new(),
                    progressive: Vec::new(),
                });
        let _ = completer.complete(publication);
        ticket
    }
}
