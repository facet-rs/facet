use facet::Facet;
use facet_reflect::{HasFields, Peek};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct NotDerivingFacet(u64);

fn serialize_with(val: &NotDerivingFacet) -> Result<u64, &'static str> {
    Ok(val.0)
}

#[derive(Facet)]
pub struct Container {
    #[facet(opaque, serialize_with=serialize_with)]
    inner: NotDerivingFacet,
}

fn main() {
    let container = Container {
        inner: NotDerivingFacet(35),
    };
    let peek_value = Peek::new(&container);
    let peek_struct = peek_value.into_struct().unwrap();
    for (field, peek) in peek_struct.fields_for_serialize() {
        let owned = peek.custom_serialization(field).unwrap();
        let peek = owned.as_peek();
        drop(owned);
        let proxy_value = peek.get::<u64>().unwrap();
    }
}
