use core::ptr::NonNull;

use alloc::boxed::Box;

use bytes::{BufMut as _, Bytes, BytesMut};

use crate::{
    Def, Facet, IterVTable, ListDef, ListVTable, PtrConst, PtrMut, PtrUninit, Shape, Type,
    UserType, value_vtable,
};

type BytesIterator<'mem> = core::slice::Iter<'mem, u8>;

unsafe impl Facet<'_> for Bytes {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable({
                let mut vtable = value_vtable!(Bytes, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ));
                {
                    vtable.try_from = {
                        Some(
                            |source: PtrConst, source_shape: &Shape, target: PtrUninit| {
                                if source_shape.is_type::<BytesMut>() {
                                    let source = unsafe { source.read::<BytesMut>() };
                                    let bytes = source.freeze();
                                    Ok(unsafe { target.put(bytes) })
                                } else {
                                    Err(crate::TryFromError::UnsupportedSourceShape {
                                        src_shape: source_shape,
                                        expected: &[Bytes::SHAPE],
                                    })
                                }
                            },
                        )
                    };
                }

                vtable
            })
            .ty(Type::User(UserType::Opaque))
            .type_identifier("Bytes")
            .inner(BytesMut::SHAPE)
            .def(Def::List(
                ListDef::builder()
                    .vtable(
                        &const {
                            ListVTable::builder()
                                .len(|ptr| unsafe {
                                    let bytes = ptr.get::<Self>();
                                    bytes.len()
                                })
                                .get(|ptr, index| unsafe {
                                    let bytes = ptr.get::<Self>();
                                    let item = bytes.get(index)?;
                                    Some(PtrConst::new(item.into()))
                                })
                                .as_ptr(|ptr| unsafe {
                                    let bytes: &Self = ptr.get::<Self>();
                                    PtrConst::new(core::ptr::NonNull::new_unchecked(
                                        bytes.as_ptr() as *mut u8
                                    ))
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| unsafe {
                                            let bytes = ptr.get::<Self>();
                                            let iter: BytesIterator = bytes.iter();
                                            let iter_state = Box::new(iter);
                                            PtrMut::new(NonNull::new_unchecked(Box::into_raw(
                                                iter_state,
                                            )
                                                as *mut u8))
                                        })
                                        .next(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<BytesIterator<'_>>();
                                            state.next().map(|value| PtrConst::new(value.into()))
                                        })
                                        .next_back(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<BytesIterator<'_>>();
                                            state
                                                .next_back()
                                                .map(|value| PtrConst::new(value.into()))
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<BytesIterator<'_>>()
                                                    as *mut BytesIterator<'_>,
                                            ));
                                        })
                                        .build(),
                                )
                                .build()
                        },
                    )
                    .t(u8::SHAPE)
                    .build(),
            ))
            .build()
    };
}

unsafe impl Facet<'_> for BytesMut {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable({
                value_vtable!(BytesMut, |f, _opts| write!(
                    f,
                    "{}",
                    Self::SHAPE.type_identifier
                ))
            })
            .type_identifier("BytesMut")
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
                                    let bytes = ptr.as_mut::<Self>();
                                    let item = item.read::<u8>();
                                    (*bytes).put_u8(item);
                                })
                                .len(|ptr| unsafe {
                                    let bytes = ptr.get::<Self>();
                                    bytes.len()
                                })
                                .get(|ptr, index| unsafe {
                                    let bytes = ptr.get::<Self>();
                                    let item = bytes.get(index)?;
                                    Some(PtrConst::new(item.into()))
                                })
                                .get_mut(|ptr, index| unsafe {
                                    let bytes = ptr.as_mut::<Self>();
                                    let item = bytes.get_mut(index)?;
                                    Some(PtrMut::new(item.into()))
                                })
                                .as_ptr(|ptr| unsafe {
                                    let bytes = ptr.get::<Self>();
                                    PtrConst::new(core::ptr::NonNull::new_unchecked(
                                        bytes.as_ptr() as *mut u8
                                    ))
                                })
                                .as_mut_ptr(|ptr| unsafe {
                                    let bytes = ptr.as_mut::<Self>();
                                    PtrMut::new(core::ptr::NonNull::new_unchecked(
                                        bytes.as_mut_ptr(),
                                    ))
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| unsafe {
                                            let bytes = ptr.get::<Self>();
                                            let iter: BytesIterator = bytes.iter();
                                            let iter_state = Box::new(iter);
                                            PtrMut::new(NonNull::new_unchecked(Box::into_raw(
                                                iter_state,
                                            )
                                                as *mut u8))
                                        })
                                        .next(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<BytesIterator<'_>>();
                                            state.next().map(|value| PtrConst::new(value.into()))
                                        })
                                        .next_back(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<BytesIterator<'_>>();
                                            state
                                                .next_back()
                                                .map(|value| PtrConst::new(value.into()))
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<BytesIterator<'_>>()
                                                    as *mut BytesIterator<'_>,
                                            ));
                                        })
                                        .build(),
                                )
                                .build()
                        },
                    )
                    .t(u8::SHAPE)
                    .build(),
            ))
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_ptr() {
        let bytes = Bytes::from(vec![0, 1, 2, 3, 4]);
        let expected = bytes.as_ptr();
        let Def::List(def) = Bytes::SHAPE.def else {
            panic!()
        };
        let actual =
            unsafe { (def.vtable.as_ptr).unwrap()(PtrConst::new((&bytes).into())) }.as_byte_ptr();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_as_ptr_mut() {
        let bytes = Bytes::from(vec![0, 1, 2, 3, 4]);
        let mut bytes = BytesMut::from(bytes);
        let expected = bytes.as_ptr();
        let Def::List(def) = BytesMut::SHAPE.def else {
            panic!()
        };
        let actual =
            unsafe { (def.vtable.as_ptr).unwrap()(PtrConst::new((&bytes).into())) }.as_byte_ptr();
        assert_eq!(expected, actual);

        let actual = unsafe { (def.vtable.as_mut_ptr).unwrap()(PtrMut::new((&mut bytes).into())) }
            .as_byte_ptr();
        assert_eq!(expected, actual);
    }
}
