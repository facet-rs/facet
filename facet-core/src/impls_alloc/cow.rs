use crate::{
    Def, Facet, MarkerTraits, PtrConst, Shape, Type, TypedPtrConst, TypedPtrUninit, UserType,
    shape_util::vtable_builder_for_ptr,
};
use alloc::borrow::Cow;
use alloc::borrow::ToOwned;

unsafe impl<'a, T> Facet<'a> for Cow<'a, T>
where
    T: 'a + ?Sized + ToOwned,
    T: Facet<'a>,
    T::Owned: Facet<'a>,
{
    const SHAPE: &'static Shape = &Shape::builder_for_sized::<Self>()
        .vtable(
            vtable_builder_for_ptr::<T, Self>()
                .type_name(|f, opts| {
                    write!(f, "Cow")?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        (T::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                })
                .display(if T::SHAPE.vtable.has_display() {
                    Some(|value: TypedPtrConst<'_, Self>, f| unsafe {
                        (T::SHAPE.vtable.display.unwrap())(
                            PtrConst::new::<T>(value.get().as_ref().into()),
                            f,
                        )
                    })
                } else {
                    None
                })
                .default_in_place(if T::Owned::SHAPE.vtable.has_default_in_place() {
                    Some(|dst: TypedPtrUninit<'_, Self>| unsafe {
                        // Create default T::Owned and wrap in Cow::Owned
                        let owned_uninit = T::Owned::SHAPE.allocate().unwrap();
                        let owned_ptr =
                            (T::Owned::SHAPE.vtable.default_in_place.unwrap())(owned_uninit);
                        let owned = owned_ptr.read::<T::Owned>();
                        let cow = Cow::Owned(owned);
                        crate::TypedPtrMut::new(dst.put(cow))
                    })
                } else {
                    None
                })
                .clone_into(if T::Owned::SHAPE.vtable.has_clone_into() {
                    Some(
                        |src: TypedPtrConst<'_, Self>, dst: TypedPtrUninit<'_, Self>| unsafe {
                            let cow_ref = src.get();
                            let cloned = cow_ref.clone();
                            crate::TypedPtrMut::new(dst.put(cloned))
                        },
                    )
                } else {
                    None
                })
                .marker_traits({
                    let mut traits = MarkerTraits::empty();
                    if T::SHAPE.vtable.marker_traits().contains(MarkerTraits::EQ) {
                        traits = traits.union(MarkerTraits::EQ);
                    }
                    if T::SHAPE.vtable.marker_traits().contains(MarkerTraits::SEND)
                        && T::Owned::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::SEND)
                    {
                        traits = traits.union(MarkerTraits::SEND);
                    }
                    if T::SHAPE.vtable.marker_traits().contains(MarkerTraits::SYNC)
                        && T::Owned::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::SYNC)
                    {
                        traits = traits.union(MarkerTraits::SYNC);
                    }
                    if T::SHAPE
                        .vtable
                        .marker_traits()
                        .contains(MarkerTraits::UNPIN)
                        && T::Owned::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::UNPIN)
                    {
                        traits = traits.union(MarkerTraits::UNPIN);
                    }
                    if T::SHAPE
                        .vtable
                        .marker_traits()
                        .contains(MarkerTraits::UNWIND_SAFE)
                        && T::Owned::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::UNWIND_SAFE)
                    {
                        traits = traits.union(MarkerTraits::UNWIND_SAFE);
                    }
                    if T::SHAPE
                        .vtable
                        .marker_traits()
                        .contains(MarkerTraits::REF_UNWIND_SAFE)
                        && T::Owned::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::REF_UNWIND_SAFE)
                    {
                        traits = traits.union(MarkerTraits::REF_UNWIND_SAFE);
                    }
                    traits
                })
                .build(),
        )
        .def(Def::Scalar)
        .type_identifier("Cow")
        .ty(Type::User(UserType::Opaque))
        .build();
}
