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
                .display({
                    if matches!(
                        T::SHAPE.ty,
                        Type::Primitive(PrimitiveType::Numeric(NumericType::Float))
                    ) && matches!(size_of::<T>(), 4 | 8)
                    {
                        Some(|ptr, f| {
                            if const {
                                matches!(
                                    T::SHAPE.ty,
                                    Type::Primitive(PrimitiveType::Numeric(NumericType::Float))
                                )
                            } {
                                if const { size_of::<T>() == 4 } {
                                    assert_eq!(T::SHAPE, <f32 as Facet>::SHAPE);
                                    let ptr = unsafe { &*(ptr.as_ptr() as *const Complex<f32>) };
                                    fmt::Display::fmt(ptr, f)
                                } else if const { size_of::<T>() == 8 } {
                                    assert_eq!(T::SHAPE, <f64 as Facet>::SHAPE);
                                    let ptr = unsafe { &*(ptr.as_ptr() as *const Complex<f64>) };
                                    fmt::Display::fmt(ptr, f)
                                } else {
                                    unreachable!()
                                }
                            } else {
                                unreachable!()
                            }
                        })
                    } else {
                        None
                    }
                })
                .debug({
                    if T::SHAPE.vtable.has_debug() {
                        Some(|this, f| unsafe {
                            crate::shape_util::debug_struct(
                                this.into(),
                                f.debug_struct("Complex"),
                                complex_fields::<T>(),
                            )
                            .finish()
                        })
                    } else {
                        None
                    }
                })
                .partial_eq({
                    if T::SHAPE.vtable.has_partial_eq() {
                        Some(|l, r| unsafe {
                            crate::shape_util::partial_eq_fields(
                                l.into(),
                                r.into(),
                                complex_fields::<T>(),
                            )
                        })
                    } else {
                        None
                    }
                })
                .hash({
                    if T::SHAPE.vtable.has_hash() {
                        Some(|this, hasher| unsafe {
                            crate::shape_util::hash_fields(
                                this.into(),
                                complex_fields::<T>(),
                                hasher,
                            )
                        })
                    } else {
                        None
                    }
                })
                .default_in_place({
                    if T::SHAPE.vtable.has_default_in_place() {
                        Some(|mut mem| unsafe {
                            let default =
                                core::mem::transmute::<DefaultInPlaceFn, DefaultInPlaceFnTyped<T>>(
                                    T::SHAPE.vtable.default_in_place.unwrap(),
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
            shape: T::SHAPE,
        }])
        .ty(crate::Type::User(crate::UserType::Struct(
            crate::StructType {
                repr: crate::Repr {
                    base: (crate::BaseRepr::C),
                    packed: false,
                },
                kind: crate::StructKind::Struct,
                fields: complex_fields::<T>(),
            },
        )))
        .def(crate::Def::Undefined)
        .doc(&["A complex number in Cartesian form"])
        .build();
}

const fn complex_fields<'facet, T: Facet<'facet>>() -> &'static [Field; 2] {
    &[
        Field {
            name: "re",
            shape: || T::SHAPE,
            offset: offset_of!(Complex<T>, re),
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
            shape: || T::SHAPE,
            offset: offset_of!(Complex<T>, im),
            flags: FieldFlags::EMPTY,
            attributes: &[],
            doc: &["Imaginary portion of the complex number"],
            vtable: &crate::FieldVTable {
                skip_serializing_if: None,
                default_fn: None,
            },
            flattened: false,
        },
    ]
}
