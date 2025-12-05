use crate::{Def, value_vtable};
use crate::{Facet, Shape, Type, UserType};

/// Helper type for opaque members
#[repr(transparent)]
pub struct Opaque<T: ?Sized>(pub T);

unsafe impl<'facet, T: 'facet> Facet<'facet> for Opaque<T> {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: value_vtable!((), |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier)),
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "Opaque",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
        }
    };
}
