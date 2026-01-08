#![cfg(feature = "num-complex")]

use crate::*;
use core::{fmt, mem::offset_of};
use num_complex::Complex;

// Named function for type_name
fn type_name_fn<'facet, T: Facet<'facet>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    f.write_str("Complex<")?;
    if let Some(opts) = opts.for_children() {
        T::SHAPE.write_type_name(f, opts)?;
    } else {
        f.write_str("…")?;
    }
    f.write_str(">")
}

// Named function for drop_in_place
unsafe fn drop_in_place<'facet, T: Facet<'facet>>(target: crate::OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(target.as_mut::<Complex<T>>());
    }
}

// Named function for display (for float types only)
unsafe fn display_f32(
    source: crate::OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let val = unsafe { source.get::<Complex<f32>>() };
    Some(fmt::Display::fmt(val, f))
}

unsafe fn display_f64(
    source: crate::OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let val = unsafe { source.get::<Complex<f64>>() };
    Some(fmt::Display::fmt(val, f))
}

// Named function for debug
unsafe fn debug<'facet, T: Facet<'facet>>(
    source: crate::OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    unsafe {
        let complex_ptr = source.ptr().as_byte_ptr() as *const Complex<T>;
        let re_ptr = PtrConst::new(core::ptr::addr_of!((*complex_ptr).re));
        let im_ptr = PtrConst::new(core::ptr::addr_of!((*complex_ptr).im));

        let mut debug_struct = f.debug_struct("Complex");

        if T::SHAPE.vtable.has_debug() {
            let re_ox = crate::OxRef::new(re_ptr, T::SHAPE);
            let im_ox = crate::OxRef::new(im_ptr, T::SHAPE);

            debug_struct.field("re", &re_ox);
            debug_struct.field("im", &im_ox);
        }

        Some(debug_struct.finish())
    }
}

// Named function for default_in_place
unsafe fn default_in_place<'facet, T: Facet<'facet>>(target: crate::OxPtrMut) {
    unsafe {
        let complex_ptr = target.ptr().as_mut_byte_ptr() as *mut Complex<T>;

        // Initialize `re` field
        let re_ptr = PtrMut::new(core::ptr::addr_of_mut!((*complex_ptr).re) as *mut u8);
        if let Some(()) = T::SHAPE.call_default_in_place(re_ptr) {
            // Initialize `im` field
            let im_ptr = PtrMut::new(core::ptr::addr_of_mut!((*complex_ptr).im) as *mut u8);
            let _ = T::SHAPE.call_default_in_place(im_ptr);
        }
    }
}

// Named function for partial_eq
unsafe fn partial_eq<'facet, T: Facet<'facet>>(
    a: crate::OxPtrConst,
    b: crate::OxPtrConst,
) -> Option<bool> {
    unsafe {
        let a_ptr = a.ptr().as_byte_ptr() as *const Complex<T>;
        let b_ptr = b.ptr().as_byte_ptr() as *const Complex<T>;

        let a_re_ptr = PtrConst::new(core::ptr::addr_of!((*a_ptr).re));
        let b_re_ptr = PtrConst::new(core::ptr::addr_of!((*b_ptr).re));

        let a_im_ptr = PtrConst::new(core::ptr::addr_of!((*a_ptr).im));
        let b_im_ptr = PtrConst::new(core::ptr::addr_of!((*b_ptr).im));

        let re_eq = T::SHAPE.call_partial_eq(a_re_ptr, b_re_ptr)?;
        let im_eq = T::SHAPE.call_partial_eq(a_im_ptr, b_im_ptr)?;

        Some(re_eq && im_eq)
    }
}

// Named function for hash
unsafe fn hash<'facet, T: Facet<'facet>>(
    source: crate::OxPtrConst,
    hasher: &mut HashProxy<'_>,
) -> Option<()> {
    unsafe {
        let ptr = source.ptr().as_byte_ptr() as *const Complex<T>;

        let re_ptr = PtrConst::new(core::ptr::addr_of!((*ptr).re));
        let im_ptr = PtrConst::new(core::ptr::addr_of!((*ptr).im));

        T::SHAPE.call_hash(re_ptr, hasher)?;
        T::SHAPE.call_hash(im_ptr, hasher)?;

        Some(())
    }
}

unsafe impl<'facet, T: Facet<'facet>> Facet<'facet> for Complex<T> {
    const SHAPE: &'static Shape = &const {
        const fn build_vtable<'facet, T: Facet<'facet>>() -> VTableIndirect {
            VTableIndirect {
                display: if const {
                    matches!(
                        T::SHAPE.ty,
                        Type::Primitive(PrimitiveType::Numeric(NumericType::Float))
                    ) && matches!(size_of::<T>(), 4 | 8)
                } {
                    if const { size_of::<T>() == 4 } {
                        Some(display_f32)
                    } else if const { size_of::<T>() == 8 } {
                        Some(display_f64)
                    } else {
                        None
                    }
                } else {
                    None
                },
                debug: if T::SHAPE.vtable.has_debug() {
                    Some(debug::<T>)
                } else {
                    None
                },
                partial_eq: if T::SHAPE.vtable.has_partial_eq() {
                    Some(partial_eq::<T>)
                } else {
                    None
                },
                hash: if T::SHAPE.vtable.has_hash() {
                    Some(hash::<T>)
                } else {
                    None
                },
                ..VTableIndirect::EMPTY
            }
        }

        const fn build_type_ops<'facet, T: Facet<'facet>>() -> TypeOpsIndirect {
            // Add default_in_place if T has default_in_place
            let has_default = match T::SHAPE.type_ops {
                Some(ops) => ops.has_default_in_place(),
                None => false,
            };

            TypeOpsIndirect {
                drop_in_place: drop_in_place::<T>,
                default_in_place: if has_default {
                    Some(default_in_place::<T>)
                } else {
                    None
                },
                clone_into: None,
                is_truthy: None,
            }
        }

        ShapeBuilder::for_sized::<Complex<T>>("Complex")
            .module_path("num_complex")
            .type_name(type_name_fn::<T>)
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
            .vtable_indirect(&const { build_vtable::<T>() })
            .type_ops_indirect(&const { build_type_ops::<T>() })
            .build()
    };
}

const fn complex_fields<'facet, T: Facet<'facet>>() -> &'static [Field; 2] {
    &const {
        [
            FieldBuilder::new("re", crate::shape_of::<T>, offset_of!(Complex<T>, re))
                .doc(&["Real portion of the complex number"])
                .build(),
            FieldBuilder::new("im", crate::shape_of::<T>, offset_of!(Complex<T>, im))
                .doc(&["Imaginary portion of the complex number"])
                .build(),
        ]
    }
}
