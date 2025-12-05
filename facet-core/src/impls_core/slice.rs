use core::ptr::NonNull;

use crate::shape_util::vtable_for_list;
use crate::*;

unsafe impl<'a, T> Facet<'a> for [T]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_unsized::<Self>(
            |f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "[")?;
                    (T::SHAPE.vtable.type_name())(f, opts)?;
                    write!(f, "]")
                } else {
                    write!(f, "[…]")
                }
            },
            "[_]",
        )
        .vtable(ValueVTable {
            type_name: |f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "[")?;
                    (T::SHAPE.vtable.type_name())(f, opts)?;
                    write!(f, "]")
                } else {
                    write!(f, "[…]")
                }
            },
            ..vtable_for_list::<T, Self>()
        })
        .ty(Type::Sequence(SequenceType::Slice(SliceType {
            t: T::SHAPE,
        })))
        .def(Def::Slice(SliceDef::new(
            &const {
                SliceVTable {
                    len: |ptr| unsafe {
                        let slice = ptr.get::<[T]>();
                        slice.len()
                    },
                    as_ptr: |ptr| unsafe {
                        let slice = ptr.get::<[T]>();
                        PtrConst::new(NonNull::new_unchecked(slice.as_ptr() as *mut T))
                    },
                    as_mut_ptr: |ptr| unsafe {
                        let slice = ptr.as_mut::<[T]>();
                        PtrMut::new(NonNull::new_unchecked(slice.as_mut_ptr()))
                    },
                }
            },
            T::SHAPE,
        )))
        .type_params(&[TypeParam {
            name: "T",
            shape: T::SHAPE,
        }])
        .build()
    };
}
