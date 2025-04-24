use core::{alloc::Layout, cmp::Ordering, fmt, mem};

use crate::{
    Characteristic, ConstTypeId, Def, Facet, Field, FieldFlags, MarkerTraits, PtrConst, Shape,
    Struct, TypeNameOpts, ValueVTable, shape_of,
};

#[inline(always)]
pub fn write_type_name_list(
    f: &mut fmt::Formatter<'_>,
    opts: TypeNameOpts,
    open: &'static str,
    delimiter: &'static str,
    close: &'static str,
    shapes: &'static [&'static Shape],
) -> fmt::Result {
    f.pad(open)?;
    if let Some(opts) = opts.for_children() {
        for (index, shape) in shapes.iter().enumerate() {
            if index > 0 {
                f.pad(delimiter)?;
            }
            shape.write_type_name(f, opts)?;
        }
    } else {
        write!(f, "⋯")?;
    }
    f.pad(close)?;
    Ok(())
}

macro_rules! impl_facet_for_tuple {
    // Used to implement the next bigger tuple type, by taking the next typename & associated index
    // out of `remaining`, if it exists.
    {
        continue from ($($elems:ident.$idx:tt,)+),
        remaining ()
    } => {};
    {
        continue from ($($elems:ident.$idx:tt,)+),
        remaining ($next:ident.$nextidx:tt, $($remaining:ident.$remainingidx:tt,)*)
    } => {
        impl_facet_for_tuple! {
            impl ($($elems.$idx,)+ $next.$nextidx,),
            remaining ($($remaining.$remainingidx,)*)
        }
    };
    // Handle commas correctly for the debug implementation
    { debug on $f:ident { $first:stmt; } } => {
        write!($f, "(")?;
        $first
        write!($f, ",)")
    };
    { debug on $f:ident { $first:stmt; $($stmt:stmt;)+ } } => {
        write!($f, "(")?;
        $first
        $(
            write!($f, ", ")?;
            $stmt
        )+
        write!($f, ")")
    };
    // Common structure of eq, partial_ord & ord
    { ord on ($($elems:ident.$idx:tt,)+), $cmp:ident($a:ident, $b:ident), eq = $eq:expr } => {{
        let a = unsafe { $a.get::<Self>() };
        let b = unsafe { $b.get::<Self>() };

        $(
            unsafe {
                let a_ptr = &a.$idx as *const $elems;
                let b_ptr = &b.$idx as *const $elems;

                let ordering = ($elems::SHAPE.vtable.$cmp.unwrap_unchecked())(
                    PtrConst::new(a_ptr),
                    PtrConst::new(b_ptr),
                );

                if ordering != $eq {
                    return ordering;
                }
            }
        )+

        $eq
    }};
    // Actually generate the trait implementation, and keep the remaining possible elements around
    {
        impl ($($elems:ident.$idx:tt,)+),
        remaining ($($remaining:ident.$remainingidx:tt,)*)
    } => {
        unsafe impl<'a $(, $elems)+> Facet<'a> for ($($elems,)+)
        where
            $($elems: Facet<'a>,)+
        {
            const SHAPE: &'static Shape = &const {
                Shape::builder()
                    .id(ConstTypeId::of::<Self>())
                    .layout(Layout::new::<Self>())
                    .vtable(&const {
                        let mut builder = ValueVTable::builder()
                            .type_name(|f, opts| {
                                write_type_name_list(f, opts, "(", ", ", ")", &[$($elems::SHAPE),+])
                            })
                            .drop_in_place(|data| unsafe { data.drop_in_place::<Self>() })
                            .marker_traits(
                                MarkerTraits::all()
                                    $(.intersection($elems::SHAPE.vtable.marker_traits))+
                            );

                        let elem_shapes = const { &[$($elems::SHAPE),+] };
                        if Characteristic::Debug.all(elem_shapes) {
                            builder = builder.debug(|value, f| {
                                let value = unsafe { value.get::<Self>() };

                                impl_facet_for_tuple! {
                                    debug on f {
                                        $(
                                            unsafe {
                                                let ptr = &value.$idx as *const $elems;
                                                ($elems::SHAPE.vtable.debug.unwrap_unchecked())(
                                                    PtrConst::new(ptr),
                                                    f,
                                                )
                                            }?;
                                        )+
                                    }
                                }
                            });
                        }

                        if Characteristic::Default.all(elem_shapes) {
                            builder = builder.default_in_place(|dst| {
                                $(
                                    unsafe {
                                        ($elems::SHAPE.vtable.default_in_place.unwrap_unchecked())(
                                            dst.field_uninit_at(mem::offset_of!(Self, $idx))
                                        );
                                    }
                                )+

                                unsafe { dst.assume_init() }
                            });
                        }

                        if Characteristic::Clone.all(elem_shapes) {
                             builder = builder.clone_into(|src, dst| {
                                $({
                                    let offset = mem::offset_of!(Self, $idx);
                                    unsafe {
                                        ($elems::SHAPE.vtable.clone_into.unwrap_unchecked())(
                                            src.field(offset),
                                            dst.field_uninit_at(offset),
                                        );
                                    }
                                })+

                                unsafe { dst.assume_init() }
                            });
                       }

                        if Characteristic::PartialEq.all(elem_shapes) {
                            builder = builder.eq(|a, b| impl_facet_for_tuple! {
                                ord on ($($elems.$idx,)+),
                                eq(a, b),
                                eq = true
                            });
                        }

                        if Characteristic::PartialOrd.all(elem_shapes) {
                            builder = builder.partial_ord(|a, b| impl_facet_for_tuple! {
                                ord on ($($elems.$idx,)+),
                                partial_ord(a, b),
                                eq = Some(Ordering::Equal)
                            });
                        }

                        if Characteristic::Ord.all(elem_shapes) {
                            builder = builder.ord(|a, b| impl_facet_for_tuple! {
                                ord on ($($elems.$idx,)+),
                                ord(a, b),
                                eq = Ordering::Equal
                            });
                        }

                        if Characteristic::Hash.all(elem_shapes) {
                            builder = builder.hash(|value, hasher_this, hasher_write_fn| {
                                let value = unsafe { value.get::<Self>() };

                                $(
                                    unsafe {
                                        let ptr = &value.$idx as *const $elems;

                                        ($elems::SHAPE.vtable.hash.unwrap_unchecked())(
                                            PtrConst::new(ptr),
                                            hasher_this,
                                            hasher_write_fn,
                                        );
                                    }
                                )+
                           });
                        }

                        builder.build()
                    })
                    .def(Def::Struct({
                        Struct::builder()
                            .tuple()
                            .fields(
                                &const {[
                                    $(
                                        Field::builder()
                                            .name(stringify!($idx))
                                            .shape(|| shape_of(&|t: &Self| &t.$idx))
                                            .offset(mem::offset_of!(Self, $idx))
                                            .flags(FieldFlags::EMPTY)
                                            .build(),
                                    )+
                                ]}
                            )
                            .build()
                    }))
                    .build()
            };
        }

        impl_facet_for_tuple! {
            continue from ($($elems.$idx,)+),
            remaining ($($remaining.$remainingidx,)*)
        }
    };
    // The entry point into this macro, all smaller tuple types get implemented as well.
    { ($first:ident.$firstidx:tt $(, $remaining:ident.$remainingidx:tt)* $(,)?) } => {
        impl_facet_for_tuple! {
            impl ($first.$firstidx,),
            remaining ($($remaining.$remainingidx,)*)
        }
    };
}

impl_facet_for_tuple! {
    (T0.0, T1.1, T2.2, T3.3, T4.4, T5.5, T6.6, T7.7, T8.8, T9.9, T10.10, T11.11)
}
