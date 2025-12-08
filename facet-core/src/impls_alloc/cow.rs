use crate::Variance;
use crate::{
    Def, Facet, FormatVTable, PtrConst, Shape, Type, UserType, ValueVTable,
    shape_util::vtable_for_ptr,
};
use alloc::borrow::Cow;
use alloc::borrow::ToOwned;

unsafe impl<'a, T> Facet<'a> for Cow<'a, T>
where
    T: 'a + ?Sized + ToOwned,
    T: Facet<'a>,
    T::Owned: Facet<'a>,
{
    const SHAPE: &'static Shape = &Shape {
        id: Shape::id_of::<Self>(),
        layout: Shape::layout_of::<Self>(),
        vtable: ValueVTable {
            type_name: |f, opts| {
                write!(f, "Cow")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    (T::SHAPE.vtable.type_name())(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            },
            format: FormatVTable {
                display: if T::SHAPE.vtable.has_display() {
                    Some(|value, f| unsafe {
                        (T::SHAPE.vtable.format.display.unwrap())(
                            PtrConst::new::<T>(value.get::<Self>().as_ref().into()),
                            f,
                        )
                    })
                } else {
                    None
                },
                ..vtable_for_ptr::<T, Self>().format
            },
            default_in_place: if T::Owned::SHAPE.vtable.has_default_in_place() {
                Some(|dst| unsafe {
                    // Create default T::Owned and wrap in Cow::Owned
                    let owned_uninit = T::Owned::SHAPE.allocate().unwrap();
                    let owned_ptr =
                        (T::Owned::SHAPE.vtable.default_in_place.unwrap())(owned_uninit);
                    let owned = owned_ptr.read::<T::Owned>();
                    let cow: Self = Cow::Owned(owned);
                    dst.put(cow)
                })
            } else {
                None
            },
            clone_into: if T::Owned::SHAPE.vtable.has_clone_into() {
                Some(|src, dst| unsafe {
                    let cow_ref = src.get::<Self>();
                    let cloned = cow_ref.clone();
                    dst.put(cloned)
                })
            } else {
                None
            },
            ..vtable_for_ptr::<T, Self>()
        },
        ty: Type::User(UserType::Opaque),
        def: Def::Scalar,
        type_identifier: "Cow",
        type_params: &[],
        doc: &[],
        attributes: &[],
        type_tag: None,
        inner: None,
        proxy: None,
        variance: Variance::Invariant,
    };
}
