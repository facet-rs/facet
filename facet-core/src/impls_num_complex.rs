use crate::*;
use core::{fmt, mem::offset_of};
use num_complex::Complex;

unsafe impl<'facet, T: Facet<'facet>> Facet<'facet> for Complex<T> {
    const SHAPE: &'static Shape = &Shape::builder_for_sized::<Self>()
        .vtable(
            ValueVTable::builder::<Self>()
                .type_name(|f, opts| {
                    f.write_str("Complex<")?;
                    if let Some(opts) = opts.for_children() {
                        (T::SHAPE.vtable.type_name)(f, opts)?;
                    } else {
                        f.write_str("…")?;
                    }
                    f.write_str(">")
                })
                .display(|| {
                    if T::SHAPE == <f32 as Facet>::SHAPE || T::SHAPE == <f64 as Facet>::SHAPE {
                        Some(|ptr, f| {
                            if T::SHAPE == <f32 as Facet>::SHAPE {
                                let ptr = unsafe { &*(ptr.as_ptr() as *const Complex<f32>) };
                                fmt::Display::fmt(ptr, f)
                            } else if T::SHAPE == <f64 as Facet>::SHAPE {
                                let ptr = unsafe { &*(ptr.as_ptr() as *const Complex<f64>) };
                                fmt::Display::fmt(ptr, f)
                            } else {
                                unreachable!()
                            }
                        })
                    } else {
                        None
                    }
                })
                .partial_eq(|| {
                    if T::SHAPE.vtable.has_partial_eq() {
                        Some(|l, r| {
                            let partial_eq = unsafe {
                                core::mem::transmute::<PartialEqFn, PartialEqFnTyped<T>>(
                                    (T::SHAPE.vtable.partial_eq)().unwrap(),
                                )
                            };
                            let l = l.get();
                            let r = r.get();

                            partial_eq((&l.re).into(), (&r.re).into())
                                && partial_eq((&l.im).into(), (&r.im).into())
                        })
                    } else {
                        None
                    }
                })
                .hash(|| {
                    if T::SHAPE.vtable.has_hash() {
                        Some(|this, hasher| {
                            let hash = unsafe {
                                core::mem::transmute::<HashFn, HashFnTyped<T>>(
                                    (T::SHAPE.vtable.hash)().unwrap(),
                                )
                            };
                            let this = this.get();
                            hash((&this.re).into(), hasher);
                            hash((&this.im).into(), hasher);
                        })
                    } else {
                        None
                    }
                })
                .default_in_place(|| {
                    if T::SHAPE.vtable.has_default_in_place() {
                        Some(|mut mem| unsafe {
                            let default =
                                core::mem::transmute::<DefaultInPlaceFn, DefaultInPlaceFnTyped<T>>(
                                    (T::SHAPE.vtable.default_in_place)().unwrap(),
                                );

                            struct DropMem<T> {
                                mem: *mut T,
                            }
                            impl<T> Drop for DropMem<T> {
                                fn drop(&mut self) {
                                    unsafe { core::ptr::drop_in_place(self.mem) }
                                }
                            }

                            {
                                let re = mem.field_uninit_at::<T>(offset_of!(Self, re));
                                let re = default(re).as_ptr();
                                let re = DropMem { mem: re };

                                let im = mem.field_uninit_at::<T>(offset_of!(Self, im));
                                default(im);
                                core::mem::forget(re);
                            }

                            mem.assume_init().into()
                        })
                    } else {
                        None
                    }
                })
                .marker_traits(T::SHAPE.vtable.marker_traits)
                .build(),
        )
        .type_identifier("Complex")
        .type_params(&[crate::TypeParam {
            name: "T",
            shape: || T::SHAPE,
        }])
        .ty(crate::Type::User(crate::UserType::Struct(
            crate::StructType {
                repr: crate::Repr {
                    base: (crate::BaseRepr::C),
                    packed: false,
                },
                kind: crate::StructKind::Struct,
                fields: &[
                    Field {
                        name: "re",
                        shape: T::SHAPE,
                        offset: offset_of!(Self, re),
                        flags: FieldFlags::EMPTY,
                        attributes: &[],
                        doc: &["Real portion of the complex number"],
                        vtable: &crate::FieldVTable {
                            skip_serializing_if: None,
                            default_fn: None,
                        },
                        flattened: false,
                    },
                    Field {
                        name: "im",
                        shape: T::SHAPE,
                        offset: offset_of!(Self, im),
                        flags: FieldFlags::EMPTY,
                        attributes: &[],
                        doc: &["Imaginary portion of the complex number"],
                        vtable: &crate::FieldVTable {
                            skip_serializing_if: None,
                            default_fn: None,
                        },
                        flattened: false,
                    },
                ],
            },
        )))
        .def(crate::Def::Undefined)
        .doc(&["A complex number in Cartesian form"])
        .build();
}
