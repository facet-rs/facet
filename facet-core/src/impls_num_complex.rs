use crate::*;
use core::{fmt, mem::offset_of};
use num_complex::Complex;

unsafe impl<'facet, T: Facet<'facet>> Facet<'facet> for Complex<T> {
    const SHAPE: &'static Shape = &const {
        fn type_name_fn<'facet, T: Facet<'facet>>(
            f: &mut core::fmt::Formatter,
            opts: TypeNameOpts,
        ) -> core::fmt::Result {
            f.write_str("Complex<")?;
            if let Some(opts) = opts.for_children() {
                (T::SHAPE.vtable.type_name)(f, opts)?;
            } else {
                f.write_str("…")?;
            }
            f.write_str(">")
        }

        ShapeBuilder::for_sized::<Complex<T>>(type_name_fn::<T>, "Complex")
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
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .doc(&["A complex number in Cartesian form"])
            .vtable(
                ValueVTable::builder(type_name_fn::<T>)
                    .drop_in_place(ValueVTable::drop_in_place_for::<Complex<T>>())
                    .display_opt({
                        if matches!(
                            T::SHAPE.ty,
                            Type::Primitive(PrimitiveType::Numeric(NumericType::Float))
                        ) && matches!(size_of::<T>(), 4 | 8)
                        {
                            Some(|ptr: PtrConst<'_>, f| {
                                if const {
                                    matches!(
                                        T::SHAPE.ty,
                                        Type::Primitive(PrimitiveType::Numeric(NumericType::Float))
                                    )
                                } {
                                    if const { size_of::<T>() == 4 } {
                                        assert_eq!(T::SHAPE, <f32 as Facet>::SHAPE);
                                        let ptr =
                                            unsafe { &*ptr.as_byte_ptr().cast::<Complex<f32>>() };
                                        fmt::Display::fmt(ptr, f)
                                    } else if const { size_of::<T>() == 8 } {
                                        assert_eq!(T::SHAPE, <f64 as Facet>::SHAPE);
                                        let ptr =
                                            unsafe { &*ptr.as_byte_ptr().cast::<Complex<f64>>() };
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
                    .debug_opt({
                        if T::SHAPE.vtable.has_debug() {
                            Some(|this, f| unsafe {
                                crate::shape_util::debug_struct(
                                    this,
                                    f.debug_struct("Complex"),
                                    complex_fields::<T>(),
                                )
                                .finish()
                            })
                        } else {
                            None
                        }
                    })
                    .default_in_place_opt({
                        if T::SHAPE.vtable.has_default_in_place() {
                            Some(|mem| unsafe {
                                let default = T::SHAPE.vtable.default_in_place.unwrap();

                                let re = mem.field_uninit_at(offset_of!(Complex<T>, re));
                                default(re);

                                let im = mem.field_uninit_at(offset_of!(Complex<T>, im));
                                default(im);

                                mem.assume_init()
                            })
                        } else {
                            None
                        }
                    })
                    .partial_eq_opt({
                        if T::SHAPE.vtable.has_partial_eq() {
                            Some(|l, r| unsafe {
                                crate::shape_util::partial_eq_fields(l, r, complex_fields::<T>())
                            })
                        } else {
                            None
                        }
                    })
                    .hash_opt({
                        if T::SHAPE.vtable.has_hash() {
                            Some(|this, hasher| unsafe {
                                crate::shape_util::hash_fields(this, complex_fields::<T>(), hasher)
                            })
                        } else {
                            None
                        }
                    })
                    .build(),
            )
            .build()
    };
}

const fn complex_fields<'facet, T: Facet<'facet>>() -> &'static [Field; 2] {
    &const {
        [
            Field {
                name: "re",
                shape: ShapeRef::Static(T::SHAPE),
                offset: offset_of!(Complex<T>, re),
                flags: FieldFlags::empty(),
                rename: None,
                alias: None,
                attributes: &[],
                doc: &["Real portion of the complex number"],
            },
            Field {
                name: "im",
                shape: ShapeRef::Static(T::SHAPE),
                offset: offset_of!(Complex<T>, im),
                flags: FieldFlags::empty(),
                rename: None,
                alias: None,
                attributes: &[],
                doc: &["Imaginary portion of the complex number"],
            },
        ]
    }
}
