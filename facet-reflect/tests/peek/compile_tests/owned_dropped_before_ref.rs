use facet::Facet;
use facet_reflect::{HasFields, Peek};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct NotDerivingFacet(u64);

// Proxy type that derives Facet
#[derive(Facet, Copy, Clone)]
pub struct NotDerivingFacetProxy(u64);

impl TryFrom<NotDerivingFacetProxy> for NotDerivingFacet {
    type Error = &'static str;
    fn try_from(val: NotDerivingFacetProxy) -> Result<Self, Self::Error> {
        Ok(NotDerivingFacet(val.0))
    }
}

impl TryFrom<&NotDerivingFacet> for NotDerivingFacetProxy {
    type Error = &'static str;
    fn try_from(val: &NotDerivingFacet) -> Result<Self, Self::Error> {
        Ok(NotDerivingFacetProxy(val.0))
    }
}

#[derive(Facet)]
pub struct Container {
    #[facet(opaque, proxy = NotDerivingFacetProxy)]
    inner: NotDerivingFacet,
}

fn main() {
    let container = Container {
        inner: NotDerivingFacet(35),
    };
    let peek_value = Peek::new(&container);
    let peek_struct = peek_value.into_struct().unwrap();
    for (field_item, peek) in peek_struct.fields_for_serialize() {
        let owned = peek
            .custom_serialization(field_item.field.unwrap())
            .unwrap();
        let peek = owned.as_peek();
        drop(owned);
        let proxy_value = peek.get::<NotDerivingFacetProxy>().unwrap();
    }
}
