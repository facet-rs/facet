//! Test that opaque adapter RecvValue type mismatch points at generated adapter checks.

use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst};

#[derive(Facet)]
#[facet(opaque = BadAdapter)]
struct BadPayload;

struct BadAdapter;

impl FacetOpaqueAdapter for BadAdapter {
    type Error = String;
    type SendValue<'a> = BadPayload;
    type RecvValue<'de> = u32;

    fn serialize_map(_: &Self::SendValue<'_>) -> OpaqueSerialize {
        OpaqueSerialize::Mapped {
            ptr: PtrConst::new(&0u8 as *const u8),
            shape: <u8 as Facet>::SHAPE,
        }
    }

    fn deserialize_build<'de>(
        _: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        Ok(0)
    }
}

fn main() {}
