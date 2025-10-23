use core::ptr::NonNull;

use crate::shape_util::*;
use crate::*;

unsafe impl<'a, T> Facet<'a> for [T]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_unsized::<Self>()
            .vtable({
                vtable_builder_for_list::<T, Self>()
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "[")?;
                            (T::SHAPE.vtable.type_name())(f, opts)?;
                            write!(f, "]")
                        } else {
                            write!(f, "[…]")
                        }
                    })
                    .marker_traits({
                        T::SHAPE
                            .vtable
                            .marker_traits()
                            .difference(MarkerTraits::COPY)
                    })
                    .build()
            })
            .type_identifier("[_]")
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .ty(Type::Sequence(SequenceType::Slice(SliceType {
                t: T::SHAPE,
            })))
            .def(Def::Slice(
                SliceDef::builder()
                    .vtable(
                        &const {
                            SliceVTable::builder()
                                .len(|ptr| unsafe {
                                    let slice = ptr.get::<[T]>();
                                    slice.len()
                                })
                                .as_ptr(|ptr| unsafe {
                                    let slice = ptr.get::<[T]>();
                                    PtrConst::new(NonNull::new_unchecked(slice.as_ptr() as *mut T))
                                })
                                .as_mut_ptr(|ptr| unsafe {
                                    let slice = ptr.as_mut::<[T]>();
                                    PtrMut::new(NonNull::new_unchecked(slice.as_mut_ptr()))
                                })
                                .build()
                        },
                    )
                    .t(T::SHAPE)
                    .build(),
            ))
            .build()
    };
}
