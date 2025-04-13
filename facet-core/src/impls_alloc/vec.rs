use crate::*;
use core::{alloc::Layout, hash::Hash as _};

use alloc::vec::Vec;

unsafe impl<T> Facet for Vec<T>
where
    T: Facet,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Vec<T>>())
            .layout(Layout::new::<Vec<T>>())
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder()
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "Vec<")?;
                                (T::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, ">")
                            } else {
                                write!(f, "Vec<⋯>")
                            }
                        })
                        .drop_in_place(|value| unsafe { value.drop_in_place::<Vec<T>>() })
                        .default_in_place(|target| unsafe { target.put(Self::default()) })
                        .clone_into(|src, dst| unsafe { dst.put(src.as_ref::<Vec<T>>()) });

                    if T::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            let value = unsafe { value.as_ref::<Vec<T>>() };
                            write!(f, "[")?;
                            for (i, item) in value.iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                unsafe {
                                    (T::SHAPE.vtable.debug.unwrap_unchecked())(
                                        OpaqueConst::new(item),
                                        f,
                                    )?;
                                }
                            }
                            write!(f, "]")
                        });
                    }

                    if T::SHAPE.vtable.eq.is_some() {
                        builder = builder.eq(|a, b| unsafe {
                            let a = a.as_ref::<Vec<T>>();
                            let b = b.as_ref::<Vec<T>>();
                            if a.len() != b.len() {
                                return false;
                            }
                            for (item_a, item_b) in a.iter().zip(b.iter()) {
                                if !(T::SHAPE.vtable.eq.unwrap_unchecked())(
                                    OpaqueConst::new(item_a),
                                    OpaqueConst::new(item_b),
                                ) {
                                    return false;
                                }
                            }
                            true
                        });
                    }

                    if T::SHAPE.vtable.hash.is_some() {
                        builder = builder.hash(|value, hasher_this, hasher_write_fn| unsafe {
                            use crate::HasherProxy;
                            let vec = value.as_ref::<Vec<T>>();
                            let t_hash = T::SHAPE.vtable.hash.unwrap_unchecked();
                            let mut hasher = HasherProxy::new(hasher_this, hasher_write_fn);
                            vec.len().hash(&mut hasher);
                            for item in vec {
                                (t_hash)(OpaqueConst::new(item), hasher_this, hasher_write_fn);
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
                            Ok(data.put(Self::with_capacity(capacity)))
                        })
                        .push(|ptr, item| unsafe {
                            let vec = ptr.as_mut::<Vec<T>>();
                            let item = item.read::<T>();
                            (*vec).push(item);
                        })
                        .len(|ptr| unsafe {
                            let vec = ptr.as_ref::<Vec<T>>();
                            vec.len()
                        })
                        .get_item_ptr(|ptr, index| unsafe {
                            let vec = ptr.as_ref::<Vec<T>>();
                            let len = vec.len();
                            if index >= len {
                                panic!(
                                    "Index out of bounds: the len is {len} but the index is {index}"
                                );
                            }
                            OpaqueConst::new(vec.as_ptr().add(index))
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
