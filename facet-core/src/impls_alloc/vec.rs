use core::ptr::NonNull;

use crate::shape_util::*;
use crate::*;

use alloc::boxed::Box;
use alloc::vec::Vec;

type VecIterator<'mem, T> = core::slice::Iter<'mem, T>;

unsafe impl<'a, T> Facet<'a> for Vec<T>
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable({
                vtable_builder_for_list::<T, Self>()
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "{}<", Self::SHAPE.type_identifier)?;
                            T::SHAPE.vtable.type_name()(f, opts)?;
                            write!(f, ">")
                        } else {
                            write!(f, "{}<…>", Self::SHAPE.type_identifier)
                        }
                    })
                    .default_in_place({
                        Some(|target| unsafe { target.put(Self::default()).into() })
                    })
                    .marker_traits({
                        MarkerTraits::SEND
                            .union(MarkerTraits::SYNC)
                            .union(MarkerTraits::EQ)
                            .union(MarkerTraits::UNPIN)
                            .union(MarkerTraits::UNWIND_SAFE)
                            .union(MarkerTraits::REF_UNWIND_SAFE)
                            .intersection(T::SHAPE.vtable.marker_traits())
                    })
                    .build()
            })
            .type_identifier("Vec")
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::List(
                ListDef::builder()
                    .vtable(
                        &const {
                            ListVTable::builder()
                                .init_in_place_with_capacity(|data, capacity| unsafe {
                                    data.put(Self::with_capacity(capacity))
                                })
                                .push(|ptr, item| unsafe {
                                    let vec = ptr.as_mut::<Self>();
                                    let item = item.read::<T>();
                                    (*vec).push(item);
                                })
                                .len(|ptr| unsafe {
                                    let vec = ptr.get::<Self>();
                                    vec.len()
                                })
                                .get(|ptr, index| unsafe {
                                    let vec = ptr.get::<Self>();
                                    let item = vec.get(index)?;
                                    Some(PtrConst::new(NonNull::from(item)))
                                })
                                .get_mut(|ptr, index| unsafe {
                                    let vec = ptr.as_mut::<Self>();
                                    let item = vec.get_mut(index)?;
                                    Some(PtrMut::new(NonNull::from(item)))
                                })
                                .as_ptr(|ptr| unsafe {
                                    let vec = ptr.get::<Self>();
                                    PtrConst::new(NonNull::new_unchecked(vec.as_ptr() as *mut T))
                                })
                                .as_mut_ptr(|ptr| unsafe {
                                    let vec = ptr.as_mut::<Self>();
                                    PtrMut::new(NonNull::new_unchecked(vec.as_mut_ptr()))
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| unsafe {
                                            let vec = ptr.get::<Self>();
                                            let iter: VecIterator<T> = vec.iter();
                                            let iter_state = Box::new(iter);
                                            PtrMut::new(NonNull::new_unchecked(Box::into_raw(
                                                iter_state,
                                            )
                                                as *mut u8))
                                        })
                                        .next(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<VecIterator<'_, T>>();
                                            state
                                                .next()
                                                .map(|value| PtrConst::new(NonNull::from(value)))
                                        })
                                        .next_back(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<VecIterator<'_, T>>();
                                            state
                                                .next_back()
                                                .map(|value| PtrConst::new(NonNull::from(value)))
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<VecIterator<'_, T>>()
                                                    as *mut VecIterator<'_, T>,
                                            ));
                                        })
                                        .build(),
                                )
                                .build()
                        },
                    )
                    .t(T::SHAPE)
                    .build(),
            ))
            .build()
    };
}
