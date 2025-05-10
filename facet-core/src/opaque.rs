use crate::{Def, ScalarAffinity, ScalarDef, ValueVTable, value_vtable};
use crate::{Facet, Shape, Type, UserType};

/// Helper type for opaque members
#[repr(transparent)]
pub struct Opaque<T>(pub T);

unsafe impl<'a, T: 'a> Facet<'a> for Opaque<T> {
    // Since T is opaque and could be anything, we can't provide much functionality.
    // Using `()` for the vtable like PhantomData.
    const VTABLE: &'static ValueVTable =
        &const { value_vtable!((), |f, _opts| write!(f, "Opaque")) };

    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::opaque().build())
                    .build(),
            ))
            .build()
    };
}
