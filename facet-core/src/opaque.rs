use crate::{Def, value_vtable};
use crate::{Facet, Shape, Type, UserType};

/// Helper type for opaque members
#[repr(transparent)]
pub struct Opaque<T: ?Sized>(pub T);

unsafe impl<'facet, T: 'facet> Facet<'facet> for Opaque<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("Opaque")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            // Since T is opaque and could be anything, we can't provide default_in_place here.
            // For fields with #[facet(default)], the grammar's `make_t or $ty::default()` syntax
            // generates the default function at compile time using the concrete field type.
            .vtable(value_vtable!((), |f, _opts| write!(
                f,
                "{}",
                Self::SHAPE.type_identifier
            )))
            .build()
    };
}
