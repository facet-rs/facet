use crate::*;
use core::hash::Hash as _;

use alloc::vec::Vec;

unsafe impl<'a, T> Facet<'a> for Vec<T>
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .type_params(&[
                TypeParam {
                    name: "T",
                    shape: || T::SHAPE,
                }
            ])
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder::<Self>()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "Vec<")?;
                                (T::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, ">")
                            } else {
                                write!(f, "Vec<⋯>")
                            }
                        })
                        .default_in_place(|target| unsafe { target.put(Self::default()) })
                    // FIXME: THIS IS VERY WRONG
                        .clone_into(|src, dst| unsafe { dst.put(core::ptr::read(src)) });

                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            write!(f, "[")?;
                            for (i, item) in value.iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                (<VTableView<T>>::of().debug().unwrap())(
                                    item,
                                    f,
                                )?;
                            }
                            write!(f, "]")
                        });
                    }

                    if T::SHAPE.vtable.eq.is_some() {
                        builder = builder.eq(|a, b|  {
                            if a.len() != b.len() {
                                return false;
                            }
                            for (item_a, item_b) in a.iter().zip(b.iter()) {
                                if !(<VTableView<T>>::of().eq().unwrap())(
                                    item_a,
                                    item_b,
                                ) {
                                    return false;
                                }
                            }
                            true
                        });
                    }

                    if T::SHAPE.vtable.hash.is_some() {
                        builder = builder.hash(|vec, hasher_this, hasher_write_fn| unsafe {
                            use crate::HasherProxy;
                            let t_hash = <VTableView<T>>::of().hash().unwrap_unchecked();
                            let mut hasher = HasherProxy::new(hasher_this, hasher_write_fn);
                            vec.len().hash(&mut hasher);
                            for item in vec {
                                (t_hash)(item, hasher_this, hasher_write_fn);
                            }
                        });
                    }

                    let traits = MarkerTraits::SEND
                        .union(MarkerTraits::SYNC)
                        .union(MarkerTraits::EQ)
                        .union(MarkerTraits::UNPIN)
                        .intersection(T::SHAPE.vtable.marker_traits);
                    builder = builder.marker_traits(traits);

                    builder.build()
                },
            )
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
                                .get_item_ptr(|ptr, index| unsafe {
                                    let vec = ptr.get::<Self>();
                                    let len = vec.len();
                                    if index >= len {
                                        panic!(
                                            "Index out of bounds: the len is {len} but the index is {index}"
                                        );
                                    }
                                    PtrConst::new(vec.as_ptr().add(index))
                                })
                                .build()
                        },
                    )
                    .t(|| T::SHAPE)
                    .build(),
            ))
            .build()
    };
}
